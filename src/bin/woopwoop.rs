//! Command line for building and checking packs.

use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;
use woopwoop::{gnaf, search_all, Country, Outcome, Pack, PackBuilder, Precision, Record};

const USAGE: &str = "usage:
  woopwoop build-gnaf <G-NAF Standard folder> <out.pack> [source label]
  woopwoop build-tsv <tsv of: number, street, locality, postcode, region, lat, lon, optional dataset> <out.pack> <source label> <AU|US>
  woopwoop build-overture <addresses.tsv> <places.tsv> <roads.tsv> <out.pack> <source label> <AU|US|CA|NZ|GB>
  woopwoop datasets <pack>
  woopwoop search <pack[,pack]> <address>
  woopwoop eval <pack[,pack]> <tsv of: text, lat, lon, optional group>";

fn metres(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (la, lb) = (a.0.to_radians(), b.0.to_radians());
    let x = ((b.0 - a.0).to_radians() / 2.0).sin().powi(2)
        + la.cos() * lb.cos() * ((b.1 - a.1).to_radians() / 2.0).sin().powi(2);
    12_742_000.0 * x.sqrt().asin()
}

#[derive(Default)]
struct Tally {
    n: u32,
    near: u32,
    coarse: u32,
    wrong: u32,
    none: u32,
}

/// Opens one pack, or several separated by commas.
fn open_packs(list: &str) -> Result<Vec<Pack>, woopwoop::Error> {
    list.split(',').map(Pack::open).collect()
}

fn eval(packs: &[Pack], path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let text = std::fs::read_to_string(path)?;
    let mut groups: BTreeMap<String, Tally> = BTreeMap::new();
    let mut times = Vec::new();
    let show = std::env::var_os("WOOPWOOP_SHOW_MISSES").is_some();
    for line in text.lines() {
        let c: Vec<&str> = line.split('\t').collect();
        let (Some(address), Some(lat), Some(lon)) = (c.first(), c.get(1), c.get(2)) else {
            continue;
        };
        let truth = (lat.parse::<f64>()?, lon.parse::<f64>()?);
        let t = groups
            .entry(c.get(3).copied().unwrap_or("all").to_string())
            .or_default();
        t.n += 1;
        let start = Instant::now();
        let outcome = search_all(packs, address);
        let took = start.elapsed().as_micros() as u64;
        times.push(took);
        if show && took > 50_000 {
            eprintln!("slow {}ms  {address}", took / 1000);
        }
        match outcome {
            Outcome::Found(found) => {
                let top = &found[0];
                let d = metres(truth, (top.lat, top.lon));
                if d <= 50.0 {
                    t.near += 1;
                } else if d > 1000.0 {
                    t.wrong += 1;
                    if show {
                        eprintln!("{:>8.0}m  {address}  ->  {}", d, top.label);
                    }
                }
                if top.precision != Precision::House {
                    t.coarse += 1;
                }
            }
            Outcome::Declined(_) => t.none += 1,
        }
    }
    println!(
        "{:26} {:>6} {:>8} {:>8} {:>8} {:>8}",
        "group", "n", "<=50m", ">1km", "coarse", "none"
    );
    let (mut all, mut near, mut wrong) = (0, 0, 0);
    for (name, t) in &groups {
        let p = |v: u32| 100.0 * f64::from(v) / f64::from(t.n);
        println!(
            "{name:26} {:>6} {:>7.1}% {:>7.1}% {:>7.1}% {:>7.1}%",
            t.n,
            p(t.near),
            p(t.wrong),
            p(t.coarse),
            p(t.none)
        );
        all += t.n;
        near += t.near;
        wrong += t.wrong;
    }
    times.sort_unstable();
    let at = |q: f64| times[((times.len() - 1) as f64 * q) as usize];
    println!(
        "overall: {:.1}% within 50 m, {:.1}% over 1 km out",
        100.0 * f64::from(near) / f64::from(all),
        100.0 * f64::from(wrong) / f64::from(all)
    );
    println!(
        "search: p50 {}us, p99 {}us, max {}us",
        at(0.5),
        at(0.99),
        at(1.0)
    );
    Ok(())
}

fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["build-gnaf", dir, out, ref rest @ ..] => {
            let started = Instant::now();
            let mut builder = PackBuilder::new(rest.first().copied().unwrap_or("G-NAF"));
            let read = gnaf::read(Path::new(dir), |r| builder.add(r))?;
            let kept = builder.finish(out)?;
            println!(
                "read {read}, kept {kept}, {} MB, {:.1?}",
                std::fs::metadata(out)?.len() / 1_000_000,
                started.elapsed()
            );
        }
        ["build-tsv", tsv, out, source, code] => {
            let started = Instant::now();
            let country = Country::from_code(code).ok_or("unknown country")?;
            let mut builder = PackBuilder::for_country(source, country);
            let mut read = 0u64;
            for line in std::io::BufReader::new(std::fs::File::open(tsv)?).lines() {
                let line = line?;
                let c: Vec<&str> = line.split('\t').map(|f| f.trim_matches('"')).collect();
                let (number, street, locality, postcode, region, lat, lon, dataset) = match c[..] {
                    [a, b, c, d, e, f, g] => (a, b, c, d, e, f, g, ""),
                    [a, b, c, d, e, f, g, h] => (a, b, c, d, e, f, g, h),
                    _ => continue,
                };
                let (Ok(lat), Ok(lon)) = (lat.parse(), lon.parse()) else {
                    continue;
                };
                builder.add_from(
                    &Record {
                        street: street.to_string(),
                        number: number.to_string(),
                        locality: locality.to_string(),
                        postcode: postcode.to_string(),
                        region: region.to_string(),
                        lat,
                        lon,
                    },
                    dataset,
                );
                read += 1;
            }
            let kept = builder.finish(out)?;
            println!(
                "read {read}, kept {kept}, {} MB, {:.1?}",
                std::fs::metadata(out)?.len() / 1_000_000,
                started.elapsed()
            );
        }
        ["build-overture", addresses, places, roads, out, source, code] => {
            let started = Instant::now();
            let country = Country::from_code(code).ok_or("unknown country")?;
            let mut builder = PackBuilder::for_country(source, country);
            let open = |path: &str| -> std::io::Result<_> {
                Ok(std::io::BufReader::new(std::fs::File::open(path)?).lines())
            };
            let mut counts = [0u64; 3];
            for line in open(places)? {
                let line = line?;
                let c: Vec<&str> = line.split('\t').map(|f| f.trim_matches('"')).collect();
                let [name, _, region, subtype, population, lat, lon] = c[..] else {
                    continue;
                };
                let (Ok(lat), Ok(lon)) = (lat.parse(), lon.parse()) else {
                    continue;
                };
                if subtype == "postcode" {
                    builder.add_postcode(name, lat, lon);
                    counts[0] += 1;
                } else if subtype == "locality" {
                    builder.add_place(name, region, lat, lon, population.parse().unwrap_or(0));
                    counts[0] += 1;
                }
            }
            for line in open(addresses)? {
                let line = line?;
                let c: Vec<&str> = line.split('\t').map(|f| f.trim_matches('"')).collect();
                let (number, street, locality, postcode, region, lat, lon, dataset) = match c[..] {
                    [a, b, c, d, e, f, g] => (a, b, c, d, e, f, g, ""),
                    [a, b, c, d, e, f, g, h] => (a, b, c, d, e, f, g, h),
                    _ => continue,
                };
                let (Ok(lat), Ok(lon)) = (lat.parse(), lon.parse()) else {
                    continue;
                };
                builder.add_from(
                    &Record {
                        street: street.to_string(),
                        number: number.to_string(),
                        locality: locality.to_string(),
                        postcode: postcode.to_string(),
                        region: region.to_string(),
                        lat,
                        lon,
                    },
                    dataset,
                );
                counts[1] += 1;
            }
            for line in open(roads)? {
                let line = line?;
                let c: Vec<&str> = line.split('\t').map(|f| f.trim_matches('"')).collect();
                let [name, lat, lon] = c[..] else { continue };
                let (Ok(lat), Ok(lon)) = (lat.parse(), lon.parse()) else {
                    continue;
                };
                builder.add_road(name, lat, lon);
                counts[2] += 1;
            }
            let kept = builder.finish(out)?;
            let size = std::fs::metadata(out)?.len() / 1_000_000;
            println!(
                "places {}, addresses {}, roads {}, kept {kept}, {size} MB, {:.1?}",
                counts[0],
                counts[1],
                counts[2],
                started.elapsed()
            );
        }
        ["search", pack, address] => {
            let outcome = search_all(&open_packs(pack)?, address);
            match outcome {
                Outcome::Found(found) => found.iter().for_each(|c| {
                    println!("{:?}  {:.6}, {:.6}  {}", c.precision, c.lat, c.lon, c.label)
                }),
                Outcome::Declined(why) => println!("declined: {why:?}"),
            }
        }
        ["datasets", pack] => {
            let pack = Pack::open(pack)?;
            let mut out = std::io::stdout().lock();
            let mut list = writeln!(out, "{}: {} records", pack.source(), pack.len());
            for (name, n) in pack.datasets() {
                list = list.and_then(|()| writeln!(out, "{n}\t{name}"));
            }
            // A reader like `head` closing the pipe early isn't a failure.
            if let Err(e) = list {
                if e.kind() != std::io::ErrorKind::BrokenPipe {
                    return Err(e.into());
                }
            }
        }
        ["eval", pack, tsv] => eval(&open_packs(pack)?, tsv)?,
        _ => return Err(USAGE.into()),
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
