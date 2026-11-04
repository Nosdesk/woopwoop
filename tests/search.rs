//! Behaviour on a small made-up country, so these run anywhere with no data files.

use std::path::PathBuf;
use std::sync::OnceLock;
use woopwoop::{Candidate, Declined, Outcome, Pack, PackBuilder, Precision, Query, Record};

fn record(
    number: &str,
    street: &str,
    locality: &str,
    postcode: &str,
    region: &str,
    lat: f64,
    lon: f64,
) -> Record {
    Record {
        street: street.into(),
        number: number.into(),
        locality: locality.into(),
        postcode: postcode.into(),
        region: region.into(),
        lat,
        lon,
    }
}

fn pack() -> &'static Pack {
    static PACK: OnceLock<Pack> = OnceLock::new();
    PACK.get_or_init(|| {
        let mut b = PackBuilder::new("test fixture");
        for n in [2, 4, 6, 8, 20, 22] {
            b.add(&record(
                &n.to_string(),
                "HIGH STREET",
                "WINDSOR",
                "3181",
                "VIC",
                -37.8500 - f64::from(n) * 0.0001,
                145.0,
            ));
        }
        b.add(&record(
            "12",
            "HIGH STREET",
            "ARMADALE",
            "3143",
            "VIC",
            -37.8600,
            145.0200,
        ));
        b.add(&record(
            "4",
            "HIGH STREET",
            "PERTH",
            "6000",
            "WA",
            -31.9500,
            115.8600,
        ));
        b.add(&record(
            "12",
            "ST KILDA ROAD",
            "ST KILDA",
            "3182",
            "VIC",
            -37.8670,
            144.9810,
        ));
        b.add(&record(
            "100",
            "COLLINS STREET",
            "MELBOURNE",
            "3000",
            "VIC",
            -37.8150,
            144.9700,
        ));
        b.add(&record(
            "2",
            "SWANSTON STREET",
            "MELBOURNE",
            "3000",
            "VIC",
            -37.8151,
            144.9701,
        ));
        b.add(&record(
            "5",
            "THE AVENUE",
            "WINDSOR",
            "3181",
            "VIC",
            -37.8560,
            144.9950,
        ));
        b.add(&record(
            "7",
            "WATTLETREE ROAD",
            "MALVERN",
            "3144",
            "VIC",
            -37.8620,
            145.0300,
        ));
        b.add(&record(
            "9",
            "TIDE STREET",
            "WEST MALVERN",
            "3144",
            "VIC",
            -37.8630,
            145.0310,
        ));
        b.add(&record(
            "9",
            "TIDE STREET W",
            "MALVERN",
            "3144",
            "VIC",
            -37.8700,
            145.0400,
        ));
        b.add(&record(
            "8",
            "JEAN STREET",
            "FITZROY",
            "3065",
            "VIC",
            -37.8000,
            144.9800,
        ));
        b.add(&record(
            "2",
            "JENA STREET",
            "PERTH",
            "6000",
            "WA",
            -31.9600,
            115.8700,
        ));
        b.add(&record(
            "11", "KINGSWAY", "WINDSOR", "3181", "VIC", -37.8560, 144.9920,
        ));
        b.add(&record(
            "15",
            "MOUNT PLEASANT ROAD",
            "MALVERN",
            "3144",
            "VIC",
            -37.8640,
            145.0320,
        ));
        b.add(&record(
            "3",
            "OUTBACK TRACK",
            "WOOP WOOP",
            "6000",
            "WA",
            -28.0000,
            121.0000,
        ));
        let path: PathBuf =
            std::env::temp_dir().join(format!("woopwoop-test-{}.pack", std::process::id()));
        b.finish(&path).expect("build");
        let p = Pack::open(&path).expect("open");
        let _ = std::fs::remove_file(&path);
        p
    })
}

fn found(text: &str) -> Vec<Candidate> {
    match pack().search(text) {
        Outcome::Found(c) => c,
        other => panic!("{text}: {other:?}"),
    }
}

fn declined(text: &str) -> Declined {
    match pack().search(text) {
        Outcome::Declined(d) => d,
        other => panic!("{text}: {other:?}"),
    }
}

#[test]
fn finds_a_plain_address() {
    let c = &found("4 High Street, Windsor VIC 3181")[0];
    assert_eq!(c.precision, Precision::House);
    assert_eq!(c.locality, "Windsor");
    assert!((c.lat - -37.8504).abs() < 1e-6);
    assert_eq!(c.label, "4 High St, Windsor VIC 3181");
}

#[test]
fn reads_the_ways_people_write_addresses() {
    for text in [
        "4 high st windsor 3181",
        "3/4 High St, Windsor",
        "Unit 3, 4 High St Windsor VIC",
        "Level 2, 4 High Street, Windsor VIC 3181",
        "Shop 3 4 High St Windsor",
        "4-6 High St, Windsor 3181",
        "Acme Plumbing Pty Ltd, 4 High St, Windsor VIC 3181",
        "4 High Street Windsor Victoria Australia",
    ] {
        let c = &found(text)[0];
        assert_eq!(
            (c.precision, c.locality.as_str()),
            (Precision::House, "Windsor"),
            "{text}"
        );
    }
}

#[test]
fn a_suburb_can_share_its_name_with_the_street() {
    let c = &found("12 St Kilda Rd, St Kilda VIC 3182")[0];
    assert_eq!(
        (c.precision, c.locality.as_str()),
        (Precision::House, "St Kilda")
    );
}

#[test]
fn states_with_australia_in_the_name_survive() {
    let c = &found("4 High Street Perth Western Australia")[0];
    assert_eq!(c.region, "WA");
}

#[test]
fn a_direction_can_belong_to_the_suburb() {
    assert_eq!(
        found("7 Wattletree Rd West Malvern")[0].precision,
        Precision::House
    );
    assert_eq!(
        found("9 Tide Street West Malvern")[0].locality,
        "West Malvern"
    );
}

#[test]
fn a_typo_and_a_wrong_suburb_together_is_too_much_guessing() {
    // Close to "Wattletree Rd" in Malvern, but the suburb asked for is Windsor.
    let c = found("7 Wattletere Rd, Windsor VIC 3181");
    assert!(c.iter().all(|x| x.precision != Precision::House), "{c:?}");
}

#[test]
fn a_street_needs_no_type_word() {
    assert_eq!(
        found("11 Kingsway, Windsor VIC 3181")[0].precision,
        Precision::House
    );
}

#[test]
fn mount_and_mt_are_the_same_and_typos_still_land() {
    for text in [
        "15 Mt Pleasant Rd, Malvern",
        "15 Mount Pleasant Road, Malvern",
        "15 Muont Pleasant Road, Malvern",
    ] {
        assert_eq!(found(text)[0].precision, Precision::House, "{text}");
    }
}

#[test]
fn a_real_street_can_still_be_a_typo() {
    let c = &found("8 Jena St, Fitzroy VIC 3065")[0];
    assert_eq!(
        (c.precision, c.locality.as_str()),
        (Precision::House, "Fitzroy")
    );
}

#[test]
fn postcode_rescues_a_wrong_suburb() {
    assert_eq!(found("4 High St, Nowhereville 3181")[0].locality, "Windsor");
}

#[test]
fn a_typo_in_the_street_is_forgiven() {
    let c = &found("7 Wattletere Rd, Malvern VIC 3144")[0];
    assert_eq!(
        (c.precision, c.locality.as_str()),
        (Precision::House, "Malvern")
    );
}

#[test]
fn an_ambiguous_address_offers_every_match() {
    let c = found("4 High St");
    assert_eq!(c.len(), 2);
    assert!(c.iter().all(|x| x.precision == Precision::House));
}

#[test]
fn a_missing_number_is_placed_between_its_neighbours() {
    let c = &found("10 High St, Windsor VIC 3181")[0];
    assert_eq!(c.precision, Precision::Street);
    assert!((c.lat - -37.8510).abs() < 1e-6, "{}", c.lat);
}

#[test]
fn numbers_on_the_other_side_of_the_road_are_not_used() {
    assert_eq!(
        found("9 High St, Windsor VIC 3181")[0].precision,
        Precision::Street
    );
}

#[test]
fn a_street_with_no_number_points_at_the_street() {
    let c = &found("High St, Windsor")[0];
    assert_eq!(c.precision, Precision::Street);
    assert_eq!(c.label, "High St, Windsor VIC 3181");
}

#[test]
fn a_suburb_alone_is_a_locality() {
    assert_eq!(found("Windsor VIC")[0].precision, Precision::Locality);
    assert_eq!(found("3181")[0].locality, "Windsor");
}

#[test]
fn woop_woop_is_findable() {
    assert_eq!(
        found("3 Outback Track, Woop Woop")[0].precision,
        Precision::House
    );
}

#[test]
fn corners_land_where_the_streets_meet() {
    let c = &found("Cnr Collins St and Swanston St, Melbourne")[0];
    assert_eq!(c.precision, Precision::Street);
    assert!((c.lat - -37.81505).abs() < 1e-4);
    assert!(found("Corner of Collins and Swanston, Melbourne")[0]
        .label
        .starts_with("Corner of"));
}

#[test]
fn postal_addresses_are_declined() {
    for text in [
        "PO Box 123, Windsor VIC 3181",
        "GPO Box 1234 Melbourne VIC 3000",
        "Locked Bag 7, Melbourne",
    ] {
        assert_eq!(declined(text), Declined::PostalAddress, "{text}");
    }
}

#[test]
fn nonsense_is_declined_rather_than_guessed() {
    assert_eq!(
        declined("the blue house near the station"),
        Declined::Unreadable
    );
    assert_eq!(declined(""), Declined::Unreadable);
    assert_eq!(declined("123"), Declined::Unreadable);
}

#[test]
fn a_match_in_a_neighbouring_suburb_is_flagged() {
    let c = &found("4 High St, Malvern VIC 3144")[0];
    assert_eq!(c.locality, "Windsor");
    assert!(!c.place_confirmed);
    assert!(found("4 High St, Windsor VIC 3181")[0].place_confirmed);
    assert!(found("4 High St, Nowhereville 3181")[0].place_confirmed);
}

#[test]
fn a_street_far_from_the_suburb_asked_for_is_not_offered() {
    let r = pack().search("3 Outback Track, Windsor VIC 3181");
    if let Outcome::Found(c) = r {
        assert!(
            c.iter()
                .all(|x| x.precision == Precision::Locality && x.locality == "Windsor"),
            "{c:?}"
        );
    }
}

#[test]
fn structured_lookups_work() {
    let q = Query {
        number: Some("4".into()),
        street: "High Street".into(),
        locality: "Windsor".into(),
        postcode: "3181".into(),
        region: "VIC".into(),
    };
    match pack().lookup(&q) {
        Outcome::Found(c) => assert_eq!(c[0].precision, Precision::House),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_corrupt_file_is_an_error_not_a_panic() {
    let path = std::env::temp_dir().join(format!("woopwoop-bad-{}.pack", std::process::id()));
    std::fs::write(
        &path,
        b"WOOPWOOPnot really a pack, this one is far too short to be real at all, is it now, no",
    )
    .expect("write");
    assert!(Pack::open(&path).is_err());
    let _ = std::fs::remove_file(&path);
}
