//! Renders a folder of `{site}_{dx}_{dy}.mvt` tiles, for comparing against another
//! implementation. Usage: render <folder> <out folder> <light|dark>

use std::{env, fs, path::Path};
use woopwoop_tiles::{base, Labels, Theme};

fn main() {
    let a: Vec<String> = env::args().collect();
    let (dir, out) = (Path::new(&a[1]), Path::new(&a[2]));
    let theme = if a[3] == "dark" {
        Theme::DARK
    } else {
        Theme::LIGHT
    };
    fs::create_dir_all(out).unwrap();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let site = path.file_stem().unwrap().to_str().unwrap().to_string();
        let meta = fs::read_to_string(&path).unwrap();
        let z: u8 = meta
            .split("\"z\":")
            .nth(1)
            .unwrap()
            .split(',')
            .next()
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let load = |x: i32, y: i32| fs::read(dir.join(format!("{site}_{x}_{y}.mvt"))).ok();
        let mut labels = Labels::new();
        for y in -2..=2 {
            for x in -3..=3 {
                if let Some(t) = load(x, y) {
                    labels.add(&t, z, x, y);
                }
            }
        }
        for y in -1..=1 {
            for x in -2..=2 {
                let Some(t) = load(x, y) else { continue };
                fs::write(
                    out.join(format!("{site}_{x}_{y}.base.svg")),
                    base(&t, z, &theme),
                )
                .unwrap();
                fs::write(
                    out.join(format!("{site}_{x}_{y}.lbl.svg")),
                    labels.draw(z, x, y, &theme),
                )
                .unwrap();
            }
        }
    }
}
