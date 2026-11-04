//! A pack is one file holding a country's addresses. Opening it maps the file and
//! loads only the small tables; addresses stay on disk until a lookup touches them.

use crate::automaton::{edits, Fuzzy};
use crate::normal::{normalise_for, normalise_place, postcode_label, title, Country};
use crate::parse::{parse, Address, Gazetteer, Parsed};
use fst::{IntoStreamer, Map, Streamer};
use memmap2::Mmap;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

pub(crate) const MAGIC: &[u8; 8] = b"WOOPWOOP";
pub(crate) const VERSION: u32 = 5;
pub(crate) const SECTIONS: usize = 11;

/// How far an unmatched suburb may be from where the query points and still count.
const NEARBY_METRES: f64 = 12_000.0;
/// How many same-named towns another pack can offer one search.
const MAX_TOWNS: usize = 16;
/// Grid size for finding a town by position, in degrees.
const TOWN_CELL: f64 = 0.1;
/// How many cells out to look for a town.
const TOWN_CELLS: i32 = 6;
const MAX_CANDIDATES: usize = 5;
const MAX_FUZZY_STREETS: usize = 64;
const MAX_FUZZY_SCAN: usize = 2_000;
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
    /// Where this came from: the dataset for a listed house, otherwise the pack's source,
    /// as a street or town result draws on many records.
    pub source: String,
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

/// Searches several packs and returns the most precise answer, preferring earlier packs
/// on a tie. A region improves by adding a pack, and data under a different licence can
/// stay in a file of its own, as packs only meet here, at search time.
pub fn search_all(packs: &[Pack], text: &str) -> Outcome {
    let mut best: Option<Outcome> = None;
    let rank = |c: &[Candidate]| (!c[0].place_confirmed, c[0].precision);
    // For a town alone, a list of towns knows better than the middle of its addresses.
    let mut order: Vec<&Pack> = packs.iter().collect();
    if packs.iter().any(|p| p.reads_as_bare_town(text)) {
        order.sort_by_key(|p| !p.has_towns());
    }
    for pack in order {
        if matches!(&best, Some(Outcome::Found(c)) if c[0].precision == Precision::House && c[0].place_confirmed)
        {
            break;
        }
        let outcome = pack.search_near(text, &Elsewhere::for_search(packs, pack, text));
        best = Some(match (best.take(), outcome) {
            (None, o) => o,
            (Some(Outcome::Declined(Declined::PostalAddress)), _) => {
                return Outcome::Declined(Declined::PostalAddress)
            }
            (Some(Outcome::Found(a)), Outcome::Found(b)) => {
                Outcome::Found(if rank(&b) < rank(&a) { b } else { a })
            }
            (Some(Outcome::Found(a)), Outcome::Declined(_)) => Outcome::Found(a),
            (Some(Outcome::Declined(_)), o) => o,
        });
    }
    best.unwrap_or(Outcome::Declined(Declined::NotFound))
}

/// What other packs can say about the town asked for, for one search. A pack of addresses
/// with no town list uses this to place its records. The packs stay separate files, and
/// nothing from one is stored in another.
#[derive(Default)]
struct Elsewhere<'a> {
    /// Where the other packs put the town.
    anchors: Vec<(f64, f64)>,
    /// The packs holding a town list.
    towns: Vec<&'a Pack>,
}

impl<'a> Elsewhere<'a> {
    fn for_search(packs: &'a [Pack], pack: &Pack, text: &str) -> Self {
        let Parsed::Address(a) = parse(text, pack) else {
            return Self::default();
        };
        if a.locality.is_empty() {
            return Self::default();
        }
        let others = || packs.iter().filter(|p| !std::ptr::eq(*p, pack));
        Self {
            anchors: others()
                .flat_map(|p| p.town_points(&a.locality, &a.region))
                .collect(),
            towns: others().filter(|p| p.has_towns()).collect(),
        }
    }

    /// Whether `locality` is the nearest town to a position.
    fn is_nearest_town(&self, locality: &str, at: (f64, f64)) -> bool {
        self.towns
            .iter()
            .any(|p| p.nearest_town(at).is_some_and(|t| t.name == locality))
    }
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
    population: u64,
}

pub struct Pack {
    streets: Map<Slice>,
    groups: Slice,
    records: Slice,
    name_offsets: Slice,
    name_text: Slice,
    postcodes: Map<Slice>,
    origins: Slice,
    origin_names: Vec<String>,
    numbers: Vec<String>,
    numeric: Vec<i64>,
    places: Vec<Place>,
    /// Towns by grid cell, when this pack was built with a town list.
    town_grid: HashMap<(i32, i32), Vec<u32>>,
    by_locality: HashMap<String, Vec<u32>>,
    by_postcode: HashMap<String, Vec<u32>>,
    source: String,
    country: Country,
}

/// The nearest listed house on one side of a missing number.
type Neighbour<'a> = Option<(i64, &'a Hit, usize)>;

struct Hit {
    place: u32,
    score: u8,
    number: u32,
    origin: u16,
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

fn town_cell(v: f64) -> i32 {
    (v / TOWN_CELL).floor() as i32
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
        let name_offsets = section(6)?;
        let name_text = section(7)?;
        let postcodes = Map::new(section(8)?).map_err(|_| Error::Invalid("bad postcode index"))?;
        let records = section(2)?;
        if records.as_ref().len() % 16 != 0 || groups.as_ref().len() % 4 != 0 {
            return Err(Error::Invalid("bad record table"));
        }
        let origins = section(9)?;
        if !origins.as_ref().is_empty() && origins.as_ref().len() != records.as_ref().len() / 8 {
            return Err(Error::Invalid("bad origin table"));
        }
        let text = |s: Slice| -> Result<String, Error> {
            std::str::from_utf8(s.as_ref())
                .map(str::to_string)
                .map_err(|_| Error::Invalid("bad text section"))
        };
        let numbers: Vec<String> = text(section(3)?)?.split('\n').map(str::to_string).collect();
        let origin_names: Vec<String> = text(section(10)?)?
            .split('\n')
            .filter(|n| !n.is_empty())
            .map(str::to_string)
            .collect();
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
            let [name, postcode, region, display, lat, lon, population] = c[..] else {
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
                population: population
                    .parse()
                    .map_err(|_| Error::Invalid("bad place row"))?,
            });
        }
        let meta = text(section(5)?)?;
        let field = |name: &str| {
            meta.lines()
                .find_map(|l| l.strip_prefix(name).map(str::to_string))
        };
        let source = field("source=").unwrap_or_default();
        let mut town_grid: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        if field("towns=").is_some_and(|n| n != "0") {
            for (i, p) in places.iter().enumerate() {
                if p.postcode.is_empty() && !p.name.is_empty() {
                    town_grid
                        .entry((town_cell(p.lat), town_cell(p.lon)))
                        .or_default()
                        .push(i as u32);
                }
            }
        }
        let country = field("country=")
            .and_then(|c| Country::from_code(&c))
            .unwrap_or_default();
        Ok(Self {
            streets,
            groups,
            records,
            name_offsets,
            name_text,
            postcodes,
            origins,
            origin_names,
            numbers,
            numeric,
            places,
            town_grid,
            by_locality,
            by_postcode,
            source,
            country,
        })
    }

    /// A street as its data wrote it.
    fn street_name(&self, group: usize) -> String {
        let (start, end) = (
            Self::u32_at(&self.name_offsets, group) as usize,
            Self::u32_at(&self.name_offsets, group + 1) as usize,
        );
        self.name_text
            .as_ref()
            .get(start..end)
            .map(|b| String::from_utf8_lossy(b).into_owned())
            .unwrap_or_default()
    }

    /// The data this pack was built from.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Each dataset named when the pack was built, with how many records it supplied.
    /// Empty when the pack names none.
    pub fn datasets(&self) -> Vec<(&str, u64)> {
        let mut counts = vec![0u64; self.origin_names.len() + 1];
        for pair in self.origins.as_ref().chunks_exact(2) {
            let id = usize::from(u16::from_le_bytes([pair[0], pair[1]]));
            if let Some(n) = counts.get_mut(id) {
                *n += 1;
            }
        }
        self.origin_names
            .iter()
            .zip(&counts[1..])
            .map(|(name, n)| (name.as_str(), *n))
            .collect()
    }

    fn origin_at(&self, record: usize) -> u16 {
        self.origins
            .as_ref()
            .get(record * 2..record * 2 + 2)
            .map_or(0, |b| u16::from_le_bytes([b[0], b[1]]))
    }

    /// The dataset behind a record, falling back to the pack's source.
    fn origin_name(&self, origin: u16) -> &str {
        usize::from(origin)
            .checked_sub(1)
            .and_then(|i| self.origin_names.get(i))
            .map_or(&self.source, String::as_str)
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
        self.search_near(text, &Elsewhere::default())
    }

    fn has_towns(&self) -> bool {
        !self.town_grid.is_empty()
    }

    fn reads_as_bare_town(&self, text: &str) -> bool {
        matches!(parse(text, self), Parsed::Area { postcode, .. } if postcode.is_empty())
    }

    /// The closest town in this pack's town list.
    fn nearest_town(&self, at: (f64, f64)) -> Option<&Place> {
        let (cy, cx) = (town_cell(at.0), town_cell(at.1));
        let mut best: Option<(f64, u32)> = None;
        for ring in 0..=TOWN_CELLS {
            for dy in -ring..=ring {
                for dx in -ring..=ring {
                    if dy.abs() != ring && dx.abs() != ring {
                        continue;
                    }
                    for &i in self
                        .town_grid
                        .get(&(cy + dy, cx + dx))
                        .into_iter()
                        .flatten()
                    {
                        let t = &self.places[i as usize];
                        let d = (t.lat - at.0).powi(2)
                            + ((t.lon - at.1) * at.0.to_radians().cos()).powi(2);
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
        best.map(|b| &self.places[b.1 as usize])
    }

    fn search_near(&self, text: &str, elsewhere: &Elsewhere) -> Outcome {
        match parse(text, self) {
            Parsed::Postal => Outcome::Declined(Declined::PostalAddress),
            Parsed::Unknown => Outcome::Declined(Declined::Unreadable),
            Parsed::Address(a) => self.finish(self.address(&a, elsewhere)),
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
            street: normalise_for(&q.street, self.country),
            locality: normalise_place(&q.locality, self.country),
            postcode: q.postcode.clone(),
            region: q.region.to_uppercase(),
            confirmed: false,
        };
        self.finish(self.address(&a, &Elsewhere::default()))
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
        // No postcode isn't a postcode: every town without one would match.
        let coded: Vec<u32> = self
            .by_postcode
            .get(postcode)
            .filter(|_| !postcode.is_empty())
            .map(|v| v.iter().copied().filter(in_region).collect())
            .unwrap_or_default();
        if !coded.is_empty() {
            return coded;
        }
        named
    }

    /// A postcode's own position, if the pack has one.
    fn postcode_point(&self, code: &str) -> Option<(f64, f64)> {
        let packed = self.postcodes.get(code)?;
        Some((
            f64::from((packed >> 32) as u32 as i32) / 1e7,
            f64::from(packed as u32 as i32) / 1e7,
        ))
    }

    fn area(&self, locality: &str, postcode: &str, region: &str) -> Vec<Candidate> {
        if let Some(at) = self.postcode_point(postcode) {
            return vec![Candidate {
                lat: at.0,
                lon: at.1,
                precision: Precision::Street,
                label: postcode_label(postcode, self.country),
                locality: String::new(),
                postcode: postcode.to_string(),
                region: String::new(),
                place_confirmed: true,
                source: self.source.clone(),
            }];
        }
        let mut ids = self.matching_places(locality, postcode, region);
        // A town's official entry beats its per-postcode pieces, and the biggest town
        // of a name is the likeliest one.
        if postcode.is_empty()
            && ids
                .iter()
                .any(|id| self.places[*id as usize].postcode.is_empty())
        {
            ids.retain(|id| self.places[*id as usize].postcode.is_empty());
        }
        ids.sort_by_key(|id| std::cmp::Reverse(self.places[*id as usize].population));
        ids.into_iter()
            .take(MAX_CANDIDATES)
            .map(|id| {
                let p = &self.places[id as usize];
                let label = self.place_label(p);
                self.candidate(id, (p.lat, p.lon), Precision::Locality, label)
            })
            .collect()
    }

    fn place_label(&self, p: &Place) -> String {
        let code = postcode_label(&p.postcode, self.country);
        // British and New Zealand addresses don't name a region.
        let region = match self.country {
            Country::UnitedKingdom | Country::NewZealand => "",
            _ => p.region.as_str(),
        };
        let parts = [p.display.as_str(), region, code.as_str()];
        parts
            .iter()
            .filter(|s| !s.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(" ")
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
            source: self.source.clone(),
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
        elsewhere: Option<&Elsewhere>,
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
            // A record with no region can't disagree with the one asked for.
            if !region.is_empty() && !p.region.is_empty() && p.region != region {
                continue;
            }
            let lat = f64::from(Self::u32_at(&self.records, i * 4) as i32) / 1e7;
            let lon = f64::from(Self::u32_at(&self.records, i * 4 + 1) as i32) / 1e7;
            // Another pack's town list can say which town a record with no town is in. Being
            // merely close won't do, as boroughs share street names.
            let towns = elsewhere.filter(|e| !e.towns.is_empty());
            let unnamed = p.name.is_empty() && !locality.is_empty();
            let in_town = !locality.is_empty()
                && (p.name == locality
                    || (unnamed && towns.is_some_and(|e| e.is_nearest_town(locality, (lat, lon)))));
            let score = u8::from(in_town) * 2
                + u8::from(!postcode.is_empty() && p.postcode == postcode) * 2;
            if unnamed && towns.is_some() && score == 0 {
                continue;
            }
            // Strict lookups skip the distance work: only the place asked for counts.
            if asked
                && score == 0
                && (elsewhere.is_none()
                    || !anchors
                        .iter()
                        .any(|a| distance(*a, (lat, lon)) < NEARBY_METRES))
            {
                continue;
            }
            out.push(Hit {
                place,
                score,
                number: Self::u32_at(&self.records, i * 4 + 2),
                origin: self.origin_at(i),
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
        // Gather widely before ranking: the stream is alphabetical, so stopping early
        // would drop the closest spelling in a country with millions of streets.
        while let Some((k, g)) = stream.next() {
            if k != key.as_bytes() {
                out.push((g as usize, String::from_utf8_lossy(k).into_owned()));
            }
            if out.len() >= MAX_FUZZY_SCAN {
                break;
            }
        }
        out.sort_by_key(|(_, name)| edits(key, name));
        out.truncate(MAX_FUZZY_STREETS);
        out
    }

    /// Where a place is, for finding records close by. Postcodes can split a city into
    /// thousands of places, so this keeps one position per grid cell.
    fn anchors(&self, places: &[u32], others: &[(f64, f64)]) -> Vec<(f64, f64)> {
        let mut seen = HashSet::new();
        places
            .iter()
            .map(|id| (self.places[*id as usize].lat, self.places[*id as usize].lon))
            .chain(others.iter().copied())
            .filter(|p| seen.insert((town_cell(p.0), town_cell(p.1))))
            .collect()
    }

    /// Positions of a town, biggest first.
    fn town_points(&self, locality: &str, region: &str) -> Vec<(f64, f64)> {
        let mut ids = self.matching_places(locality, "", region);
        ids.sort_by_key(|id| std::cmp::Reverse(self.places[*id as usize].population));
        ids.iter()
            .take(MAX_TOWNS)
            .map(|id| (self.places[*id as usize].lat, self.places[*id as usize].lon))
            .collect()
    }

    fn address(&self, a: &Address, elsewhere: &Elsewhere) -> Vec<Candidate> {
        let mut found = self.resolve(a, elsewhere);
        if !a.locality.is_empty() || !a.postcode.is_empty() {
            for c in &mut found {
                // With a town list, a record with no town only gets here if it's in the
                // town asked for.
                c.place_confirmed = normalise_place(&c.locality, self.country) == a.locality
                    || (!a.postcode.is_empty() && c.postcode == a.postcode)
                    || (a.postcode.is_empty()
                        && c.locality.is_empty()
                        && !elsewhere.towns.is_empty());
            }
        }
        found
    }

    fn resolve(&self, a: &Address, elsewhere: &Elsewhere) -> Vec<Candidate> {
        let places = self.matching_places(&a.locality, &a.postcode, &a.region);
        let anchors = self.anchors(&places, &elsewhere.anchors);
        let mut hits: Vec<(Hit, usize)> = Vec::new();
        let mut groups = self.exact_street(&a.street);
        for (index, (group, _)) in groups.iter().enumerate() {
            hits.extend(
                self.hits(
                    *group,
                    &a.locality,
                    &a.postcode,
                    &a.region,
                    &anchors,
                    Some(elsewhere),
                )
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
                    .hits(*group, &a.locality, &a.postcode, &a.region, &anchors, None)
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
        let street_name = |index: usize| self.street_name(groups[index].0);
        let tail = |place: u32| {
            let p = &self.places[place as usize];
            self.place_label(p)
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
                    let mut found =
                        self.candidate(h.place, (h.lat, h.lon), Precision::House, label);
                    found.source = self.origin_name(h.origin).to_string();
                    exact.push(found);
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

        // A postcode with its own position is far tighter than the middle of a street.
        if let Some(at) = self.postcode_point(&a.postcode) {
            let code = postcode_label(&a.postcode, self.country);
            return vec![Candidate {
                lat: at.0,
                lon: at.1,
                precision: Precision::Street,
                label: format!("{}, {code}", street_name(top[0].1)),
                locality: String::new(),
                postcode: a.postcode.clone(),
                region: String::new(),
                place_confirmed: true,
                source: self.source.clone(),
            }];
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
        let anchors = self.anchors(&places, &[]);
        let collect = |key: &str| -> Vec<Hit> {
            self.exact_street(key)
                .into_iter()
                .flat_map(|(g, _)| {
                    self.hits(
                        g,
                        locality,
                        postcode,
                        region,
                        &anchors,
                        Some(&Elsewhere::default()),
                    )
                })
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
                let named = |key: &str| {
                    self.streets
                        .get(key)
                        .map_or_else(|| title(key), |g| self.street_name(g as usize))
                };
                let label = format!(
                    "Corner of {} and {}, {}",
                    named(first),
                    named(second),
                    self.place_label(p)
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

    fn country(&self) -> Country {
        self.country
    }
}
