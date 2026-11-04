//! Packs that name their datasets, and packs that only meet at search time.

use std::path::PathBuf;
use woopwoop::{search_all, Outcome, Pack, PackBuilder, Precision, Record};

fn path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("woopwoop-datasets-{}-{name}", std::process::id()))
}

fn record(number: &str, street: &str, locality: &str, postcode: &str, lat: f64) -> Record {
    Record {
        street: street.into(),
        number: number.into(),
        locality: locality.into(),
        postcode: postcode.into(),
        region: "VIC".into(),
        lat,
        lon: 145.0,
    }
}

fn top(packs: &[Pack], text: &str) -> woopwoop::Candidate {
    match search_all(packs, text) {
        Outcome::Found(mut found) => found.remove(0),
        Outcome::Declined(why) => panic!("{text}: {why:?}"),
    }
}

#[test]
fn a_listed_house_names_its_dataset() {
    let file = path("named.pack");
    let mut b = PackBuilder::new("test release");
    b.add_from(
        &record("1", "Smith Street", "Fitzroy", "3065", -37.80),
        "city/one",
    );
    b.add_from(
        &record("2", "Smith Street", "Fitzroy", "3065", -37.81),
        "city/two",
    );
    b.add_from(
        &record("3", "Smith Street", "Fitzroy", "3065", -37.82),
        "city/two",
    );
    b.finish(&file).unwrap();
    let pack = Pack::open(&file).unwrap();

    assert_eq!(pack.datasets(), vec![("city/one", 1), ("city/two", 2)]);
    let packs = [pack];
    assert_eq!(
        top(&packs, "2 Smith St, Fitzroy VIC 3065").source,
        "city/two"
    );
    // A place draws on many records, so it credits the pack.
    assert_eq!(top(&packs, "Fitzroy VIC").source, "test release");
    std::fs::remove_file(&file).ok();
}

#[test]
fn a_pack_with_no_datasets_credits_its_source() {
    let file = path("plain.pack");
    let mut b = PackBuilder::new("test release");
    b.add(&record("1", "Smith Street", "Fitzroy", "3065", -37.80));
    b.finish(&file).unwrap();
    let pack = Pack::open(&file).unwrap();

    assert!(pack.datasets().is_empty());
    assert_eq!(
        top(&[pack], "1 Smith St, Fitzroy VIC 3065").source,
        "test release"
    );
    std::fs::remove_file(&file).ok();
}

#[test]
fn an_address_with_no_town_is_found_by_postcode() {
    let file = path("no-town.pack");
    let mut b = PackBuilder::new("addresses");
    b.add(&record("7", "Quiet Lane", "", "3999", -37.90));
    b.finish(&file).unwrap();
    let found = top(&[Pack::open(&file).unwrap()], "7 Quiet Lane, VIC 3999");

    assert_eq!(found.precision, Precision::House);
    std::fs::remove_file(&file).ok();
}

#[test]
fn an_address_with_no_town_is_found_in_a_town_from_another_pack() {
    let (addresses, map) = (path("blank.pack"), path("towns.pack"));
    let mut b = PackBuilder::new("addresses");
    b.add(&record("9", "Quiet Lane", "", "", -36.01));
    b.add(&record("9", "Quiet Lane", "", "", -30.00));
    b.finish(&addresses).unwrap();
    let mut b = PackBuilder::new("towns");
    b.add_place("Ghost Town", "VIC", -36.0, 145.0, 10);
    b.finish(&map).unwrap();
    let packs = [Pack::open(&addresses).unwrap(), Pack::open(&map).unwrap()];
    let found = top(&packs, "9 Quiet Lane, Ghost Town VIC");

    assert_eq!(
        (found.precision, found.source.as_str()),
        (Precision::House, "addresses")
    );
    assert!((found.lat + 36.01).abs() < 0.001);
    std::fs::remove_file(&addresses).ok();
    std::fs::remove_file(&map).ok();
}

#[test]
fn a_house_with_no_town_beats_the_street_in_the_town_asked_for() {
    let (addresses, map) = (path("street.pack"), path("street-towns.pack"));
    let mut b = PackBuilder::new("addresses");
    b.add(&record("1", "Smith Street", "Fitzroy", "3065", -37.80));
    b.add(&record("3", "Smith Street", "Fitzroy", "3065", -37.801));
    b.add(&record("5", "Smith Street", "", "", -37.802));
    b.finish(&addresses).unwrap();
    let mut b = PackBuilder::new("towns");
    b.add_place("Fitzroy", "VIC", -37.80, 145.0, 10);
    b.finish(&map).unwrap();
    let packs = [Pack::open(&addresses).unwrap(), Pack::open(&map).unwrap()];

    assert_eq!(
        top(&packs, "5 Smith St, Fitzroy VIC").precision,
        Precision::House
    );
    std::fs::remove_file(&addresses).ok();
    std::fs::remove_file(&map).ok();
}

#[test]
fn a_house_nearer_another_town_is_not_offered_as_in_this_one() {
    let (addresses, map) = (path("boroughs.pack"), path("borough-towns.pack"));
    let mut b = PackBuilder::new("addresses");
    b.add(&record("1", "Fifth Avenue", "", "", -37.80));
    b.add(&record("350", "Fifth Avenue", "", "", -37.86));
    b.finish(&addresses).unwrap();
    let mut b = PackBuilder::new("towns");
    b.add_place("Midtown", "VIC", -37.80, 145.0, 100);
    b.add_place("Southside", "VIC", -37.86, 145.0, 100);
    b.finish(&map).unwrap();
    let packs = [Pack::open(&addresses).unwrap(), Pack::open(&map).unwrap()];

    let wrong = top(&packs, "350 Fifth Ave, Midtown VIC");
    assert_ne!(wrong.precision, Precision::House);
    assert_eq!(
        top(&packs, "350 Fifth Ave, Southside VIC").precision,
        Precision::House
    );
    std::fs::remove_file(&addresses).ok();
    std::fs::remove_file(&map).ok();
}

#[test]
fn a_town_alone_comes_from_the_town_list() {
    let (addresses, map) = (path("centre.pack"), path("centre-towns.pack"));
    let mut b = PackBuilder::new("addresses");
    b.add(&record("1", "Edge Road", "Bigtown", "3999", -38.50));
    b.finish(&addresses).unwrap();
    let mut b = PackBuilder::new("towns");
    b.add_place("Bigtown", "VIC", -38.00, 145.0, 5000);
    b.finish(&map).unwrap();
    let packs = [Pack::open(&addresses).unwrap(), Pack::open(&map).unwrap()];
    let found = top(&packs, "Bigtown VIC");

    assert_eq!(found.source, "towns");
    assert!((found.lat + 38.0).abs() < 0.001);
    std::fs::remove_file(&addresses).ok();
    std::fs::remove_file(&map).ok();
}

#[test]
fn a_house_with_no_town_shows_the_town_from_another_pack() {
    let (addresses, map) = (path("label.pack"), path("label-towns.pack"));
    let mut b = PackBuilder::new("addresses");
    b.add(&record("9", "Quiet Lane", "", "3999", -36.01));
    b.finish(&addresses).unwrap();
    let mut b = PackBuilder::new("towns");
    b.add_place("Ghost Town", "VIC", -36.0, 145.0, 10);
    b.finish(&map).unwrap();
    let packs = [Pack::open(&addresses).unwrap(), Pack::open(&map).unwrap()];

    for text in ["9 Quiet Lane, Ghost Town VIC", "9 Quiet Lane, VIC 3999"] {
        let found = top(&packs, text);
        assert_eq!(found.locality, "Ghost Town", "{text}");
        assert_eq!(found.label, "9 Quiet Lane, Ghost Town VIC 3999", "{text}");
    }
    let alone = [Pack::open(&addresses).unwrap()];
    assert_eq!(
        top(&alone, "9 Quiet Lane, VIC 3999").label,
        "9 Quiet Lane, VIC 3999"
    );
    std::fs::remove_file(&addresses).ok();
    std::fs::remove_file(&map).ok();
}

#[test]
fn a_town_in_another_pack_marks_where_a_street_ends() {
    let (addresses, map) = (path("ends.pack"), path("ends-towns.pack"));
    let mut b = PackBuilder::new("addresses");
    b.add(&record("7500", "County Road 100 West", "", "", -36.01));
    b.add(&record(
        "7500",
        "County Road 100",
        "Elsewhere",
        "3888",
        -30.0,
    ));
    b.finish(&addresses).unwrap();
    let mut b = PackBuilder::new("towns");
    b.add_place("Walesboro", "VIC", -36.0, 145.0, 10);
    b.finish(&map).unwrap();
    let packs = [Pack::open(&addresses).unwrap(), Pack::open(&map).unwrap()];
    let found = top(&packs, "7500 County Road 100 West Walesboro VIC");

    assert_eq!(found.precision, Precision::House);
    assert!((found.lat + 36.01).abs() < 0.001, "{found:?}");
    std::fs::remove_file(&addresses).ok();
    std::fs::remove_file(&map).ok();
}

#[test]
fn separate_packs_fill_each_others_gaps_at_search_time() {
    let (addresses, map) = (path("addresses.pack"), path("map.pack"));
    let mut b = PackBuilder::new("addresses");
    b.add(&record("1", "Smith Street", "Fitzroy", "3065", -37.80));
    b.finish(&addresses).unwrap();
    let mut b = PackBuilder::new("towns and roads");
    b.add_place("Ghost Town", "VIC", -36.0, 145.0, 10);
    b.add_road("Lonely Road", -36.001, 145.0);
    b.finish(&map).unwrap();
    let packs = [Pack::open(&addresses).unwrap(), Pack::open(&map).unwrap()];

    assert_eq!(
        top(&packs, "1 Smith St, Fitzroy VIC 3065").source,
        "addresses"
    );
    assert_eq!(top(&packs, "Ghost Town VIC").source, "towns and roads");
    let road = top(&packs, "Lonely Road, Ghost Town VIC");
    assert_eq!(
        (road.precision, road.source.as_str()),
        (Precision::Street, "towns and roads")
    );
    std::fs::remove_file(&addresses).ok();
    std::fs::remove_file(&map).ok();
}
