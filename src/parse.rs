//! Free-text Australian addresses into structured fields.
//!
//! The hard part is deciding where the street ends and the suburb begins: "12 St
//! Kilda Rd St Kilda" has three candidates for the street type. Rather than guess,
//! the parser tries each split and asks the gazetteer which streets and localities
//! exist.

use crate::normal::{corrected_type, normalise_for, normalise_place, street_type, Country};

/// What the parser may ask of the data it's parsing against.
pub trait Gazetteer {
    fn has_street(&self, key: &str) -> bool;
    fn has_locality(&self, key: &str) -> bool;
    /// Whose address conventions to read by.
    fn country(&self) -> Country {
        Country::Australia
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Address {
    pub number: Option<String>,
    pub street: String,
    pub locality: String,
    pub postcode: String,
    pub region: String,
    /// False when the street isn't known exactly, so fuzzy matching should try.
    pub confirmed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parsed {
    Address(Address),
    Corner {
        first: String,
        second: String,
        locality: String,
        postcode: String,
        region: String,
    },
    Area {
        locality: String,
        postcode: String,
        region: String,
    },
    /// A PO box or similar: a delivery point, not a place on a map.
    Postal,
    Unknown,
}

const UNIT_WORDS: &[&str] = &[
    "UNIT",
    "APT",
    "APARTMENT",
    "FLAT",
    "SUITE",
    "STE",
    "SHOP",
    "LEVEL",
    "LVL",
    "FLOOR",
    "FL",
    "VILLA",
    "TOWNHOUSE",
    "ROOM",
    "U",
    "L",
    "G",
];
const DIRECTIONS: &[&str] = &["NORTH", "SOUTH", "EAST", "WEST", "N", "S", "E", "W"];

const US_STATES: &[(&str, &str)] = &[
    ("ALABAMA", "AL"),
    ("ALASKA", "AK"),
    ("ARIZONA", "AZ"),
    ("ARKANSAS", "AR"),
    ("CALIFORNIA", "CA"),
    ("COLORADO", "CO"),
    ("CONNECTICUT", "CT"),
    ("DELAWARE", "DE"),
    ("DISTRICT OF COLUMBIA", "DC"),
    ("FLORIDA", "FL"),
    ("GEORGIA", "GA"),
    ("HAWAII", "HI"),
    ("IDAHO", "ID"),
    ("ILLINOIS", "IL"),
    ("INDIANA", "IN"),
    ("IOWA", "IA"),
    ("KANSAS", "KS"),
    ("KENTUCKY", "KY"),
    ("LOUISIANA", "LA"),
    ("MAINE", "ME"),
    ("MARYLAND", "MD"),
    ("MASSACHUSETTS", "MA"),
    ("MICHIGAN", "MI"),
    ("MINNESOTA", "MN"),
    ("MISSISSIPPI", "MS"),
    ("MISSOURI", "MO"),
    ("MONTANA", "MT"),
    ("NEBRASKA", "NE"),
    ("NEVADA", "NV"),
    ("NEW HAMPSHIRE", "NH"),
    ("NEW JERSEY", "NJ"),
    ("NEW MEXICO", "NM"),
    ("NEW YORK", "NY"),
    ("NORTH CAROLINA", "NC"),
    ("NORTH DAKOTA", "ND"),
    ("OHIO", "OH"),
    ("OKLAHOMA", "OK"),
    ("OREGON", "OR"),
    ("PENNSYLVANIA", "PA"),
    ("RHODE ISLAND", "RI"),
    ("SOUTH CAROLINA", "SC"),
    ("SOUTH DAKOTA", "SD"),
    ("TENNESSEE", "TN"),
    ("TEXAS", "TX"),
    ("UTAH", "UT"),
    ("VERMONT", "VT"),
    ("VIRGINIA", "VA"),
    ("WASHINGTON", "WA"),
    ("WEST VIRGINIA", "WV"),
    ("WISCONSIN", "WI"),
    ("WYOMING", "WY"),
];

fn us_region(words: &[&str]) -> Option<(&'static str, usize)> {
    for n in (1..=3).rev() {
        if words.len() < n {
            continue;
        }
        let tail = words[words.len() - n..].join(" ");
        if let Some((_, code)) = US_STATES
            .iter()
            .find(|(name, code)| *name == tail || (n == 1 && *code == tail))
        {
            return Some((code, n));
        }
    }
    None
}

fn is_us_zip(word: &str) -> bool {
    let (five, rest) = word.split_at(word.len().min(5));
    five.len() == 5
        && five.chars().all(|c| c.is_ascii_digit())
        && (rest.is_empty()
            || (rest.len() == 5
                && rest.starts_with('-')
                && rest[1..].chars().all(|c| c.is_ascii_digit())))
}

/// American units follow the street: "123 Main St Apt 4B", "Suite 200", "#12".
fn strip_trailing_units<'a>(words: &[&'a str]) -> Vec<&'a str> {
    const WORDS: &[&str] = &[
        "APT",
        "APARTMENT",
        "UNIT",
        "STE",
        "SUITE",
        "FL",
        "FLOOR",
        "BLDG",
        "BUILDING",
        "RM",
        "ROOM",
        "LOT",
        "TRLR",
        "SPC",
        "DEPT",
    ];
    let mut out = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        let w = words[i];
        if w.starts_with('#') {
            i += if w.len() == 1 { 2 } else { 1 };
            continue;
        }
        if WORDS.contains(&w) && i + 1 < words.len() && i > 0 {
            i += 2;
            continue;
        }
        out.push(w);
        i += 1;
    }
    out
}

fn region_code(words: &[&str]) -> Option<(&'static str, usize)> {
    let tail = |n: usize| (words.len() >= n).then(|| words[words.len() - n..].join(" "));
    match tail(3).as_deref() {
        Some("NEW SOUTH WALES") => return Some(("NSW", 3)),
        Some("AUSTRALIAN CAPITAL TERRITORY") => return Some(("ACT", 3)),
        _ => {}
    }
    match tail(2).as_deref() {
        Some("SOUTH AUSTRALIA") => return Some(("SA", 2)),
        Some("WESTERN AUSTRALIA") => return Some(("WA", 2)),
        Some("NORTHERN TERRITORY") => return Some(("NT", 2)),
        _ => {}
    }
    Some(match tail(1)?.as_str() {
        "VIC" | "VICTORIA" => ("VIC", 1),
        "NSW" => ("NSW", 1),
        "QLD" | "QUEENSLAND" => ("QLD", 1),
        "SA" => ("SA", 1),
        "WA" => ("WA", 1),
        "TAS" | "TASMANIA" => ("TAS", 1),
        "ACT" => ("ACT", 1),
        "NT" => ("NT", 1),
        _ => return None,
    })
}

fn is_postal(text: &str) -> bool {
    const SIGNS: &[&str] = &[
        "PO BOX",
        "P O BOX",
        "POBOX",
        "POST OFFICE BOX",
        "GPO BOX",
        "LOCKED BAG",
        "PRIVATE BAG",
        "RMB ",
        "CMB ",
        "RSD ",
    ];
    let padded = format!(" {text} ");
    SIGNS.iter().any(|s| padded.contains(&format!(" {s}")))
}

/// "12", "12A", "12-14", "12A-14B" as the first number and its suffix.
fn house_number(token: &str) -> Option<String> {
    let first = token.split('-').next()?;
    let digits = first.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 || digits > 6 {
        return None;
    }
    let rest = &first[digits..];
    if rest.len() > 1 || !rest.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    Some(first.to_lowercase())
}

fn is_postcode(word: &str) -> bool {
    word.len() == 4 && word.chars().all(|c| c.is_ascii_digit())
}

fn is_type(token: &str) -> bool {
    street_type(&token.to_lowercase()).is_some()
}

fn strip_units<'a>(words: &[&'a str]) -> Vec<&'a str> {
    let mut out = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        let w = words[i];
        if let Some((_, after)) = w.split_once('/') {
            if after.starts_with(|c: char| c.is_ascii_digit()) {
                out.push(after);
                i += 1;
                continue;
            }
        }
        let next_is_number = words.get(i + 1).is_some_and(|n| house_number(n).is_some());
        if UNIT_WORDS.contains(&w) && next_is_number {
            i += 2;
            continue;
        }
        // "L2", "U3": a level or unit written as one word.
        let glued = w.len() >= 2
            && matches!(w.as_bytes()[0], b'L' | b'U')
            && w[1..].chars().all(|c| c.is_ascii_digit());
        if glued {
            i += 1;
            continue;
        }
        // "Lot 5 12 High St": the lot goes, the street number stays.
        if w == "LOT"
            && next_is_number
            && words.get(i + 2).is_some_and(|n| house_number(n).is_some())
        {
            i += 2;
            continue;
        }
        out.push(w);
        i += 1;
    }
    out
}

fn known_street(g: &dyn Gazetteer, words: &[&str]) -> Option<String> {
    let key = normalise_for(&words.join(" "), g.country());
    if g.has_street(&key) {
        return Some(key);
    }
    ["st", "rd", "av", "dr"]
        .iter()
        .map(|t| format!("{key} {t}"))
        .find(|k| g.has_street(k))
}

fn parse_corner(words: &[&str], g: &dyn Gazetteer, postcode: &str, region: &str) -> Parsed {
    let words: Vec<&str> = words
        .iter()
        .copied()
        .filter(|w| !matches!(*w, "CNR" | "CORNER" | "OF"))
        .collect();
    let Some(split) = words.iter().position(|w| matches!(*w, "AND" | "&")) else {
        return Parsed::Unknown;
    };
    let (left, right) = (&words[..split], &words[split + 1..]);
    let Some(first) = known_street(g, left) else {
        return Parsed::Unknown;
    };
    for end in (1..=right.len()).rev() {
        let locality = normalise_place(&right[end..].join(" "), g.country());
        if !locality.is_empty() && !g.has_locality(&locality) {
            continue;
        }
        if let Some(second) = known_street(g, &right[..end]) {
            return Parsed::Corner {
                first,
                second,
                locality,
                postcode: postcode.into(),
                region: region.into(),
            };
        }
    }
    Parsed::Unknown
}

/// Parses free text. Never fails: anything it can't read comes back as `Unknown`.
pub fn parse(text: &str, g: &dyn Gazetteer) -> Parsed {
    let country = g.country();
    let norm = |s: &str| normalise_for(s, country);
    let place = |s: &str| normalise_place(s, country);
    let is_code = |w: &str| match country {
        Country::Australia => is_postcode(w),
        Country::UnitedStates => is_us_zip(w),
    };
    let regions = |w: &[&str]| match country {
        Country::Australia => region_code(w),
        Country::UnitedStates => us_region(w),
    };
    let cleaned: String = text
        .to_uppercase()
        .chars()
        .map(|c| {
            if matches!(c, ',' | '.' | ';' | '(' | ')' | ':') {
                ' '
            } else {
                c
            }
        })
        .collect();
    let mut joined = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    joined = joined
        .replace(" / ", "/")
        .replace("/ ", "/")
        .replace(" /", "/");
    if is_postal(&joined) {
        return Parsed::Postal;
    }
    let mut words: Vec<&str> = joined.split_whitespace().collect();

    if country == Country::Australia
        && words.last() == Some(&"AUSTRALIA")
        && !matches!(words.iter().rev().nth(1), Some(&"SOUTH") | Some(&"WESTERN"))
    {
        words.pop();
    }
    if country == Country::UnitedStates {
        if words.ends_with(&["UNITED", "STATES"]) {
            words.truncate(words.len() - 2);
        } else if matches!(words.last(), Some(&"USA") | Some(&"US")) {
            words.pop();
        }
    }
    let mut postcode = String::new();
    if words.len() > 1 || words.last().is_some_and(|w| is_code(w)) {
        if let Some(w) = words.last().filter(|w| is_code(w)) {
            postcode = w.chars().take(5).collect();
            words.pop();
        }
    }
    let mut region = String::new();
    if let Some((code, n)) = regions(&words) {
        // "11 Bolton Ct 34758": Ct is the street's type here, not Connecticut.
        let street_ends_here = country == Country::UnitedStates
            && n == 1
            && words.len() > 2
            && is_type(words[words.len() - 1])
            && g.has_street(&norm(&words[1..].join(" ")));
        if !street_ends_here {
            region = code.to_string();
            words.truncate(words.len() - n);
        }
    }
    if postcode.is_empty() {
        if let Some(w) = words.last().filter(|w| is_code(w)) {
            postcode = w.chars().take(5).collect();
            words.pop();
        }
    }
    if words.is_empty() {
        return if postcode.is_empty() {
            Parsed::Unknown
        } else {
            Parsed::Area {
                locality: String::new(),
                postcode,
                region,
            }
        };
    }
    if words.iter().any(|w| matches!(*w, "CNR" | "CORNER"))
        && !words[0].starts_with(|c: char| c.is_ascii_digit())
    {
        return parse_corner(&words, g, &postcode, &region);
    }

    let words = strip_units(&words);
    let words = if country == Country::UnitedStates {
        strip_trailing_units(&words)
    } else {
        words
    };
    let mut best: Option<((u8, u8, usize), Address)> = None;
    for start in 0..words.len() {
        let Some(number) = house_number(words[start]) else {
            continue;
        };
        for end in start + 2..=words.len() {
            let street = norm(&words[start + 1..end].join(" "));
            if !g.has_street(&street) {
                continue;
            }
            let locality = place(&words[end..].join(" "));
            let score = u8::from(g.has_locality(&locality)) * 4
                + u8::from(locality.is_empty())
                + u8::from(!postcode.is_empty());
            let plain = u8::from(!DIRECTIONS.contains(&words[end - 1]));
            let rank = (score, plain, end - start);
            if best.as_ref().is_none_or(|b| rank > b.0) {
                let a = Address {
                    number: Some(number.clone()),
                    street,
                    locality,
                    postcode: postcode.clone(),
                    region: region.clone(),
                    confirmed: true,
                };
                best = Some((rank, a));
            }
        }
    }
    // A street that exists, followed by words that aren't a place, is a weak reading:
    // "1304 Idaho Steret, Baytown" isn't a street called Idaho. It only gives way when
    // a known place further along says where the street really ends.
    let place_follows = |start: usize| {
        (start + 2..words.len()).any(|e| g.has_locality(&place(&words[e..].join(" "))))
    };
    let weak = best
        .as_ref()
        .is_some_and(|(_, a)| !a.locality.is_empty() && !g.has_locality(&a.locality));
    let first_number = words.iter().position(|w| house_number(w).is_some());
    if !(weak && first_number.is_some_and(place_follows)) {
        if let Some((_, a)) = best {
            return Parsed::Address(a);
        }
    }

    // No exact street: take the likeliest split, and let fuzzy matching check it.
    if let Some(start) = words.iter().position(|w| house_number(w).is_some()) {
        let last = words.len();
        // A known suburb marks where the street ends, even if the street is misspelled.
        let before_place =
            (start + 2..last).find(|&e| g.has_locality(&place(&words[e..].join(" "))));
        let typed = (start + 2..=last).find(|&e| is_type(words[e - 1]));
        let end = before_place
            .or(typed)
            .or((!postcode.is_empty() && last >= start + 2).then_some(last));
        if let Some(end) = end {
            let mut street = norm(&words[start + 1..end].join(" "));
            let tail = street.rsplit(' ').next().unwrap_or("").to_string();
            if let Some(fixed) = (street_type(&tail).is_none())
                .then(|| corrected_type(&tail))
                .flatten()
            {
                street.truncate(street.len() - tail.len());
                street.push_str(fixed);
            }
            return Parsed::Address(Address {
                number: house_number(words[start]),
                street,
                locality: place(&words[end..].join(" ")),
                postcode,
                region,
                confirmed: false,
            });
        }
    }

    // A place on its own.
    let whole = place(&words.join(" "));
    if g.has_locality(&whole) {
        return Parsed::Area {
            locality: whole,
            postcode,
            region,
        };
    }

    // A street with no number: "High St, Windsor".
    for end in (1..=words.len()).rev() {
        if !is_type(words[end - 1]) {
            continue;
        }
        let street = norm(&words[..end].join(" "));
        if g.has_street(&street) {
            let locality = place(&words[end..].join(" "));
            return Parsed::Address(Address {
                number: None,
                street,
                locality,
                postcode,
                region,
                confirmed: true,
            });
        }
    }
    if postcode.is_empty() {
        Parsed::Unknown
    } else {
        Parsed::Area {
            locality: String::new(),
            postcode,
            region,
        }
    }
}
