//! Free-text Australian addresses into structured fields.
//!
//! The hard part is deciding where the street ends and the suburb begins: "12 St
//! Kilda Rd St Kilda" has three candidates for the street type. Rather than guess,
//! the parser tries each split and asks the gazetteer which streets and localities
//! exist.

use crate::normal::{normalise, street_type};

/// What the parser may ask of the data it's parsing against.
pub trait Gazetteer {
    fn has_street(&self, key: &str) -> bool;
    fn has_locality(&self, key: &str) -> bool;
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
    let key = normalise(&words.join(" "));
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
        let locality = normalise(&right[end..].join(" "));
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

    if words.last() == Some(&"AUSTRALIA")
        && !matches!(words.iter().rev().nth(1), Some(&"SOUTH") | Some(&"WESTERN"))
    {
        words.pop();
    }
    let is_postcode = |w: &&str| w.len() == 4 && w.chars().all(|c| c.is_ascii_digit());
    let mut postcode = String::new();
    if words.len() > 1 || words.last().is_some_and(&is_postcode) {
        if let Some(w) = words.last().filter(|w| is_postcode(w)) {
            postcode = (*w).to_string();
            words.pop();
        }
    }
    let mut region = String::new();
    if let Some((code, n)) = region_code(&words) {
        region = code.to_string();
        words.truncate(words.len() - n);
    }
    if postcode.is_empty() {
        if let Some(w) = words.last().filter(|w| is_postcode(w)) {
            postcode = (*w).to_string();
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
    let mut best: Option<((u8, u8, usize), Address)> = None;
    for start in 0..words.len() {
        let Some(number) = house_number(words[start]) else {
            continue;
        };
        for end in start + 2..=words.len() {
            let street = normalise(&words[start + 1..end].join(" "));
            if !g.has_street(&street) {
                continue;
            }
            let locality = normalise(&words[end..].join(" "));
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
    if let Some((_, a)) = best {
        return Parsed::Address(a);
    }

    // No exact street: take the likeliest split, and let fuzzy matching check it.
    if let Some(start) = words.iter().position(|w| house_number(w).is_some()) {
        let mut ends: Vec<usize> = (start + 2..=words.len())
            .filter(|&e| is_type(words[e - 1]))
            .collect();
        let suffixed: Vec<usize> = ends
            .iter()
            .map(|e| e + 1)
            .filter(|&e| e <= words.len() && DIRECTIONS.contains(&words[e - 1]))
            .collect();
        ends.extend(suffixed);
        let end = ends
            .iter()
            .copied()
            .find(|&e| g.has_locality(&normalise(&words[e..].join(" "))))
            .or(ends.first().copied());
        if let Some(end) = end {
            return Parsed::Address(Address {
                number: house_number(words[start]),
                street: normalise(&words[start + 1..end].join(" ")),
                locality: normalise(&words[end..].join(" ")),
                postcode,
                region,
                confirmed: false,
            });
        }
    }

    // A place on its own.
    let whole = normalise(&words.join(" "));
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
        let street = normalise(&words[..end].join(" "));
        if g.has_street(&street) {
            let locality = normalise(&words[end..].join(" "));
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
