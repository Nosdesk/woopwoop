//! Building a pack. Run offline: it holds everything in memory, then writes one file.

use crate::normal::{normalise, title};
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
}

impl PackBuilder {
    /// `source` names the data and its version, and is stored in the pack.
    pub fn new(source: &str) -> Self {
        Self {
            source: source.to_string(),
            ..Self::default()
        }
    }

    pub fn add(&mut self, r: &Record) {
        let street = normalise(&r.street);
        if street.is_empty() {
            return;
        }
        let locality = normalise(&r.locality);
        let key = format!("{locality}\t{}\t{}", r.postcode, r.region);
        let place = self.places.id(&key);
        if place as usize == self.display.len() {
            self.display.push(title(&r.locality));
        }
        self.rows.push(Row {
            street: self.streets.id(&street),
            place,
            number: self.numbers.id(&r.number.to_lowercase()),
            lat: (r.lat * 1e7).round() as i32,
            lon: (r.lon * 1e7).round() as i32,
        });
    }

    /// Writes the pack and returns the number of addresses kept.
    pub fn finish(mut self, path: impl AsRef<Path>) -> Result<usize, Error> {
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

        let number_text = numbers.join("\n").into_bytes();
        let mut places = String::new();
        for (i, key) in self.places.list.iter().enumerate() {
            let (la, lo, n) = sums[i];
            let n = f64::from(n.max(1));
            places.push_str(&format!(
                "{key}\t{}\t{:.7}\t{:.7}\n",
                self.display[i],
                la / n,
                lo / n
            ));
        }
        let meta = format!("source={}\naddresses={}\n", self.source, self.rows.len());

        let sections: [&[u8]; SECTIONS] = [
            &fst,
            &groups,
            &records,
            &number_text,
            places.as_bytes(),
            meta.as_bytes(),
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
