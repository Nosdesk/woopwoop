//! Text normalisation, shared by the build and every lookup so both sides agree.

use crate::street_types::STREET_TYPES;

/// The address conventions a pack follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Country {
    #[default]
    Australia,
    UnitedStates,
    Canada,
    NewZealand,
    UnitedKingdom,
}

impl Country {
    pub fn code(self) -> &'static str {
        match self {
            Self::Australia => "AU",
            Self::UnitedStates => "US",
            Self::Canada => "CA",
            Self::NewZealand => "NZ",
            Self::UnitedKingdom => "GB",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "AU" => Some(Self::Australia),
            "US" => Some(Self::UnitedStates),
            "CA" => Some(Self::Canada),
            "NZ" => Some(Self::NewZealand),
            "GB" => Some(Self::UnitedKingdom),
            _ => None,
        }
    }
}

/// The canonical form of a street type, if `word` (lowercase) is one.
pub fn street_type(word: &str) -> Option<&'static str> {
    STREET_TYPES
        .binary_search_by_key(&word, |(k, _)| k)
        .ok()
        .map(|i| STREET_TYPES[i].1)
}

fn is_direction(word: &str) -> bool {
    short_direction(word).is_some()
}

fn short_direction(word: &str) -> Option<&'static str> {
    Some(match word {
        "north" | "n" => "n",
        "south" | "s" => "s",
        "east" | "e" => "e",
        "west" | "w" => "w",
        "northwest" | "nw" => "nw",
        "northeast" | "ne" => "ne",
        "southwest" | "sw" => "sw",
        "southeast" | "se" => "se",
        _ => return None,
    })
}

/// "Fifth" and "5th" both become "5", as American data uses all of "5", "5th" and "Fifth".
fn ordinal(word: &str) -> Option<String> {
    const WORDS: &[&str] = &[
        "first",
        "second",
        "third",
        "fourth",
        "fifth",
        "sixth",
        "seventh",
        "eighth",
        "ninth",
        "tenth",
        "eleventh",
        "twelfth",
        "thirteenth",
        "fourteenth",
        "fifteenth",
        "sixteenth",
        "seventeenth",
        "eighteenth",
        "nineteenth",
        "twentieth",
    ];
    if let Some(i) = WORDS.iter().position(|w| *w == word) {
        return Some((i + 1).to_string());
    }
    let digits = word.chars().take_while(char::is_ascii_digit).count();
    (digits > 0 && matches!(&word[digits..], "st" | "nd" | "rd" | "th"))
        .then(|| word[..digits].to_string())
}

/// American route names have several spellings: "FM 2725" is "Farm to Market Road 2725".
fn route_words(words: &[&str]) -> Option<(&'static str, usize)> {
    match words {
        ["farm", "to", "market", "road" | "rd", n, ..] if is_number(n) => Some(("fm", 4)),
        ["farm", "to", "market", n, ..] if is_number(n) => Some(("fm", 3)),
        ["ranch", "road" | "rd", n, ..] if is_number(n) => Some(("rr", 2)),
        ["county", "road" | "rd" | "route" | "highway" | "hwy", n, ..] if is_number(n) => {
            Some(("cr", 2))
        }
        ["state", "road" | "rd" | "route" | "highway" | "hwy", n, ..] if is_number(n) => {
            Some(("sr", 2))
        }
        ["us", "highway" | "hwy" | "route" | "rte", n, ..] if is_number(n) => Some(("us", 2)),
        ["highway" | "hwy", n, ..] if is_number(n) => Some(("hwy", 1)),
        ["interstate", n, ..] if is_number(n) => Some(("i", 1)),
        _ => None,
    }
}

/// French accents, which Canadians often leave out: "Montréal" and "Montreal".
fn unaccented(c: char) -> char {
    match c {
        'à' | 'â' | 'ä' => 'a',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'î' | 'ï' => 'i',
        'ô' | 'ö' => 'o',
        'ù' | 'û' | 'ü' => 'u',
        'ç' => 'c',
        other => other,
    }
}

/// A postcode as packs store it: Canadian and British codes lose their space.
pub fn postcode_key(code: &str, country: Country) -> String {
    match country {
        Country::Canada | Country::UnitedKingdom => {
            code.split_whitespace().collect::<String>().to_uppercase()
        }
        _ => code.to_string(),
    }
}

/// A stored postcode as people write it: "SW1A2AA" becomes "SW1A 2AA".
pub fn postcode_label(code: &str, country: Country) -> String {
    match country {
        Country::Canada | Country::UnitedKingdom if code.len() > 3 && code.is_ascii() => {
            format!("{} {}", &code[..code.len() - 3], &code[code.len() - 3..])
        }
        _ => code.to_string(),
    }
}

fn is_number(word: &str) -> bool {
    !word.is_empty() && word.chars().all(|c| c.is_ascii_digit())
}

fn is_common_type(word: &str) -> bool {
    matches!(
        word,
        "street"
            | "st"
            | "road"
            | "rd"
            | "avenue"
            | "ave"
            | "boulevard"
            | "blvd"
            | "drive"
            | "dr"
            | "lane"
            | "ln"
            | "court"
            | "ct"
            | "circle"
            | "cir"
            | "place"
            | "pl"
            | "terrace"
            | "ter"
            | "highway"
            | "hwy"
            | "parkway"
            | "pkwy"
    )
}

/// The street type a misspelled word was probably meant to be.
pub(crate) fn corrected_type(word: &str) -> Option<&'static str> {
    if word.len() < 5 {
        return None;
    }
    const COMMON: &[&str] = &[
        "street",
        "road",
        "avenue",
        "drive",
        "court",
        "place",
        "lane",
        "boulevard",
        "circle",
        "crescent",
        "terrace",
        "highway",
        "parkway",
        "close",
        "parade",
    ];
    COMMON
        .iter()
        .find(|t| crate::automaton::edits(word, t) <= 2)
        .and_then(|t| street_type(t))
}

/// Corrects a misspelled word that normalising would otherwise lose: "Coutny Road 702"
/// has to read "County" before it can become "CR 702".
pub(crate) fn corrected_keyword(word: &str) -> Option<&'static str> {
    const WORDS: &[&str] = &[
        "county",
        "north",
        "south",
        "east",
        "west",
        "northeast",
        "northwest",
        "southeast",
        "southwest",
        "state",
        "highway",
        "market",
    ];
    if word.len() < 4 {
        return None;
    }
    WORDS
        .iter()
        .find(|w| **w != word && crate::automaton::edits(word, w) == 1)
        .copied()
}

/// Normalises a suburb or city, dropping the official prefix: people type "El Paso"
/// for "City of El Paso".
pub fn normalise_place(text: &str, country: Country) -> String {
    normalise_for(strip_place_prefix(text), country)
}

/// `text` without "City of" and the like.
pub fn strip_place_prefix(text: &str) -> &str {
    for prefix in [
        "city of ",
        "town of ",
        "township of ",
        "village of ",
        "borough of ",
    ] {
        if text.len() > prefix.len()
            && text.is_char_boundary(prefix.len())
            && text[..prefix.len()].eq_ignore_ascii_case(prefix)
        {
            return &text[prefix.len()..];
        }
    }
    text
}

/// Lowercases, drops punctuation and settles spelling, so "St. Kilda Road" and "ST
/// KILDA RD" produce the same key.
///
/// Only the street type itself is abbreviated: the last word, or the last before a
/// trailing direction. Words inside a name stay whole, because "Green" in "Green
/// Court" is a name, and shortening it would put a typo out of fuzzy reach.
pub fn normalise(text: &str) -> String {
    normalise_for(text, Country::Australia)
}

/// `normalise`, with the conventions of one country.
pub fn normalise_for(text: &str, country: Country) -> String {
    let lower = text.to_lowercase();
    let french = country == Country::Canada;
    let spaced: String = lower
        .chars()
        .map(|c| if french { unaccented(c) } else { c })
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    let words: Vec<&str> = spaced.split_whitespace().collect();
    let american = country == Country::UnitedStates;
    let north_american = american || french;
    let mut end = words.len();
    // Trailing directions and route numbers aren't the type: "County Road 169 North".
    while end > 1 && (is_direction(words[end - 1]) || is_number(words[end - 1])) {
        end -= 1;
    }
    let mut out = String::with_capacity(spaced.len());
    let mut i = 0;
    while i < words.len() {
        if !out.is_empty() {
            out.push(' ');
        }
        if american {
            if let Some((short, used)) = route_words(&words[i..]) {
                out.push_str(short);
                i += used;
                continue;
            }
        }
        let word = words[i];
        // American streets lead with a direction: "South Main Street", "S Main St".
        let leading = american && i == 0 && end > 2;
        if (i >= end || leading) && short_direction(word).is_some() {
            out.push_str(short_direction(word).unwrap_or(word));
        } else if let Some(number) = ordinal(word).filter(|_| north_american) {
            out.push_str(&number);
        } else {
            out.push_str(match word {
                "mt" => "mount",
                "saint" => "st",
                w if i + 1 == end => street_type(w).unwrap_or(w),
                // "39th Street Circle East": American names can hold a type mid-way.
                w if north_american && i > 0 && is_common_type(w) => street_type(w).unwrap_or(w),
                w => w,
            });
        }
        i += 1;
    }
    out
}

/// Title case for display: "high st" becomes "High St".
pub fn title(text: &str) -> String {
    text.split_whitespace()
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f
                    .to_uppercase()
                    .chain(c.flat_map(char::to_lowercase))
                    .collect(),
                None => String::new(),
            }
        })
        .collect::<Vec<String>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spellings_of_a_street_agree() {
        assert_eq!(normalise("St. Kilda Road"), normalise("ST KILDA RD"));
        assert_eq!(normalise("High Street"), normalise("high st"));
        assert_eq!(
            normalise("Saint Georges Avenue"),
            normalise("St Georges Ave")
        );
        assert_eq!(
            normalise("Mount Alexander Road"),
            normalise("Mt Alexander Rd")
        );
    }

    #[test]
    fn only_the_street_type_is_abbreviated() {
        assert_eq!(normalise("Green Park Road"), "green park rd");
        assert_eq!(normalise("North Street"), "north st");
        assert_eq!(normalise("High Street West"), normalise("High St W"));
    }

    #[test]
    fn american_streets_lead_with_a_direction() {
        let us = |t| normalise_for(t, Country::UnitedStates);
        assert_eq!(us("South Sasabe Highway"), us("S Sasabe Hwy"));
        assert_eq!(us("North Street"), "north st");
    }

    #[test]
    fn numbered_routes_keep_their_type() {
        assert_eq!(normalise("County Road 169"), normalise("County Rd 169"));
        assert_eq!(
            normalise_for("39th Street Circle East", Country::UnitedStates),
            normalise_for("39th St Cir E", Country::UnitedStates)
        );
    }

    #[test]
    fn american_numbers_and_routes_have_one_spelling() {
        let us = |t| normalise_for(t, Country::UnitedStates);
        assert_eq!(us("Fifth Avenue"), us("5th Ave"));
        assert_eq!(us("5 Avenue"), us("5th Ave"));
        assert_eq!(us("West 34th Street"), us("W 34 St"));
        assert_eq!(
            us("Pennsylvania Avenue Northwest"),
            us("Pennsylvania Ave NW")
        );
        assert_eq!(us("Farm to Market Road 2725"), us("FM 2725"));
        assert_eq!(us("County Road 26"), us("CR 26"));
        assert_eq!(us("State Route 110"), us("SR 110"));
    }

    #[test]
    fn places_lose_their_official_prefix() {
        assert_eq!(
            normalise_place("CITY OF EL PASO", Country::UnitedStates),
            "el paso"
        );
        assert_eq!(
            normalise_place("Kansas City", Country::UnitedStates),
            "kansas city"
        );
    }

    #[test]
    fn a_misspelled_type_is_recognised() {
        assert_eq!(corrected_type("steret"), street_type("street"));
        assert_eq!(corrected_type("avenuw"), street_type("avenue"));
        assert_eq!(corrected_type("windsor"), None);
    }

    #[test]
    fn title_case() {
        assert_eq!(title("high st"), "High St");
    }

    #[test]
    fn the_table_is_sorted() {
        assert!(STREET_TYPES.windows(2).all(|w| w[0].0 < w[1].0));
    }
}
