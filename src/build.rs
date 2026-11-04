//! Building a pack. Run offline: it holds everything in memory, then writes one file.

use crate::normal::{
    normalise_for, normalise_place, postcode_key, strip_place_prefix, title, Country,
};
use crate::pack::{Error, MAGIC, SECTIONS, VERSION};
use fst::MapBuilder;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

/// One address to index.
#[derive(Debug, Clone)]
pub struct Record {
    pub street: String,
    pub number: String,
    pub locality: String,
    pub postcode: String,
    pub region: String,
    pub lat: f64,
    pub lon: f64,
}

#[derive(Default)]
struct Interner {
    ids: HashMap<String, u32>,
    list: Vec<String>,
}

impl Interner {
    fn id(&mut self, s: &str) -> u32 {
        if let Some(&i) = self.ids.get(s) {
            return i;
        }
        let i = self.list.len() as u32;
        self.ids.insert(s.to_string(), i);
        self.list.push(s.to_string());
        i
    }
}

/// Grid size for finding the nearest town, in degrees.
const CELL: f64 = 0.1;
/// How many cells out to look. A record further than this from any town stays unplaced.
const SEARCH_CELLS: i32 = 6;

/// An official town or suburb.
struct Town {
    name: String,
    region: String,
    lat: f64,
    lon: f64,
}

/// A record waiting for its nearest town.
struct Unplaced {
    street: u32,
    number: u32,
    postcode: u32,
    region: u32,
    lat: i32,
    lon: i32,
}

struct Row {
    street: u32,
    place: u32,
    number: u32,
    lat: i32,
    lon: i32,
}

/// Collects records, then writes a pack.
#[derive(Default)]
pub struct PackBuilder {
    streets: Interner,
    numbers: Interner,
    places: Interner,
    display: Vec<String>,
    rows: Vec<Row>,
    source: String,
    country: Country,
    towns: Vec<Town>,
    /// Official positions and populations, by place id.
    official: HashMap<u32, (f64, f64, u64)>,
    unplaced: Vec<Unplaced>,
    labels: Interner,
    /// Postcode positions, kept apart from places as there can be millions.
    postcodes: Vec<(String, i32, i32)>,
    /// Each street as written, by street id.
    street_names: Vec<String>,
}

impl PackBuilder {
    /// `source` names the data and its version, and is stored in the pack.
    pub fn new(source: &str) -> Self {
        Self::for_country(source, Country::Australia)
    }

    /// A builder for a country's address conventions.
    pub fn for_country(source: &str, country: Country) -> Self {
        Self {
            source: source.to_string(),
            country,
            ..Self::default()
        }
    }

    /// Adds an official town or suburb, which makes it searchable and gives it its
    /// proper position, whether or not any address names it.
    pub fn add_place(&mut self, name: &str, region: &str, lat: f64, lon: f64, population: u64) {
        let locality = normalise_place(name, self.country);
        if locality.is_empty() {
            return;
        }
        let id = self.place_id(&locality, name, "", region);
        let e = self.official.entry(id).or_insert((lat, lon, population));
        // Two places can share a key; the bigger one is the one people mean.
        if population >= e.2 {
            *e = (lat, lon, population);
        }
        self.towns.push(Town {
            name: name.to_string(),
            region: region.to_string(),
            lat,
            lon,
        });
    }

    /// Adds a postcode's position, so a search can resolve to it with no address.
    pub fn add_postcode(&mut self, code: &str, lat: f64, lon: f64) {
        let code = postcode_key(code, self.country);
        self.postcodes
            .push((code, (lat * 1e7).round() as i32, (lon * 1e7).round() as i32));
    }

    fn place_id(&mut self, locality: &str, display: &str, postcode: &str, region: &str) -> u32 {
        let key = format!("{locality}\t{postcode}\t{region}");
        let place = self.places.id(&key);
        if place as usize == self.display.len() {
            self.display.push(title(strip_place_prefix(display)));
        }
        place
    }

    /// Adds a named road with no addresses, so a search can still reach the street.
    pub fn add_road(&mut self, name: &str, lat: f64, lon: f64) {
        self.add(&Record {
            street: name.to_string(),
            number: String::new(),
            locality: String::new(),
            postcode: String::new(),
            region: String::new(),
            lat,
            lon,
        });
    }

    pub fn add(&mut self, r: &Record) {
        let street = normalise_for(&r.street, self.country);
        if street.is_empty() {
            return;
        }
        let street = self.streets.id(&street);
        if street as usize == self.street_names.len() {
            self.street_names.push(title(&r.street));
        }
        let number = self.numbers.id(&r.number.to_lowercase());
        let (lat, lon) = ((r.lat * 1e7).round() as i32, (r.lon * 1e7).round() as i32);
        let locality = normalise_place(&r.locality, self.country);
        if locality.is_empty() {
            // Placed at the end, once every town is known.
            let code = postcode_key(&r.postcode, self.country);
            let (postcode, region) = (self.labels.id(&code), self.labels.id(&r.region));
            self.unplaced.push(Unplaced {
                street,
                number,
                postcode,
                region,
                lat,
                lon,
            });
            return;
        }
        let code = postcode_key(&r.postcode, self.country);
        let place = self.place_id(&locality, &r.locality, &code, &r.region);
        self.rows.push(Row {
            street,
            place,
            number,
            lat,
            lon,
        });
    }

    /// Gives each record with no town the nearest one. This approximates a boundary
    /// lookup, so it can pick the neighbouring town near an edge.
    fn place_the_unplaced(&mut self) {
        let towns = std::mem::take(&mut self.towns);
        let cell = |v: f64| (v / CELL).floor() as i32;
        let mut grid: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        for (i, t) in towns.iter().enumerate() {
            grid.entry((cell(t.lat), cell(t.lon)))
                .or_default()
                .push(i as u32);
        }
        for u in std::mem::take(&mut self.unplaced) {
            let (lat, lon) = (f64::from(u.lat) / 1e7, f64::from(u.lon) / 1e7);
            let (cy, cx) = (cell(lat), cell(lon));
            let mut best: Option<(f64, u32)> = None;
            for ring in 0..=SEARCH_CELLS {
                for dy in -ring..=ring {
                    for dx in -ring..=ring {
                        if dy.abs() != ring && dx.abs() != ring {
                            continue;
                        }
                        for &i in grid.get(&(cy + dy, cx + dx)).into_iter().flatten() {
                            let t = &towns[i as usize];
                            let d = (t.lat - lat).powi(2)
                                + ((t.lon - lon) * lat.to_radians().cos()).powi(2);
                            if best.is_none_or(|b| d < b.0) {
                                best = Some((d, i));
                            }
                        }
                    }
                }
                // One ring past a hit covers anything nearer across a cell corner.
                if best.is_some() && ring >= 1 {
                    break;
                }
            }
            let postcode = self.labels.list[u.postcode as usize].clone();
            let given = self.labels.list[u.region as usize].clone();
            let (name, region) = match best {
                Some((_, i)) => (
                    towns[i as usize].name.clone(),
                    towns[i as usize].region.clone(),
                ),
                None => (String::new(), given),
            };
            let locality = normalise_place(&name, self.country);
            let place = self.place_id(&locality, &name, &postcode, &region);
            self.rows.push(Row {
                street: u.street,
                place,
                number: u.number,
                lat: u.lat,
                lon: u.lon,
            });
        }
    }

    /// Writes the pack and returns the number of addresses kept.
    pub fn finish(mut self, path: impl AsRef<Path>) -> Result<usize, Error> {
        self.place_the_unplaced();
        // Street ids become ranks in sorted key order, as the FST requires.
        let mut order: Vec<u32> = (0..self.streets.list.len() as u32).collect();
        order.sort_unstable_by(|a, b| {
            self.streets.list[*a as usize].cmp(&self.streets.list[*b as usize])
        });
        let mut rank = vec![0u32; order.len()];
        for (r, id) in order.iter().enumerate() {
            rank[*id as usize] = r as u32;
        }
        let numeric = |id: u32, list: &[String]| -> u64 {
            list[id as usize]
                .chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
                .parse()
                .unwrap_or(0)
        };
        let numbers = std::mem::take(&mut self.numbers.list);
        self.rows.sort_unstable_by_key(|r| {
            (
                rank[r.street as usize],
                r.place,
                numeric(r.number, &numbers),
                r.number,
            )
        });
        // Units in one building share a street number: keep one, as this resolves to the building.
        self.rows.dedup_by_key(|r| (r.street, r.place, r.number));

        let mut fst = MapBuilder::memory();
        for (r, id) in order.iter().enumerate() {
            fst.insert(&self.streets.list[*id as usize], r as u64)
                .map_err(|e| Error::Build(e.to_string()))?;
        }
        let fst = fst.into_inner().map_err(|e| Error::Build(e.to_string()))?;

        let mut groups = Vec::with_capacity((order.len() + 1) * 4);
        let mut records = Vec::with_capacity(self.rows.len() * 16);
        let mut sums = vec![(0f64, 0f64, 0u32); self.places.list.len()];
        let mut current = None;
        for (i, r) in self.rows.iter().enumerate() {
            let group = rank[r.street as usize];
            while current != Some(group) {
                let next = current.map_or(0, |c| c + 1);
                groups.extend_from_slice(&(i as u32).to_le_bytes());
                current = Some(next);
            }
            records.extend_from_slice(&r.lat.to_le_bytes());
            records.extend_from_slice(&r.lon.to_le_bytes());
            records.extend_from_slice(&r.number.to_le_bytes());
            records.extend_from_slice(&r.place.to_le_bytes());
            let s = &mut sums[r.place as usize];
            s.0 += f64::from(r.lat) / 1e7;
            s.1 += f64::from(r.lon) / 1e7;
            s.2 += 1;
        }
        groups.extend_from_slice(&(self.rows.len() as u32).to_le_bytes());

        let mut name_offsets = Vec::with_capacity((order.len() + 1) * 4);
        let mut name_text = Vec::new();
        for id in &order {
            name_offsets.extend_from_slice(&(name_text.len() as u32).to_le_bytes());
            name_text.extend_from_slice(self.street_names[*id as usize].as_bytes());
        }
        name_offsets.extend_from_slice(&(name_text.len() as u32).to_le_bytes());

        self.postcodes.sort_unstable();
        self.postcodes.dedup_by(|a, b| a.0 == b.0);
        let mut postcodes = MapBuilder::memory();
        for (code, lat, lon) in &self.postcodes {
            let packed = u64::from(*lat as u32) << 32 | u64::from(*lon as u32);
            postcodes
                .insert(code, packed)
                .map_err(|e| Error::Build(e.to_string()))?;
        }
        let postcodes = postcodes
            .into_inner()
            .map_err(|e| Error::Build(e.to_string()))?;

        let number_text = numbers.join("\n").into_bytes();
        let mut places = String::new();
        for (i, key) in self.places.list.iter().enumerate() {
            let (la, lo, n) = sums[i];
            let n = f64::from(n.max(1));
            let (lat, lon, population) =
                self.official
                    .get(&(i as u32))
                    .copied()
                    .unwrap_or((la / n, lo / n, 0));
            places.push_str(&format!(
                "{key}\t{}\t{lat:.7}\t{lon:.7}\t{population}\n",
                self.display[i]
            ));
        }
        let meta = format!(
            "source={}\ncountry={}\naddresses={}\n",
            self.source,
            self.country.code(),
            self.rows.len()
        );

        let sections: [&[u8]; SECTIONS] = [
            &fst,
            &groups,
            &records,
            &number_text,
            places.as_bytes(),
            meta.as_bytes(),
            &name_offsets,
            &name_text,
            &postcodes,
        ];
        let mut out = BufWriter::new(File::create(path)?);
        out.write_all(MAGIC)?;
        out.write_all(&VERSION.to_le_bytes())?;
        let mut offset = (MAGIC.len() + 4 + SECTIONS * 16) as u64;
        for s in sections {
            out.write_all(&offset.to_le_bytes())?;
            out.write_all(&(s.len() as u64).to_le_bytes())?;
            offset += s.len() as u64;
        }
        for s in sections {
            out.write_all(s)?;
        }
        out.flush()?;
        Ok(self.rows.len())
    }
}
