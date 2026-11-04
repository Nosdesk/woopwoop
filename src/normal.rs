//! Text normalisation, shared by the build and every lookup so both sides agree.

use crate::street_types::STREET_TYPES;

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

/// Lowercases, drops punctuation and settles spelling, so "St. Kilda Road" and "ST
/// KILDA RD" produce the same key.
///
/// Only the street type itself is abbreviated: the last word, or the last before a
/// trailing direction. Words inside a name stay whole, because "Green" in "Green
/// Court" is a name, and shortening it would put a typo out of fuzzy reach.
pub fn normalise(text: &str) -> String {
    let lower = text.to_lowercase();
    let spaced: String = lower
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    let words: Vec<&str> = spaced.split_whitespace().collect();
    let mut end = words.len();
    while end > 1 && is_direction(words[end - 1]) {
        end -= 1;
    }
    let mut out = String::with_capacity(spaced.len());
    for (i, word) in words.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let canonical = match *word {
            "mt" => "mount",
            "saint" => "st",
            w if i + 1 == end => street_type(w).unwrap_or(w),
            "north" if i >= end => "n",
            "south" if i >= end => "s",
            "east" if i >= end => "e",
            "west" if i >= end => "w",
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
    fn title_case() {
        assert_eq!(title("high st"), "High St");
    }

    #[test]
    fn the_table_is_sorted() {
        assert!(STREET_TYPES.windows(2).all(|w| w[0].0 < w[1].0));
    }
}
