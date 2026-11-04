// Lists the addresses in an OpenStreetMap extract as number, street, city, postcode,
// region, lat, lon, for `woopwoop build-overture`.
// Usage: osm_addresses <extract.osm.pbf> <region> <out.tsv>
use osmpbf::{Element, ElementReader};
use std::io::Write;
fn tag<'a>(t: &[(&'a str, &'a str)], k: &str) -> Option<&'a str> { t.iter().find(|(a, _)| *a == k).map(|(_, v)| *v) }
fn clean(s: &str) -> String { s.replace(['\t', '\n'], " ") }
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (pbf, state, out) = (&a[1], &a[2], &a[3]);
    let mut w = std::io::BufWriter::new(std::fs::File::create(out).unwrap());
    let mut nodes: Vec<(i64, f64, f64)> = vec![];
    let mut ways: Vec<(Vec<i64>, String)> = vec![];
    let (mut n, mut wn) = (0u64, 0u64);
    let row = |t: &[(&str, &str)]| -> Option<String> {
        let (num, st) = (tag(t, "addr:housenumber")?, tag(t, "addr:street")?);
        Some(format!("{}\t{}\t{}\t{}\t{}", clean(num), clean(st), clean(tag(t, "addr:city").unwrap_or("")), clean(tag(t, "addr:postcode").unwrap_or("")), state))
    };
    ElementReader::from_path(pbf).unwrap().for_each(|e| match e {
        Element::DenseNode(d) => { nodes.push((d.id(), d.lat(), d.lon())); let t: Vec<_> = d.tags().collect(); if let Some(r) = row(&t) { writeln!(w, "{r}\t{:.7}\t{:.7}", d.lat(), d.lon()).unwrap(); n += 1; } }
        Element::Node(d) => { nodes.push((d.id(), d.lat(), d.lon())); let t: Vec<_> = d.tags().collect(); if let Some(r) = row(&t) { writeln!(w, "{r}\t{:.7}\t{:.7}", d.lat(), d.lon()).unwrap(); n += 1; } }
        Element::Way(x) => { let t: Vec<_> = x.tags().collect(); if let Some(r) = row(&t) { ways.push((x.refs().collect(), r)); } }
        _ => {}
    }).unwrap();
    nodes.sort_unstable_by_key(|n| n.0);
    for (refs, r) in ways {
        let (mut la, mut lo, mut c) = (0.0, 0.0, 0.0);
        for id in &refs { if let Ok(i) = nodes.binary_search_by_key(id, |n| n.0) { la += nodes[i].1; lo += nodes[i].2; c += 1.0; } }
        if c > 0.0 { writeln!(w, "{r}\t{:.7}\t{:.7}", la / c, lo / c).unwrap(); wn += 1; }
    }
    println!("{state}: {} addresses ({n} nodes, {wn} ways)", n + wn);
}
