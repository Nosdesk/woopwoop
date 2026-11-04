//! Text normalisation, shared by the build and every lookup so both sides agree.

use crate::street_types::STREET_TYPES;

/// The address conventions a pack follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Country {
    #[default]
    Australia,
    UnitedStates,
}

impl Country {
    pub fn code(self) -> &'static str {
        match self {
            Self::Australia => "AU",
            Self::UnitedStates => "US",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "AU" => Some(Self::Australia),
            "US" => Some(Self::UnitedStates),
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
    matches!(
        word,
        "n" | "s" | "e" | "w" | "north" | "south" | "east" | "west"
    )
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
    let spaced: String = lower
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    let words: Vec<&str> = spaced.split_whitespace().collect();
    let mut end = words.len();
    // Trailing directions and route numbers aren't the type: "County Road 169 North".
    while end > 1
        && (is_direction(words[end - 1]) || words[end - 1].chars().all(|c| c.is_ascii_digit()))
    {
        end -= 1;
    }
    // American streets lead with a direction: "South Main Street", "S Main St".
    let leading = |i: usize| country == Country::UnitedStates && i == 0 && end > 2;
    let mut out = String::with_capacity(spaced.len());
    for (i, word) in words.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let canonical = match *word {
            "mt" => "mount",
            "saint" => "st",
            w if i + 1 == end => street_type(w).unwrap_or(w),
            // "39th Street Circle East": American names can hold a type mid-way.
            w if country == Country::UnitedStates && i > 0 && is_common_type(w) => {
                street_type(w).unwrap_or(w)
            }
            "north" if i >= end || leading(i) => "n",
            "south" if i >= end || leading(i) => "s",
            "east" if i >= end || leading(i) => "e",
            "west" if i >= end || leading(i) => "w",
            w => w,
        };
        out.push_str(canonical);
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
