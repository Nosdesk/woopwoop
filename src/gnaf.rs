//! Reads G-NAF, Australia's address file, from its pipe-separated release.
//!
//! Pass the release's `Standard` folder. Only principal addresses are used:
//! aliases would add near-duplicates of the same spot.

use crate::build::Record;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Error, ErrorKind, Result};
use std::path::Path;

const STATES: &[&str] = &["ACT", "NSW", "NT", "OT", "QLD", "SA", "TAS", "VIC", "WA"];

fn rows(path: &Path, mut each: impl FnMut(&dyn Fn(&str) -> usize, &[&str])) -> Result<()> {
    let mut lines = BufReader::new(File::open(path)?).lines();
    let header = lines
        .next()
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "empty file"))??;
    let columns: HashMap<String, usize> = header
        .split('|')
        .enumerate()
        .map(|(i, c)| (c.to_string(), i))
        .collect();
    let column = |name: &str| columns.get(name).copied().unwrap_or(usize::MAX);
    for line in lines {
        let line = line?;
        let fields: Vec<&str> = line.split('|').collect();
        each(&column, &fields);
    }
    Ok(())
}

fn field<'a>(fields: &[&'a str], at: usize) -> &'a str {
    fields.get(at).copied().unwrap_or("")
}

/// Calls `each` for every principal address, and returns how many there were.
pub fn read(standard: &Path, mut each: impl FnMut(&Record)) -> Result<usize> {
    let mut count = 0;
    for state in STATES {
        let table = |name: &str| standard.join(format!("{state}_{name}_psv.psv"));
        let mut localities: HashMap<String, String> = HashMap::new();
        rows(&table("LOCALITY"), |c, f| {
            localities.insert(
                field(f, c("LOCALITY_PID")).into(),
                field(f, c("LOCALITY_NAME")).into(),
            );
        })?;
        let mut streets: HashMap<String, String> = HashMap::new();
        rows(&table("STREET_LOCALITY"), |c, f| {
            let parts = [
                field(f, c("STREET_NAME")),
                field(f, c("STREET_TYPE_CODE")),
                field(f, c("STREET_SUFFIX_CODE")),
            ];
            let name = parts
                .iter()
                .filter(|p| !p.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join(" ");
            streets.insert(field(f, c("STREET_LOCALITY_PID")).into(), name);
        })?;
        let mut points: HashMap<String, (f64, f64)> = HashMap::new();
        rows(&table("ADDRESS_DEFAULT_GEOCODE"), |c, f| {
            if let (Ok(lat), Ok(lon)) = (
                field(f, c("LATITUDE")).parse(),
                field(f, c("LONGITUDE")).parse(),
            ) {
                points.insert(field(f, c("ADDRESS_DETAIL_PID")).into(), (lat, lon));
            }
        })?;
        rows(&table("ADDRESS_DETAIL"), |c, f| {
            let number = field(f, c("NUMBER_FIRST"));
            if !field(f, c("DATE_RETIRED")).is_empty()
                || field(f, c("ALIAS_PRINCIPAL")) != "P"
                || number.is_empty()
            {
                return;
            }
            let (Some(point), Some(street), Some(locality)) = (
                points.get(field(f, c("ADDRESS_DETAIL_PID"))),
                streets.get(field(f, c("STREET_LOCALITY_PID"))),
                localities.get(field(f, c("LOCALITY_PID"))),
            ) else {
                return;
            };
            each(&Record {
                street: street.clone(),
                number: format!("{number}{}", field(f, c("NUMBER_FIRST_SUFFIX"))),
                locality: locality.clone(),
                postcode: field(f, c("POSTCODE")).to_string(),
                region: (*state).to_string(),
                lat: point.0,
                lon: point.1,
            });
            count += 1;
        })?;
    }
    Ok(count)
}
