//! A pack is one file holding a country's addresses. Opening it maps the file and
//! loads only the small tables; addresses stay on disk until a lookup touches them.

use crate::automaton::{edits, Fuzzy};
use crate::normal::{normalise, title};
use crate::parse::{parse, Address, Gazetteer, Parsed};
use fst::{IntoStreamer, Map, Streamer};
use memmap2::Mmap;
use std::collections::HashMap;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

pub(crate) const MAGIC: &[u8; 8] = b"WOOPWOOP";
pub(crate) const VERSION: u32 = 1;
pub(crate) const SECTIONS: usize = 6;

/// How far an unmatched suburb may be from where the query points and still count.
const NEARBY_METRES: f64 = 12_000.0;
const MAX_CANDIDATES: usize = 5;
const MAX_FUZZY_STREETS: usize = 64;
/// Interpolating across a wider gap than this is guesswork.
const MAX_NUMBER_GAP: i64 = 60;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    /// The file isn't a pack, or is damaged.
    Invalid(&'static str),
    /// The pack needs a newer version of this crate.
    Version(u32),
    Build(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Invalid(why) => write!(f, "invalid pack: {why}"),
            Self::Version(v) => write!(f, "unsupported pack version {v}"),
            Self::Build(e) => write!(f, "build failed: {e}"),
        }
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// How exact a result is. Anything coarser than `House` should be shown as approximate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Precision {
    House,
    Street,
    Locality,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub lat: f64,
    pub lon: f64,
    pub precision: Precision,
    pub label: String,
    pub locality: String,
    pub postcode: String,
    pub region: String,
    /// False when this is in a different suburb and postcode from the ones asked
    /// for. People mix up neighbouring suburbs, so it's worth offering, but say so.
    pub place_confirmed: bool,
}

/// Why a search has no result. Declining beats a confident wrong pin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Declined {
    /// A PO box or similar, which has no location of its own.
    PostalAddress,
    /// It read as an address, but no such place is in this pack.
    NotFound,
    /// It couldn't be read as an address.
    Unreadable,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Best first. More than one means the input was ambiguous: let a person choose.
    Found(Vec<Candidate>),
    Declined(Declined),
}

/// A structured lookup, for addresses already split into fields.
#[derive(Debug, Clone, Default)]
pub struct Query {
    pub number: Option<String>,
    pub street: String,
    pub locality: String,
    pub postcode: String,
    pub region: String,
}

struct Slice {
    map: Arc<Mmap>,
    start: usize,
    end: usize,
}

impl AsRef<[u8]> for Slice {
    fn as_ref(&self) -> &[u8] {
        &self.map[self.start..self.end]
    }
}

struct Place {
    name: String,
    display: String,
    postcode: String,
    region: String,
    lat: f64,
    lon: f64,
}

pub struct Pack {
    streets: Map<Slice>,
    groups: Slice,
    records: Slice,
    numbers: Vec<String>,
    numeric: Vec<i64>,
    places: Vec<Place>,
    by_locality: HashMap<String, Vec<u32>>,
    by_postcode: HashMap<String, Vec<u32>>,
    source: String,
}

/// The nearest listed house on one side of a missing number.
type Neighbour<'a> = Option<(i64, &'a Hit, usize)>;

struct Hit {
    place: u32,
    score: u8,
    number: u32,
    lat: f64,
    lon: f64,
}

fn distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (la, lb) = (a.0.to_radians(), b.0.to_radians());
    let dlat = (b.0 - a.0).to_radians() / 2.0;
    let dlon = (b.1 - a.1).to_radians() / 2.0;
    let x = dlat.sin().powi(2) + la.cos() * lb.cos() * dlon.sin().powi(2);
    12_742_000.0 * x.sqrt().asin()
}

fn leading_number(s: &str) -> Option<i64> {
    s.chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .ok()
}

impl Pack {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let file = File::open(path)?;
        // SAFETY: the pack is read-only and treated as untrusted bytes: every
        // section is bounds-checked here and read through checked slices. Replace
        // packs by writing a new file and renaming, never by editing one in place.
        let map = Arc::new(unsafe { Mmap::map(&file)? });
        if map.len() < 12 + SECTIONS * 16 || &map[..8] != MAGIC {
            return Err(Error::Invalid("not a WoopWoop pack"));
        }
        let word = |at: usize| u64::from_le_bytes(map[at..at + 8].try_into().unwrap_or([0; 8]));
        let version = u32::from_le_bytes(map[8..12].try_into().unwrap_or([0; 4]));
        if version != VERSION {
            return Err(Error::Version(version));
        }
        let section = |i: usize| -> Result<Slice, Error> {
            let at = 12 + i * 16;
            let (start, len) = (word(at) as usize, word(at + 8) as usize);
            let end = start
                .checked_add(len)
                .filter(|e| *e <= map.len())
                .ok_or(Error::Invalid("section out of bounds"))?;
            Ok(Slice {
                map: map.clone(),
                start,
                end,
            })
        };
        let streets = Map::new(section(0)?).map_err(|_| Error::Invalid("bad street index"))?;
        let groups = section(1)?;
        let records = section(2)?;
        if records.as_ref().len() % 16 != 0 || groups.as_ref().len() % 4 != 0 {
            return Err(Error::Invalid("bad record table"));
        }
        let text = |s: Slice| -> Result<String, Error> {
            std::str::from_utf8(s.as_ref())
                .map(str::to_string)
                .map_err(|_| Error::Invalid("bad text section"))
        };
        let numbers: Vec<String> = text(section(3)?)?.split('\n').map(str::to_string).collect();
        let numeric = numbers
            .iter()
            .map(|n| leading_number(n).unwrap_or(-1))
            .collect();
        let mut places = Vec::new();
        let (mut by_locality, mut by_postcode): (
            HashMap<String, Vec<u32>>,
            HashMap<String, Vec<u32>>,
        ) = Default::default();
        for line in text(section(4)?)?.lines() {
            let c: Vec<&str> = line.split('\t').collect();
            let [name, postcode, region, display, lat, lon] = c[..] else {
                return Err(Error::Invalid("bad place row"));
            };
            let id = places.len() as u32;
            by_locality.entry(name.to_string()).or_default().push(id);
            by_postcode
                .entry(postcode.to_string())
                .or_default()
                .push(id);
            places.push(Place {
                name: name.to_string(),
                display: display.to_string(),
                postcode: postcode.to_string(),
                region: region.to_string(),
                lat: lat.parse().map_err(|_| Error::Invalid("bad place row"))?,
                lon: lon.parse().map_err(|_| Error::Invalid("bad place row"))?,
            });
        }
        let source = text(section(5)?)?
            .lines()
            .find_map(|l| l.strip_prefix("source=").map(str::to_string))
            .unwrap_or_default();
        Ok(Self {
            streets,
            groups,
            records,
            numbers,
            numeric,
            places,
            by_locality,
            by_postcode,
            source,
        })
    }

    /// The data this pack was built from.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Number of addresses in the pack.
    pub fn len(&self) -> usize {
        self.records.as_ref().len() / 16
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn u32_at(slice: &Slice, i: usize) -> u32 {
        let b = slice.as_ref();
        u32::from_le_bytes([b[i * 4], b[i * 4 + 1], b[i * 4 + 2], b[i * 4 + 3]])
    }

    /// Searches free text, as a person typed it.
    pub fn search(&self, text: &str) -> Outcome {
        match parse(text, self) {
            Parsed::Postal => Outcome::Declined(Declined::PostalAddress),
            Parsed::Unknown => Outcome::Declined(Declined::Unreadable),
            Parsed::Address(a) => self.finish(self.address(&a)),
            Parsed::Area {
                locality,
                postcode,
                region,
            } => self.finish(self.area(&locality, &postcode, &region)),
            Parsed::Corner {
                first,
                second,
                locality,
                postcode,
                region,
            } => self.finish(self.corner(&first, &second, &locality, &postcode, &region)),
        }
    }

    /// Looks up an address already split into fields.
    pub fn lookup(&self, q: &Query) -> Outcome {
        let a = Address {
            number: q.number.as_ref().map(|n| n.to_lowercase()),
            street: normalise(&q.street),
            locality: normalise(&q.locality),
            postcode: q.postcode.clone(),
            region: q.region.to_uppercase(),
            confirmed: false,
        };
        self.finish(self.address(&a))
    }

    fn finish(&self, found: Vec<Candidate>) -> Outcome {
        if found.is_empty() {
            Outcome::Declined(Declined::NotFound)
        } else {
            Outcome::Found(found)
        }
    }

    fn matching_places(&self, locality: &str, postcode: &str, region: &str) -> Vec<u32> {
        let in_region = |id: &u32| region.is_empty() || self.places[*id as usize].region == region;
        let named: Vec<u32> = self
            .by_locality
            .get(locality)
            .map(|v| v.iter().copied().filter(in_region).collect())
            .unwrap_or_default();
        let exact: Vec<u32> = named
            .iter()
            .copied()
            .filter(|id| self.places[*id as usize].postcode == postcode)
            .collect();
        if !exact.is_empty() {
            return exact;
        }
        let coded: Vec<u32> = self
            .by_postcode
            .get(postcode)
            .map(|v| v.iter().copied().filter(in_region).collect())
            .unwrap_or_default();
        if !coded.is_empty() {
            return coded;
        }
        named
    }

    fn area(&self, locality: &str, postcode: &str, region: &str) -> Vec<Candidate> {
        self.matching_places(locality, postcode, region)
            .into_iter()
            .take(MAX_CANDIDATES)
            .map(|id| {
                let p = &self.places[id as usize];
                self.candidate(
                    id,
                    (p.lat, p.lon),
                    Precision::Locality,
                    format!("{} {} {}", p.display, p.region, p.postcode),
                )
            })
            .collect()
    }

    fn candidate(
        &self,
        place: u32,
        at: (f64, f64),
        precision: Precision,
        label: String,
    ) -> Candidate {
        let p = &self.places[place as usize];
        Candidate {
            lat: at.0,
            lon: at.1,
            precision,
            label,
            locality: p.display.clone(),
            postcode: p.postcode.clone(),
            region: p.region.clone(),
            place_confirmed: true,
        }
    }

    /// Every record on a street that could belong to the query, scored by how well
    /// its place agrees with what was asked.
    fn hits(
        &self,
        group: usize,
        locality: &str,
        postcode: &str,
        region: &str,
        anchors: &[(f64, f64)],
    ) -> Vec<Hit> {
        let (start, end) = (
            Self::u32_at(&self.groups, group) as usize,
            Self::u32_at(&self.groups, group + 1) as usize,
        );
        let asked = !locality.is_empty() || !postcode.is_empty();
        let mut out = Vec::new();
        for i in start..end {
            let place = Self::u32_at(&self.records, i * 4 + 3);
            let p = &self.places[place as usize];
            if !region.is_empty() && p.region != region {
                continue;
            }
            let lat = f64::from(Self::u32_at(&self.records, i * 4) as i32) / 1e7;
            let lon = f64::from(Self::u32_at(&self.records, i * 4 + 1) as i32) / 1e7;
            let score = u8::from(!locality.is_empty() && p.name == locality) * 2
                + u8::from(!postcode.is_empty() && p.postcode == postcode) * 2;
            if asked
                && score == 0
                && !anchors
                    .iter()
                    .any(|a| distance(*a, (lat, lon)) < NEARBY_METRES)
            {
                continue;
            }
            out.push(Hit {
                place,
                score,
                number: Self::u32_at(&self.records, i * 4 + 2),
                lat,
                lon,
            });
        }
        out
    }

    fn exact_street(&self, key: &str) -> Vec<(usize, String)> {
        self.streets
            .get(key)
            .map(|g| vec![(g as usize, key.to_string())])
            .unwrap_or_default()
    }

    /// Streets spelled like `key`, closest first. Excludes `key` itself.
    fn similar_streets(&self, key: &str) -> Vec<(usize, String)> {
        let length = key.chars().count();
        if length < 5 {
            return Vec::new();
        }
        let automaton = Fuzzy::new(key, if length < 9 { 1 } else { 2 });
        let mut stream = self.streets.search(automaton).into_stream();
        let mut out = Vec::new();
        while let Some((k, g)) = stream.next() {
            if k != key.as_bytes() {
                out.push((g as usize, String::from_utf8_lossy(k).into_owned()));
            }
            if out.len() >= MAX_FUZZY_STREETS {
                break;
            }
        }
        out.sort_by_key(|(_, name)| edits(key, name));
        out
    }

    fn address(&self, a: &Address) -> Vec<Candidate> {
        let mut found = self.resolve(a);
        if !a.locality.is_empty() || !a.postcode.is_empty() {
            for c in &mut found {
                c.place_confirmed = normalise(&c.locality) == a.locality
                    || (!a.postcode.is_empty() && c.postcode == a.postcode);
            }
        }
        found
    }

    fn resolve(&self, a: &Address) -> Vec<Candidate> {
        let places = self.matching_places(&a.locality, &a.postcode, &a.region);
        let anchors: Vec<(f64, f64)> = places
            .iter()
            .map(|id| (self.places[*id as usize].lat, self.places[*id as usize].lon))
            .collect();
        let mut hits: Vec<(Hit, usize)> = Vec::new();
        let mut groups = self.exact_street(&a.street);
        for (index, (group, _)) in groups.iter().enumerate() {
            hits.extend(
                self.hits(*group, &a.locality, &a.postcode, &a.region, &anchors)
                    .into_iter()
                    .map(|h| (h, index)),
            );
        }
        // A real street with nothing in the right place may be a typo for one that has.
        if !hits
            .iter()
            .any(|h| h.0.score > 0 || (a.locality.is_empty() && a.postcode.is_empty()))
        {
            let base = groups.len();
            groups.extend(self.similar_streets(&a.street));
            let mut tier = None;
            for (index, (group, name)) in groups.iter().enumerate().skip(base) {
                let distance = edits(&a.street, name);
                // Closest spelling first: a nearer street in the right place beats a further one.
                if tier.is_some_and(|t| t != distance) && hits.iter().any(|h| h.0.score > 0) {
                    break;
                }
                tier = Some(distance);
                // One guess at a time: a respelled street must be in the place asked for.
                let confirmed = self
                    .hits(*group, &a.locality, &a.postcode, &a.region, &anchors)
                    .into_iter()
                    .filter(|h| h.score > 0)
                    .map(|h| (h, index));
                hits.extend(confirmed);
            }
        }
        let best = hits.iter().map(|h| h.0.score).max();
        let Some(best) = best else {
            return self.area(&a.locality, &a.postcode, &a.region);
        };
        let top: Vec<&(Hit, usize)> = hits.iter().filter(|h| h.0.score == best).collect();
        let street_name = |index: usize| title(&groups[index].1);
        let tail = |place: u32| {
            let p = &self.places[place as usize];
            format!("{} {} {}", p.display, p.region, p.postcode)
        };

        if let Some(number) = &a.number {
            let mut exact: Vec<Candidate> = Vec::new();
            let mut seen = Vec::new();
            for (h, index) in &top {
                if &self.numbers[h.number as usize] == number && !seen.contains(&h.place) {
                    seen.push(h.place);
                    let label = format!(
                        "{} {}, {}",
                        number.to_uppercase(),
                        street_name(*index),
                        tail(h.place)
                    );
                    exact.push(self.candidate(h.place, (h.lat, h.lon), Precision::House, label));
                }
            }
            if !exact.is_empty() {
                exact.truncate(MAX_CANDIDATES);
                return exact;
            }
            // The number isn't listed: place it between its neighbours on the same side.
            if let Some(wanted) = leading_number(number) {
                let (mut below, mut above): (Neighbour, Neighbour) = (None, None);
                for (h, index) in &top {
                    let n = self.numeric[h.number as usize];
                    if n < 0 || n % 2 != wanted % 2 || top[0].0.place != h.place {
                        continue;
                    }
                    if n < wanted && below.as_ref().is_none_or(|b| n > b.0) {
                        below = Some((n, h, *index));
                    }
                    if n > wanted && above.as_ref().is_none_or(|b| n < b.0) {
                        above = Some((n, h, *index));
                    }
                }
                if let (Some(b), Some(u)) = (&below, &above) {
                    if u.0 - b.0 <= MAX_NUMBER_GAP {
                        let f = (wanted - b.0) as f64 / (u.0 - b.0) as f64;
                        let at = (
                            b.1.lat + (u.1.lat - b.1.lat) * f,
                            b.1.lon + (u.1.lon - b.1.lon) * f,
                        );
                        let label = format!(
                            "{} {}, {}",
                            number.to_uppercase(),
                            street_name(b.2),
                            tail(b.1.place)
                        );
                        return vec![self.candidate(b.1.place, at, Precision::Street, label)];
                    }
                }
            }
        }

        // The street exists but the house doesn't: point at the street, in each place.
        let mut out = Vec::new();
        let mut seen: Vec<u32> = Vec::new();
        for (h, index) in &top {
            if seen.contains(&h.place) {
                continue;
            }
            seen.push(h.place);
            let on: Vec<&&(Hit, usize)> = top.iter().filter(|x| x.0.place == h.place).collect();
            let n = on.len() as f64;
            let at = (
                on.iter().map(|x| x.0.lat).sum::<f64>() / n,
                on.iter().map(|x| x.0.lon).sum::<f64>() / n,
            );
            out.push(self.candidate(
                h.place,
                at,
                Precision::Street,
                format!("{}, {}", street_name(*index), tail(h.place)),
            ));
            if out.len() >= MAX_CANDIDATES {
                break;
            }
        }
        out
    }

    fn corner(
        &self,
        first: &str,
        second: &str,
        locality: &str,
        postcode: &str,
        region: &str,
    ) -> Vec<Candidate> {
        let places = self.matching_places(locality, postcode, region);
        let anchors: Vec<(f64, f64)> = places
            .iter()
            .map(|id| (self.places[*id as usize].lat, self.places[*id as usize].lon))
            .collect();
        let collect = |key: &str| -> Vec<Hit> {
            self.exact_street(key)
                .into_iter()
                .flat_map(|(g, _)| self.hits(g, locality, postcode, region, &anchors))
                .collect()
        };
        let (a, b) = (collect(first), collect(second));
        // Addresses mark a street, so the closest pair is about where the two meet.
        let mut best: Option<(f64, &Hit, &Hit)> = None;
        for x in a.iter().take(4000) {
            for y in b.iter().take(4000) {
                let d = distance((x.lat, x.lon), (y.lat, y.lon));
                if best.as_ref().is_none_or(|m| d < m.0) {
                    best = Some((d, x, y));
                }
            }
        }
        match best {
            Some((d, x, y)) if d < 500.0 => {
                let at = ((x.lat + y.lat) / 2.0, (x.lon + y.lon) / 2.0);
                let p = &self.places[x.place as usize];
                let label = format!(
                    "Corner of {} and {}, {} {} {}",
                    title(first),
                    title(second),
                    p.display,
                    p.region,
                    p.postcode
                );
                vec![self.candidate(x.place, at, Precision::Street, label)]
            }
            _ => self.area(locality, postcode, region),
        }
    }
}

impl Gazetteer for Pack {
    fn has_street(&self, key: &str) -> bool {
        self.streets.contains_key(key)
    }

    fn has_locality(&self, key: &str) -> bool {
        self.by_locality.contains_key(key)
    }
}
