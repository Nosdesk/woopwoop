//! The base layer: land, water and roads.

use crate::mvt::{read_layer, Point, LINE, POLYGON};
use crate::theme::Theme;

const GREEN: &[&str] = &[
    "cemetery",
    "forest",
    "garden",
    "golf_course",
    "grass",
    "meadow",
    "national_park",
    "nature_reserve",
    "park",
    "protected_area",
    "recreation_ground",
    "scrub",
    "wood",
];

/// Tile coordinates run to 4096. Drawing at half that keeps paths short and is
/// still finer than any screen.
pub(crate) fn half(n: i32) -> i32 {
    (n + 1).div_euclid(2)
}

pub(crate) fn push_int(out: &mut String, n: i32) {
    let mut buffer = [0u8; 12];
    let mut at = buffer.len();
    let mut v = n.unsigned_abs();
    loop {
        at -= 1;
        buffer[at] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    if n < 0 {
        out.push('-');
    }
    for &digit in &buffer[at..] {
        out.push(digit as char);
    }
}

pub(crate) fn path(out: &mut String, points: &[Point], close: bool) {
    let (mut px, mut py) = (0, 0);
    for (i, p) in points.iter().enumerate() {
        let (x, y) = (half(p.0), half(p.1));
        if i == 0 {
            out.push('M');
            push_int(out, x);
            out.push(' ');
            push_int(out, y);
        } else if x != px || y != py {
            out.push('l');
            push_int(out, x - px);
            out.push(' ');
            push_int(out, y - py);
        }
        px = x;
        py = y;
    }
    if close {
        out.push('z');
    }
}

fn polygons(
    tile: &[u8],
    name: &str,
    out: &mut String,
    keep: impl Fn(&crate::mvt::Layer, &crate::mvt::Feature) -> bool,
) {
    if let Some(l) = read_layer(tile, name) {
        for f in &l.features {
            if f.kind == POLYGON && keep(&l, f) {
                f.rings(|r| path(out, r, true));
            }
        }
    }
}

/// Draws one tile's land, water and roads as an SVG document. `z` is the tile's zoom.
pub fn base(tile: &[u8], z: u8, t: &Theme) -> String {
    let (mut water, mut park, mut building, mut boundary) =
        (String::new(), String::new(), String::new(), String::new());
    let (mut minor, mut major, mut motorway, mut rail, mut foot) = (
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
    );

    polygons(tile, "water", &mut water, |_, _| true);
    let green = |l: &crate::mvt::Layer, f: &crate::mvt::Feature| {
        l.text(f, "kind").is_some_and(|k| GREEN.contains(&k))
    };
    polygons(tile, "landuse", &mut park, green);
    polygons(tile, "landcover", &mut park, green);
    if z >= 15 {
        polygons(tile, "buildings", &mut building, |_, _| true);
    }
    if let Some(l) = read_layer(tile, "boundaries") {
        for f in l.features.iter().filter(|f| f.kind == LINE) {
            f.rings(|r| path(&mut boundary, r, false));
        }
    }
    if let Some(l) = read_layer(tile, "roads") {
        for f in l.features.iter().filter(|f| f.kind == LINE) {
            let kind = l.text(f, "kind").unwrap_or("");
            let target = match kind {
                "rail" => &mut rail,
                "path" | "other" if z < 15 => continue,
                "path" | "other" => &mut foot,
                "highway" => &mut motorway,
                "major_road" => &mut major,
                _ => &mut minor,
            };
            f.rings(|r| path(target, r, false));
        }
    }

    let width = match z {
        15.. => 26.0,
        14 => 18.0,
        13 => 14.0,
        12 => 11.0,
        10 | 11 => 8.0,
        _ => 6.0,
    };
    let wide: f64 = width * 1.7;
    let (text, halo) = (t.text, t.halo);
    let css = format!(
        "path{{fill:none;stroke-linejoin:round;stroke-linecap:round}}text{{font:500 56px system-ui,-apple-system,\"Segoe UI\",Roboto,sans-serif;fill:{text};paint-order:stroke;stroke:{halo};stroke-width:12px}}.p{{font-size:84px;font-weight:600}}"
    );
    let mut out = String::with_capacity(
        water.len()
            + park.len()
            + building.len()
            + boundary.len()
            + 2 * (minor.len() + major.len() + motorway.len())
            + rail.len()
            + foot.len()
            + 2048,
    );
    out.push_str("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 2048 2048\"><style>");
    out.push_str(&css);
    out.push_str("</style>");
    out.push_str(&format!(
        "<rect width=\"2048\" height=\"2048\" fill=\"{}\"/>",
        t.land
    ));
    let mut layer = |style: String, extra: &str, d: &str| {
        out.push_str("<path style=\"");
        out.push_str(&style);
        out.push('"');
        out.push_str(extra);
        out.push_str(" d=\"");
        out.push_str(d);
        out.push_str("\"/>");
    };
    layer(
        format!("fill:{};stroke:none", t.park),
        " fill-rule=\"evenodd\"",
        &park,
    );
    layer(
        format!("fill:{};stroke:none", t.water),
        " fill-rule=\"evenodd\"",
        &water,
    );
    layer(format!("fill:{};stroke:none", t.building), "", &building);
    layer(
        format!(
            "stroke:{};stroke-opacity:.35;stroke-width:6;stroke-dasharray:24 16",
            t.text
        ),
        "",
        &boundary,
    );
    layer(format!("stroke:{};stroke-width:8", t.rail), "", &rail);
    layer(
        format!("stroke:{};stroke-width:6;stroke-opacity:.85", t.road),
        "",
        &foot,
    );
    layer(
        format!("stroke:{};stroke-width:{}", t.casing, width + 8.0),
        "",
        &minor,
    );
    layer(
        format!("stroke:{};stroke-width:{}", t.casing, wide + 10.0),
        "",
        &major,
    );
    layer(
        format!("stroke:{};stroke-width:{}", t.road, width),
        "",
        &minor,
    );
    layer(
        format!("stroke:{};stroke-width:{}", t.major, wide),
        "",
        &major,
    );
    layer(
        format!("stroke:{};stroke-width:{}", t.casing, wide * 1.25 + 10.0),
        "",
        &motorway,
    );
    layer(
        format!("stroke:{};stroke-width:{}", t.motorway, wide * 1.25),
        "",
        &motorway,
    );
    out.push_str("</svg>");
    out
}
