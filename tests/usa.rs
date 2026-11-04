//! American addresses, on a small made-up fixture.

use std::sync::OnceLock;
use woopwoop::{Candidate, Country, Declined, Outcome, Pack, PackBuilder, Precision, Record};

fn record(
    number: &str,
    street: &str,
    city: &str,
    zip: &str,
    state: &str,
    lat: f64,
    lon: f64,
) -> Record {
    Record {
        street: street.into(),
        number: number.into(),
        locality: city.into(),
        postcode: zip.into(),
        region: state.into(),
        lat,
        lon,
    }
}

fn pack() -> &'static Pack {
    static PACK: OnceLock<Pack> = OnceLock::new();
    PACK.get_or_init(|| {
        let mut b = PackBuilder::for_country("test fixture", Country::UnitedStates);
        b.add(&record(
            "100",
            "Main Street",
            "Springfield",
            "62701",
            "IL",
            39.80,
            -89.65,
        ));
        b.add(&record(
            "100",
            "Main Street",
            "Springfield",
            "01103",
            "MA",
            42.10,
            -72.59,
        ));
        b.add(&record(
            "42050",
            "South Sasabe Highway",
            "Sasabe",
            "85633",
            "AZ",
            31.48,
            -111.54,
        ));
        b.add(&record(
            "350",
            "Fifth Avenue",
            "New York",
            "10118",
            "NY",
            40.748,
            -73.985,
        ));
        b.add(&record(
            "1600",
            "Pennsylvania Avenue Northwest",
            "Washington",
            "20500",
            "DC",
            38.897,
            -77.036,
        ));
        b.add(&record(
            "7",
            "Martin Luther King Jr Boulevard",
            "Atlanta",
            "30303",
            "GA",
            33.75,
            -84.39,
        ));
        b.add(&record(
            "18563",
            "County Road 26",
            "Tyler",
            "75707",
            "TX",
            32.3,
            -95.2,
        ));
        b.add(&record(
            "5911",
            "Montoya Drive",
            "CITY OF EL PASO",
            "79932",
            "TX",
            31.87,
            -106.59,
        ));
        b.add(&record(
            "104",
            "West Bird Street",
            "Westbrook",
            "79565",
            "TX",
            32.35,
            -101.01,
        ));
        b.add(&record(
            "1304",
            "Idaho Street",
            "Baytown",
            "77520",
            "TX",
            29.74,
            -94.97,
        ));
        b.add(&record("9", "Idaho", "Boise", "83702", "ID", 43.6, -116.2));
        b.add(&record(
            "11",
            "Bolton Court",
            "Kissimmee",
            "34758",
            "FL",
            28.2,
            -81.5,
        ));
        b.add_place("Las Vegas", "NV", 36.17, -115.14, 641000);
        b.add_place("Las Vegas", "NM", 35.59, -105.22, 13000);
        b.add_place("Reno", "NV", 39.53, -119.81, 264000);
        b.add_road("West Tropicana Avenue", 36.10, -115.20);
        b.add(&record(
            "9",
            "Sahara Avenue",
            "",
            "89102",
            "NV",
            36.15,
            -115.16,
        ));
        let path = std::env::temp_dir().join(format!("woopwoop-usa-{}.pack", std::process::id()));
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

#[test]
fn reads_american_addresses() {
    for text in [
        "350 Fifth Avenue, New York, NY 10118",
        "350 Fifth Ave New York NY",
        "350 Fifth Ave, Suite 3300, New York, NY 10118",
        "350 Fifth Ave Apt 4B, New York NY",
        "350 Fifth Ave #12, New York, New York 10118",
        "350 Fifth Ave, New York, NY 10118-0110",
        "350 Fifth Avenue, New York, NY, USA",
    ] {
        let c = &found(text)[0];
        assert_eq!(
            (c.precision, c.region.as_str()),
            (Precision::House, "NY"),
            "{text}"
        );
    }
}

#[test]
fn state_tells_two_springfields_apart() {
    assert_eq!(found("100 Main St, Springfield, IL")[0].region, "IL");
    assert_eq!(found("100 Main St, Springfield, MA")[0].region, "MA");
    assert_eq!(
        found("100 Main St, Springfield, Massachusetts")[0].region,
        "MA"
    );
    assert_eq!(found("100 Main St, Springfield").len(), 2);
}

#[test]
fn a_leading_direction_can_be_abbreviated() {
    assert_eq!(
        found("42050 S Sasabe Hwy, Sasabe AZ 85633")[0].precision,
        Precision::House
    );
}

#[test]
fn numbered_county_roads_resolve() {
    assert_eq!(
        found("18563 County Rd 26, Tyler, TX 75707")[0].precision,
        Precision::House
    );
}

#[test]
fn city_of_is_optional() {
    let c = &found("5911 Montoya Dr, El Paso, TX")[0];
    assert_eq!(
        (c.precision, c.locality.as_str()),
        (Precision::House, "El Paso")
    );
}

#[test]
fn a_misspelled_street_type_is_forgiven() {
    assert_eq!(
        found("104 West Bird Steret, Westbrook, TX 79565")[0].precision,
        Precision::House
    );
}

#[test]
fn a_street_that_exists_elsewhere_does_not_swallow_a_typo() {
    let c = &found("1304 Idaho Steret, Baytown, TX 77520")[0];
    assert_eq!(
        (c.precision, c.locality.as_str()),
        (Precision::House, "Baytown")
    );
}

#[test]
fn ct_can_be_a_court_as_well_as_connecticut() {
    let c = &found("11 Bolton Ct, 34758")[0];
    assert_eq!((c.precision, c.region.as_str()), (Precision::House, "FL"));
}

#[test]
fn an_official_town_needs_no_addresses() {
    let c = &found("Las Vegas, NV")[0];
    assert_eq!(
        (c.precision, c.locality.as_str()),
        (Precision::Locality, "Las Vegas")
    );
    assert!((c.lat - 36.17).abs() < 0.01);
}

#[test]
fn the_bigger_town_comes_first() {
    assert_eq!(found("Las Vegas")[0].region, "NV");
}

#[test]
fn a_road_with_no_addresses_still_resolves_to_the_street() {
    let c = &found("4500 W Tropicana Ave, Las Vegas, NV")[0];
    assert_eq!(c.precision, Precision::Street);
    assert!((c.lat - 36.10).abs() < 0.01);
}

#[test]
fn an_address_with_no_town_takes_the_nearest() {
    assert_eq!(
        found("9 Sahara Ave, Las Vegas, NV")[0].precision,
        Precision::House
    );
}

#[test]
fn every_spelling_of_fifth_avenue_works() {
    for text in [
        "350 5th Ave, New York, NY",
        "350 Fifth Avenue, New York, NY",
        "350 5 Ave, New York, NY",
    ] {
        assert_eq!(found(text)[0].precision, Precision::House, "{text}");
    }
}

#[test]
fn a_full_compass_point_matches_its_initials() {
    assert_eq!(
        found("1600 Pennsylvania Ave NW, Washington, DC")[0].precision,
        Precision::House
    );
}

#[test]
fn labels_use_the_street_as_written() {
    assert_eq!(
        found("350 5th Ave, New York, NY")[0].label,
        "350 Fifth Avenue, New York NY 10118"
    );
}

#[test]
fn a_typo_in_a_route_word_is_forgiven() {
    assert_eq!(
        found("18563 Coutny Road 26, Tyler, TX 75707")[0].precision,
        Precision::House
    );
    assert_eq!(
        found("42050 Suoth Sasabe Hwy, Sasabe AZ 85633")[0].precision,
        Precision::House
    );
}

#[test]
fn zip_alone_is_a_place() {
    assert_eq!(found("62701")[0].locality, "Springfield");
}

#[test]
fn po_boxes_are_declined() {
    assert_eq!(
        pack().search("PO Box 100, Springfield IL"),
        Outcome::Declined(Declined::PostalAddress)
    );
}
