//! Whatever a person types, a search answers and never panics.

use woopwoop::{Country, Outcome, Pack, PackBuilder, Record};

const ODD: [&str; 5] = ["Н", "é", "€", "𝟙", "\u{200b}"];
const WORDS: [&str; 7] = [
    "12345",
    "12345-6789",
    "SW1A2AA",
    "SW1A",
    "K1A0B1",
    "3000",
    "1st",
];

fn pack(country: Country, code: &str) -> Pack {
    let file = std::env::temp_dir().join(format!("woopwoop-odd-{}-{code}", std::process::id()));
    let mut b = PackBuilder::for_country("test", country);
    b.add(&Record {
        street: "Smith Street".into(),
        number: "1".into(),
        locality: "Fitzroy".into(),
        postcode: "3065".into(),
        region: "VIC".into(),
        lat: -37.8,
        lon: 145.0,
    });
    b.finish(&file).unwrap();
    let pack = Pack::open(&file).unwrap();
    std::fs::remove_file(&file).ok();
    pack
}

#[test]
fn a_multibyte_character_anywhere_in_a_word_is_safe() {
    for code in ["AU", "US", "CA", "NZ", "GB"] {
        let pack = pack(Country::from_code(code).unwrap(), code);
        for word in WORDS {
            for odd in ODD {
                for at in 0..=word.len() {
                    let mixed = format!("{}{odd}{}", &word[..at], &word[at..]);
                    pack.search(&mixed);
                    pack.search(&format!("1 Smith St, Fitzroy {mixed}"));
                    pack.search(&format!("{mixed} Smith St"));
                    pack.search(&format!("1 Smith St, {mixed} {word}"));
                }
            }
        }
    }
}

fn us_pack(name: &str) -> Pack {
    let file = std::env::temp_dir().join(format!("woopwoop-us-{}-{name}", std::process::id()));
    let mut b = PackBuilder::for_country("test", Country::from_code("US").unwrap());
    let mut add = |number: &str, locality: &str, postcode: &str, region: &str, lat: f64| {
        b.add(&Record {
            street: "E Main St".into(),
            number: number.into(),
            locality: locality.into(),
            postcode: postcode.into(),
            region: region.into(),
            lat,
            lon: -90.0,
        })
    };
    add("316", "Edna", "77957", "TX", 28.98);
    add("100", "Lebanon", "62254", "IL", 38.60);
    b.add_place("Edna", "TX", 28.97, -90.0, 5000);
    b.add_place("Lebanon", "IL", 38.60, -90.0, 4000);
    b.finish(&file).unwrap();
    let pack = Pack::open(&file).unwrap();
    std::fs::remove_file(&file).ok();
    pack
}

#[test]
fn a_zip_with_no_dash_is_a_zip() {
    let Outcome::Found(found) = us_pack("dashless").search("100 E Main St, Lebanon, IL 622540000")
    else {
        panic!("not found");
    };
    assert_eq!(
        (found[0].region.as_str(), found[0].postcode.as_str()),
        ("IL", "62254")
    );
}

#[test]
fn an_unreadable_postcode_does_not_send_a_search_to_another_state() {
    let pack = us_pack("unreadable");
    if let Outcome::Found(found) = pack.search("316 E Main St, Lebanon, IL 999") {
        assert_ne!(found[0].region, "TX", "{:?}", found[0]);
    }
}
