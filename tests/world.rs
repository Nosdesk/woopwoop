//! Canadian, British and New Zealand addresses, on small made-up fixtures.

use std::sync::atomic::{AtomicUsize, Ordering};
use woopwoop::{Candidate, Country, Declined, Outcome, Pack, PackBuilder, Precision, Record};

fn record(
    number: &str,
    street: &str,
    place: &str,
    code: &str,
    region: &str,
    lat: f64,
    lon: f64,
) -> Record {
    Record {
        street: street.into(),
        number: number.into(),
        locality: place.into(),
        postcode: code.into(),
        region: region.into(),
        lat,
        lon,
    }
}

fn pack(name: &str, country: Country, fill: impl FnOnce(&mut PackBuilder)) -> Pack {
    let mut b = PackBuilder::for_country("test fixture", country);
    fill(&mut b);
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let unique = NEXT.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "woopwoop-{name}-{}-{unique}.pack",
        std::process::id()
    ));
    b.finish(&path).expect("build");
    let p = Pack::open(&path).expect("open");
    let _ = std::fs::remove_file(&path);
    p
}

fn found(p: &Pack, text: &str) -> Vec<Candidate> {
    match p.search(text) {
        Outcome::Found(c) => c,
        other => panic!("{text}: {other:?}"),
    }
}

fn canada() -> Pack {
    pack("ca", Country::Canada, |b| {
        b.add(&record(
            "290",
            "Bremner Boulevard",
            "Toronto",
            "M5V3L9",
            "ON",
            43.6426,
            -79.3871,
        ));
        b.add(&record(
            "1000",
            "Rue Saint-Antoine Ouest",
            "Montréal",
            "H3C3R7",
            "QC",
            45.5017,
            -73.5673,
        ));
        b.add(&record(
            "345",
            "King Street West",
            "Toronto",
            "M5V1K1",
            "ON",
            43.6455,
            -79.3900,
        ));
        b.add(&record(
            "100",
            "5th Avenue SW",
            "Calgary",
            "T2P3S9",
            "AB",
            51.0486,
            -114.0708,
        ));
    })
}

#[test]
fn canadian_addresses_read() {
    let p = canada();
    for text in [
        "290 Bremner Blvd, Toronto, ON M5V 3L9",
        "290 Bremner Blvd Toronto Ontario M5V3L9",
        "290 Bremner Boulevard, Toronto, Canada",
        "Suite 200, 290 Bremner Blvd, Toronto ON",
        "290 Bremner Blvd Unit 12, Toronto ON",
    ] {
        assert_eq!(found(&p, text)[0].precision, Precision::House, "{text}");
    }
}

#[test]
fn the_unit_comes_first_in_canada() {
    let c = &found(&canada(), "12-345 King St W, Toronto, ON")[0];
    assert_eq!(
        (c.precision, c.postcode.as_str()),
        (Precision::House, "M5V1K1")
    );
}

#[test]
fn french_names_read_with_or_without_accents() {
    let p = canada();
    for text in [
        "1000 rue Saint-Antoine Ouest, Montréal, QC",
        "1000 Rue Saint Antoine Ouest, Montreal, Quebec H3C 3R7",
    ] {
        assert_eq!(found(&p, text)[0].precision, Precision::House, "{text}");
    }
}

#[test]
fn canadian_postcodes_show_their_space() {
    assert!(found(&canada(), "290 Bremner Blvd, Toronto")
        .first()
        .is_some_and(|c| c.label.ends_with("M5V 3L9")));
}

fn britain() -> Pack {
    pack("gb", Country::UnitedKingdom, |b| {
        b.add_place("London", "", 51.5074, -0.1278, 9000000);
        b.add_postcode("SW1A 2AA", 51.5034, -0.1276);
        b.add_postcode("EC1A 1BB", 51.5203, -0.0976);
        b.add(&record(
            "221B",
            "Baker Street",
            "London",
            "NW1 6XE",
            "",
            51.5238,
            -0.1586,
        ));
        b.add(&record(
            "1",
            "High Street",
            "Bath",
            "BA1 5AW",
            "",
            51.3811,
            -2.3590,
        ));
    })
}

#[test]
fn british_addresses_read() {
    let p = britain();
    for text in [
        "221B Baker St, London NW1 6XE",
        "221b baker street london",
        "Flat 2, 221B Baker Street, London, UK",
        "221B Baker Street, London, England NW16XE",
    ] {
        assert_eq!(found(&p, text)[0].precision, Precision::House, "{text}");
    }
}

#[test]
fn a_british_postcode_alone_lands_on_the_postcode() {
    let p = britain();
    for text in ["SW1A 2AA", "sw1a2aa", "10 Downing Street, London SW1A 2AA"] {
        let c = &found(&p, text)[0];
        assert!((c.lat - 51.5034).abs() < 1e-3, "{text}: {c:?}");
    }
}

#[test]
fn a_postcode_beats_the_middle_of_a_street() {
    let p = pack("gb-street", Country::UnitedKingdom, |b| {
        b.add_postcode("SW1A 2AA", 51.5034, -0.1276);
        b.add(&record(
            "1",
            "Downing Street",
            "London",
            "",
            "ENG",
            51.5100,
            -0.1300,
        ));
    });
    let c = &found(&p, "10 Downing Street, London SW1A 2AA")[0];
    assert!((c.lat - 51.5034).abs() < 1e-4, "{c:?}");
    assert_eq!(c.label, "Downing Street, SW1A 2AA");
}

#[test]
fn british_labels_leave_out_the_region() {
    let p = pack("gb-label", Country::UnitedKingdom, |b| {
        b.add(&record(
            "1",
            "High Street",
            "Bath",
            "BA1 5AW",
            "ENG",
            51.38,
            -2.36,
        ))
    });
    assert_eq!(
        found(&p, "1 High St, Bath")[0].label,
        "1 High Street, Bath BA1 5AW"
    );
}

#[test]
fn postcodes_with_a_letter_in_the_middle_read() {
    assert!((found(&britain(), "EC1A 1BB")[0].lat - 51.5203).abs() < 1e-3);
}

#[test]
fn british_po_boxes_are_declined() {
    assert_eq!(
        britain().search("PO Box 100, London"),
        Outcome::Declined(Declined::PostalAddress)
    );
}

fn zealand() -> Pack {
    pack("nz", Country::NewZealand, |b| {
        b.add_place("Auckland", "", -36.8485, 174.7633, 1400000);
        b.add(&record(
            "1",
            "Queen Street",
            "Auckland Central",
            "",
            "Auckland",
            -36.8430,
            174.7670,
        ));
        b.add(&record(
            "48",
            "Queen Street",
            "Thames",
            "",
            "Waikato",
            -37.1400,
            175.5400,
        ));
    })
}

#[test]
fn new_zealand_addresses_read() {
    let p = zealand();
    for text in [
        "1 Queen St, Auckland Central",
        "1/1 Queen Street, Auckland Central, New Zealand",
        "1 Queen St, Auckland Central 1010",
    ] {
        assert_eq!(found(&p, text)[0].locality, "Auckland Central", "{text}");
    }
}

#[test]
fn new_zealand_labels_do_not_repeat_the_city() {
    assert_eq!(
        found(&zealand(), "1 Queen St, Auckland Central")[0].label,
        "1 Queen Street, Auckland Central"
    );
}

#[test]
fn a_new_zealand_town_alone_resolves() {
    assert_eq!(
        found(&zealand(), "Auckland")[0].precision,
        Precision::Locality
    );
}
