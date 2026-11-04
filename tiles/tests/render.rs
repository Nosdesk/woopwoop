//! Behaviour on small hand-built tiles, so these run with no data files.

use woopwoop_tiles::{base, Labels, Theme};

fn varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

fn field(out: &mut Vec<u8>, number: u64, bytes: &[u8]) {
    varint(out, number << 3 | 2);
    varint(out, bytes.len() as u64);
    out.extend_from_slice(bytes);
}

fn zigzag(v: i32) -> u64 {
    ((v << 1) ^ (v >> 31)) as u32 as u64
}

fn line(points: &[(i32, i32)]) -> Vec<u8> {
    let mut g = Vec::new();
    let (mut x, mut y) = (0, 0);
    varint(&mut g, 1 | 1 << 3);
    varint(&mut g, zigzag(points[0].0));
    varint(&mut g, zigzag(points[0].1));
    (x, y) = (x + points[0].0, y + points[0].1);
    varint(&mut g, 2 | ((points.len() as u64 - 1) << 3));
    for p in &points[1..] {
        varint(&mut g, zigzag(p.0 - x));
        varint(&mut g, zigzag(p.1 - y));
        (x, y) = *p;
    }
    g
}

/// A tile with one layer of features, each `(type, kind, name, geometry)`.
fn tile(layer: &str, features: &[(u64, &str, &str, Vec<u8>)]) -> Vec<u8> {
    let mut l = Vec::new();
    field(&mut l, 1, layer.as_bytes());
    field(&mut l, 3, b"kind");
    field(&mut l, 3, b"name");
    for (i, (kind, k, n, geometry)) in features.iter().enumerate() {
        let mut f = Vec::new();
        let mut tags = Vec::new();
        for v in [0, 2 * i as u64, 1, 2 * i as u64 + 1] {
            varint(&mut tags, v);
        }
        field(&mut f, 2, &tags);
        varint(&mut f, 3 << 3);
        varint(&mut f, *kind);
        field(&mut f, 4, geometry);
        field(&mut l, 2, &f);
        let _ = (k, n);
    }
    for (_, k, n, _) in features {
        for s in [k, n] {
            let mut v = Vec::new();
            field(&mut v, 1, s.as_bytes());
            field(&mut l, 4, &v);
        }
    }
    let mut t = Vec::new();
    field(&mut t, 3, &l);
    t
}

#[test]
fn draws_a_road() {
    let t = tile(
        "roads",
        &[(2, "major_road", "High St", line(&[(0, 0), (4000, 0)]))],
    );
    let svg = base(&t, 15, &Theme::DARK);
    assert!(svg.starts_with("<svg "));
    assert!(svg.contains("M0 0l2000 0"), "{svg}");
    assert!(svg.contains(Theme::DARK.major));
}

#[test]
fn footpaths_wait_for_street_level() {
    let t = tile("roads", &[(2, "path", "", line(&[(0, 0), (4000, 0)]))]);
    assert!(base(&t, 15, &Theme::DARK).contains("M0 0l2000 0"));
    assert!(!base(&t, 14, &Theme::DARK).contains("M0 0l2000 0"));
}

#[test]
fn a_damaged_tile_draws_an_empty_map() {
    for t in [vec![0xff; 64], vec![0x1a, 0xff, 0xff], Vec::new()] {
        assert!(base(&t, 15, &Theme::LIGHT).ends_with("</svg>"));
        let mut l = Labels::new();
        l.add(&t, 15, 0, 0);
        assert!(l.draw(15, 0, 0, &Theme::LIGHT).ends_with("</svg>"));
    }
}

#[test]
fn a_road_name_is_drawn_once_along_the_road() {
    let t = tile(
        "roads",
        &[(
            2,
            "medium_road",
            "Collins St",
            line(&[(100, 2000), (3900, 2000)]),
        )],
    );
    let mut l = Labels::new();
    l.add(&t, 15, 0, 0);
    let svg = l.draw(15, 0, 0, &Theme::DARK);
    assert_eq!(svg.matches("Collins St").count(), 1);
    assert!(svg.contains("textPath"));
}

#[test]
fn text_runs_the_right_way_up() {
    let t = tile(
        "roads",
        &[(
            2,
            "medium_road",
            "Reversed Rd",
            line(&[(3900, 2000), (100, 2000)]),
        )],
    );
    let mut l = Labels::new();
    l.add(&t, 15, 0, 0);
    assert!(l.draw(15, 0, 0, &Theme::DARK).contains("d=\"M50 1000"));
}

#[test]
fn neighbouring_tiles_agree_on_a_repeated_name() {
    // The same road either side of a tile edge, labelled close together.
    let road = |from, to| {
        tile(
            "roads",
            &[(
                2,
                "medium_road",
                "Long Rd",
                line(&[(from, 2000), (to, 2000)]),
            )],
        )
    };
    let (left, right) = (road(3000, 4000), road(0, 1000));
    let mut l = Labels::new();
    l.add(&left, 15, 0, 0);
    l.add(&right, 15, 1, 0);
    let drawn = |x| l.draw(15, x, 0, &Theme::DARK).matches("Long Rd").count();
    assert_eq!(drawn(0) + drawn(1), 1);
}

#[test]
fn a_name_may_repeat_once_it_is_far_enough_away() {
    let road = tile(
        "roads",
        &[(
            2,
            "medium_road",
            "Long Rd",
            line(&[(100, 2000), (3900, 2000)]),
        )],
    );
    let mut l = Labels::new();
    l.add(&road, 15, 0, 0);
    l.add(&road, 15, 1, 0);
    let drawn = |x| l.draw(15, x, 0, &Theme::DARK).matches("Long Rd").count();
    assert_eq!(drawn(0) + drawn(1), 2);
}

#[test]
fn names_are_escaped() {
    let t = tile(
        "roads",
        &[(
            2,
            "medium_road",
            "A & B <Rd> \"x\" of a good length here",
            line(&[(100, 2000), (3900, 2000)]),
        )],
    );
    let mut l = Labels::new();
    l.add(&t, 15, 0, 0);
    let svg = l.draw(15, 0, 0, &Theme::DARK);
    assert!(svg.contains("A &amp; B &lt;Rd&gt; &quot;x&quot;"), "{svg}");
    assert!(!svg.contains("<Rd>"));
}

#[test]
fn memory_stays_bounded() {
    let mut l = Labels::new();
    for i in 0..1000 {
        l.add(&[], 15, i, 0);
    }
    assert!(l.has(15, 999, 0));
    assert!(!l.has(15, 0, 0));
}
