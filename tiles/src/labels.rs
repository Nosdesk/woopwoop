//! The label layer, drawn in a transparent image twice the tile's size.
//!
//! Names must not repeat or overlap across tile edges, yet each tile is drawn alone
//! and in any order. So every tile works out the same answer independently. Each
//! candidate label comes only from its own tile's data, and a candidate survives if
//! no higher-priority candidate in the surrounding tiles collides with it or repeats
//! its name nearby. Two tiles looking at the same pair always agree.

use crate::mvt::{read_layer, Point, Value, LINE, POINT};
use crate::svg::{half, path, push_int};
use crate::theme::Theme;
use std::collections::HashMap;

const SIZE: f64 = 2048.0;
/// The same name closer than this is a repeat.
const NEAR: f64 = SIZE * 0.9;
/// Tiles of candidates kept between requests.
const MEMORY: usize = 400;

#[derive(Clone)]
struct Candidate {
    name: String,
    /// Position inside its tile, for a place label.
    local: Point,
    x: f64,
    y: f64,
    /// The label as a capsule: a line from `a` to `b` with radius `r`.
    a: (f64, f64),
    b: (f64, f64),
    r: f64,
    priority: f64,
    /// The road to run the text along. Empty for a place.
    road: Vec<Point>,
}

fn round_half(v: f64) -> f64 {
    (v / 2.0 + 0.5).floor()
}

fn hash(name: &str, x: i64, y: i64) -> f64 {
    let mut h: u32 = 2_166_136_261;
    let tail = format!("{x}/{y}");
    for unit in name.encode_utf16().chain(tail.encode_utf16()) {
        h ^= u32::from(unit);
        h = h.wrapping_mul(16_777_619);
    }
    f64::from(h) / 4_294_967_296.0
}

fn rank(kind: &str) -> f64 {
    match kind {
        "highway" => 0.0,
        "major_road" => 1.0,
        "medium_road" => 2.0,
        _ => 3.0,
    }
}

fn text_width(name: &str, size: f64) -> f64 {
    let mut w = 0.0;
    for c in name.chars() {
        w += if u32::from(c) > 0x2e7f { 1.0 } else { 0.56 };
    }
    w * size
}

fn distance(a: Point, b: Point) -> f64 {
    f64::from(b.0 - a.0).hypot(f64::from(b.1 - a.1))
}

fn places(tile: &[u8], z: u8, tx: i32, ty: i32, out: &mut Vec<Candidate>) {
    let Some(l) = read_layer(tile, "places") else {
        return;
    };
    for f in l.features.iter().filter(|f| f.kind == POINT) {
        let Some(name) = l.text(f, "name").filter(|n| !n.is_empty()) else {
            continue;
        };
        let min_zoom = match l.property(f, "min_zoom") {
            Some(Value::Number(n)) => n,
            _ => 9.0,
        };
        if min_zoom > f64::from(z) + 1.5 {
            continue;
        }
        let Some(p) = f.first_point() else { continue };
        let (x, y) = (half(p.0), half(p.1));
        if x < 0 || y < 0 || x >= SIZE as i32 || y >= SIZE as i32 {
            continue;
        }
        let (gx, gy) = (
            f64::from(tx) * SIZE + f64::from(x),
            f64::from(ty) * SIZE + f64::from(y),
        );
        let w = text_width(name, 84.0) / 2.0;
        out.push(Candidate {
            name: name.to_string(),
            local: (x, y),
            x: gx,
            y: gy,
            a: (gx - w, gy - 24.0),
            b: (gx + w, gy - 24.0),
            r: 56.0,
            priority: min_zoom + hash(name, i64::from(x), i64::from(y)) * 0.01,
            road: Vec::new(),
        });
    }
}

fn roads(tile: &[u8], tx: i32, ty: i32, out: &mut Vec<Candidate>) {
    let Some(l) = read_layer(tile, "roads") else {
        return;
    };
    let mut best: Vec<(f64, Candidate)> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for f in l.features.iter().filter(|f| f.kind == LINE) {
        let Some(name) = l.text(f, "name").filter(|n| !n.is_empty()) else {
            continue;
        };
        let kind = l.text(f, "kind").unwrap_or("");
        let need = text_width(name, 56.0) * 2.0 + 40.0;
        f.rings(|r| {
            if r.len() < 2 {
                return;
            }
            let segments: Vec<f64> = r.windows(2).map(|w| distance(w[0], w[1])).collect();
            let length: f64 = segments.iter().sum();
            if length < need {
                return;
            }
            let (first, last) = (r[0], r[r.len() - 1]);
            // Loops and hairpins garble text on a path.
            if distance(first, last) / length < 0.82 {
                return;
            }
            let at = index.get(name).copied();
            if at.is_some_and(|i| best[i].0 >= length) {
                return;
            }
            let (mut walked, mut mx, mut my) = (0.0, f64::from(first.0), f64::from(first.1));
            for (k, s) in segments.iter().enumerate() {
                if walked + s >= length / 2.0 {
                    let t = (length / 2.0 - walked) / s;
                    mx = f64::from(r[k].0) + f64::from(r[k + 1].0 - r[k].0) * t;
                    my = f64::from(r[k].1) + f64::from(r[k + 1].1 - r[k].1) * t;
                    break;
                }
                walked += s;
            }
            let (x, y) = (round_half(mx), round_half(my));
            if x < 0.0 || y < 0.0 || x >= SIZE || y >= SIZE {
                return;
            }
            let angle = f64::from(last.1 - first.1).atan2(f64::from(last.0 - first.0));
            let w = text_width(name, 56.0) / 2.0 + 10.0;
            let (gx, gy) = (f64::from(tx) * SIZE + x, f64::from(ty) * SIZE + y);
            let mut points = r.to_vec();
            if last.0 < first.0 {
                points.reverse();
            }
            let c = Candidate {
                name: name.to_string(),
                local: (0, 0),
                x: gx,
                y: gy,
                a: (gx - angle.cos() * w, gy - angle.sin() * w),
                b: (gx + angle.cos() * w, gy + angle.sin() * w),
                r: 40.0,
                priority: 20.0 + rank(kind) * 2.0 - (length / 4096.0).min(1.0)
                    + hash(name, x as i64, y as i64) * 0.01,
                road: points,
            };
            match at {
                Some(i) => best[i] = (length, c),
                None => {
                    index.insert(name.to_string(), best.len());
                    best.push((length, c));
                }
            }
        });
    }
    out.extend(best.into_iter().map(|(_, c)| c));
}

fn candidates(tile: &[u8], z: u8, tx: i32, ty: i32) -> Vec<Candidate> {
    let mut out = Vec::new();
    places(tile, z, tx, ty, &mut out);
    if z >= 14 {
        roads(tile, tx, ty, &mut out);
    }
    out
}

fn to_segment(p: (f64, f64), c: &Candidate) -> f64 {
    let (dx, dy) = (c.b.0 - c.a.0, c.b.1 - c.a.1);
    let mut l = dx * dx + dy * dy;
    if l == 0.0 {
        l = 1.0;
    }
    let t = (((p.0 - c.a.0) * dx + (p.1 - c.a.1) * dy) / l).clamp(0.0, 1.0);
    (p.0 - c.a.0 - t * dx).hypot(p.1 - c.a.1 - t * dy)
}

fn side(p: (f64, f64), q: (f64, f64), r: (f64, f64)) -> f64 {
    (q.0 - p.0) * (r.1 - p.1) - (q.1 - p.1) * (r.0 - p.0)
}

fn crosses(a: &Candidate, b: &Candidate) -> bool {
    side(a.a, a.b, b.a) * side(a.a, a.b, b.b) < 0.0
        && side(b.a, b.b, a.a) * side(b.a, b.b, a.b) < 0.0
}

fn clash(a: &Candidate, b: &Candidate) -> bool {
    if (a.x - b.x).hypot(a.y - b.y) > 1400.0 {
        return false;
    }
    crosses(a, b)
        || to_segment(a.a, b)
            .min(to_segment(a.b, b))
            .min(to_segment(b.a, a))
            .min(to_segment(b.b, a))
            < a.r + b.r
}

fn escape(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
}

/// Draws label images, remembering each tile's candidates, because every tile is
/// also a neighbour of eight others.
#[derive(Default)]
pub struct Labels {
    tiles: HashMap<(u8, i32, i32), Vec<Candidate>>,
    order: Vec<(u8, i32, i32)>,
}

impl Labels {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether this tile has been added, so a caller can skip fetching it.
    pub fn has(&self, z: u8, x: i32, y: i32) -> bool {
        self.tiles.contains_key(&(z, x, y))
    }

    /// Adds a tile's data. Pass an empty slice for a tile that doesn't exist.
    pub fn add(&mut self, tile: &[u8], z: u8, x: i32, y: i32) {
        if self.has(z, x, y) {
            return;
        }
        if self.order.len() >= MEMORY {
            let oldest = self.order.remove(0);
            self.tiles.remove(&oldest);
        }
        self.tiles.insert((z, x, y), candidates(tile, z, x, y));
        self.order.push((z, x, y));
    }

    /// Draws the labels for one tile, using it and its eight neighbours. Add those
    /// tiles first: anything missing is treated as empty.
    pub fn draw(&self, z: u8, x: i32, y: i32, t: &Theme) -> String {
        let mine: &[Candidate] = self.tiles.get(&(z, x, y)).map_or(&[], Vec::as_slice);
        let mut around: Vec<&Candidate> = Vec::new();
        for j in -1..=1 {
            for i in -1..=1 {
                if let Some(c) = self.tiles.get(&(z, x + i, y + j)) {
                    around.extend(c.iter());
                }
            }
        }
        let keep = mine.iter().filter(|c| {
            !around.iter().any(|o| {
                !std::ptr::eq(*o, *c)
                    && o.priority < c.priority
                    && (clash(c, o) || (o.name == c.name && (o.x - c.x).hypot(o.y - c.y) < NEAR))
            })
        });
        let (mut defs, mut body, mut id) = (String::new(), String::new(), 0);
        for c in keep {
            if c.road.is_empty() {
                body.push_str("<text class=\"p\" x=\"");
                push_int(&mut body, c.local.0);
                body.push_str("\" y=\"");
                push_int(&mut body, c.local.1);
                body.push_str("\" text-anchor=\"middle\">");
                escape(&mut body, &c.name);
                body.push_str("</text>");
            } else {
                defs.push_str("<path id=\"p");
                push_int(&mut defs, id);
                defs.push_str("\" d=\"");
                path(&mut defs, &c.road, false);
                defs.push_str("\"/>");
                body.push_str("<text><textPath href=\"#p");
                push_int(&mut body, id);
                body.push_str("\" startOffset=\"50%\" text-anchor=\"middle\">");
                escape(&mut body, &c.name);
                body.push_str("</textPath></text>");
                id += 1;
            }
        }
        let (text, halo, place) = (t.text, t.halo, t.place);
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"-1024 -1024 4096 4096\"><style>path{{fill:none}}text{{font:500 56px Inter,system-ui,-apple-system,\"Segoe UI\",Roboto,\"Noto Sans\",sans-serif;fill:{text};paint-order:stroke;stroke:{halo};stroke-width:12px}}.p{{font-size:84px;font-weight:600;fill:{place}}}</style><defs>{defs}</defs>{body}</svg>"
        )
    }
}
