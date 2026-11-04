//! A small Mapbox Vector Tile reader. It borrows from the tile and decodes geometry
//! only when asked, because most features in a tile are never drawn.

pub(crate) const LINE: u8 = 2;
pub(crate) const POINT: u8 = 1;
pub(crate) const POLYGON: u8 = 3;

/// One point in tile coordinates.
pub(crate) type Point = (i32, i32);

struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, at: 0 }
    }

    fn more(&self) -> bool {
        self.at < self.data.len()
    }

    fn varint(&mut self) -> Option<u64> {
        let (mut value, mut shift) = (0u64, 0u32);
        loop {
            let byte = *self.data.get(self.at)?;
            self.at += 1;
            value |= u64::from(byte & 0x7f) << shift;
            if byte < 0x80 {
                return Some(value);
            }
            shift += 7;
            if shift > 63 {
                return None;
            }
        }
    }

    fn bytes(&mut self) -> Option<&'a [u8]> {
        let len = self.varint()? as usize;
        let end = self.at.checked_add(len)?;
        let out = self.data.get(self.at..end)?;
        self.at = end;
        Some(out)
    }

    fn skip(&mut self, wire: u64) -> Option<()> {
        match wire {
            0 => {
                self.varint()?;
            }
            1 => self.at += 8,
            2 => {
                self.bytes()?;
            }
            5 => self.at += 4,
            _ => return None,
        }
        Some(())
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Value<'a> {
    Text(&'a str),
    Number(f64),
    Other,
}

pub(crate) struct Feature<'a> {
    pub(crate) kind: u8,
    tags: &'a [u8],
    geometry: &'a [u8],
}

pub(crate) struct Layer<'a> {
    keys: Vec<&'a str>,
    values: Vec<Value<'a>>,
    pub(crate) features: Vec<Feature<'a>>,
}

fn value(data: &[u8]) -> Option<Value<'_>> {
    let mut r = Reader::new(data);
    let mut out = Value::Other;
    while r.more() {
        let tag = r.varint()?;
        match (tag >> 3, tag & 7) {
            (1, 2) => out = Value::Text(std::str::from_utf8(r.bytes()?).ok()?),
            (2, 5) => {
                let b = r.data.get(r.at..r.at + 4)?;
                out = Value::Number(f64::from(f32::from_le_bytes(b.try_into().ok()?)));
                r.at += 4;
            }
            (3, 1) => {
                let b = r.data.get(r.at..r.at + 8)?;
                out = Value::Number(f64::from_le_bytes(b.try_into().ok()?));
                r.at += 8;
            }
            (4 | 5, 0) => out = Value::Number(r.varint()? as i64 as f64),
            (6, 0) => {
                let v = r.varint()?;
                out = Value::Number(((v >> 1) as i64 ^ -((v & 1) as i64)) as f64);
            }
            (_, wire) => r.skip(wire)?,
        }
    }
    Some(out)
}

fn feature(data: &[u8]) -> Option<Feature<'_>> {
    let mut r = Reader::new(data);
    let mut f = Feature {
        kind: 0,
        tags: &[],
        geometry: &[],
    };
    while r.more() {
        let tag = r.varint()?;
        match (tag >> 3, tag & 7) {
            (2, 2) => f.tags = r.bytes()?,
            (3, 0) => f.kind = r.varint()? as u8,
            (4, 2) => f.geometry = r.bytes()?,
            (_, wire) => r.skip(wire)?,
        }
    }
    Some(f)
}

fn layer(data: &[u8]) -> Option<(&str, Layer<'_>)> {
    let mut r = Reader::new(data);
    let mut name = "";
    let mut l = Layer {
        keys: Vec::new(),
        values: Vec::new(),
        features: Vec::new(),
    };
    while r.more() {
        let tag = r.varint()?;
        match (tag >> 3, tag & 7) {
            (1, 2) => name = std::str::from_utf8(r.bytes()?).ok()?,
            (2, 2) => l.features.push(feature(r.bytes()?)?),
            (3, 2) => l.keys.push(std::str::from_utf8(r.bytes()?).ok()?),
            (4, 2) => l.values.push(value(r.bytes()?)?),
            (_, wire) => r.skip(wire)?,
        }
    }
    Some((name, l))
}

/// Reads the named layer of a tile. A damaged tile reads as having no such layer.
pub(crate) fn read_layer<'a>(tile: &'a [u8], wanted: &str) -> Option<Layer<'a>> {
    let mut r = Reader::new(tile);
    while r.more() {
        let tag = r.varint()?;
        if tag == (3 << 3 | 2) {
            let bytes = r.bytes()?;
            if layer_name(bytes)? == wanted {
                return layer(bytes).map(|(_, l)| l);
            }
        } else {
            r.skip(tag & 7)?;
        }
    }
    None
}

fn layer_name(data: &[u8]) -> Option<&str> {
    let mut r = Reader::new(data);
    while r.more() {
        let tag = r.varint()?;
        if tag == (1 << 3 | 2) {
            return std::str::from_utf8(r.bytes()?).ok();
        }
        r.skip(tag & 7)?;
    }
    None
}

impl<'a> Layer<'a> {
    pub(crate) fn property(&self, f: &Feature<'a>, key: &str) -> Option<Value<'a>> {
        let mut r = Reader::new(f.tags);
        while r.more() {
            let k = r.varint()? as usize;
            let v = r.varint()? as usize;
            if *self.keys.get(k)? == key {
                return self.values.get(v).copied();
            }
        }
        None
    }

    pub(crate) fn text(&self, f: &Feature<'a>, key: &str) -> Option<&'a str> {
        match self.property(f, key) {
            Some(Value::Text(s)) => Some(s),
            _ => None,
        }
    }
}

impl Feature<'_> {
    /// Calls `each` for every ring or line, reusing one buffer.
    pub(crate) fn rings(&self, mut each: impl FnMut(&[Point])) {
        let mut r = Reader::new(self.geometry);
        let (mut x, mut y, mut left, mut command) = (0i32, 0i32, 0u64, 0u64);
        let mut line: Vec<Point> = Vec::new();
        let mut open = false;
        while r.more() {
            if left == 0 {
                let Some(c) = r.varint() else { break };
                command = c & 7;
                left = c >> 3;
                if left == 0 {
                    continue;
                }
            }
            left -= 1;
            match command {
                1 | 2 => {
                    let (Some(dx), Some(dy)) = (r.varint(), r.varint()) else {
                        break;
                    };
                    x = x.wrapping_add(((dx >> 1) as i32) ^ -((dx & 1) as i32));
                    y = y.wrapping_add(((dy >> 1) as i32) ^ -((dy & 1) as i32));
                    if command == 1 {
                        if open {
                            each(&line);
                        }
                        line.clear();
                        open = true;
                    }
                    if open {
                        line.push((x, y));
                    }
                }
                7 => {
                    if let Some(first) = line.first().copied() {
                        line.push(first);
                    }
                }
                _ => break,
            }
        }
        if open {
            each(&line);
        }
    }

    /// The first point of a point feature.
    pub(crate) fn first_point(&self) -> Option<Point> {
        let mut found = None;
        self.rings(|r| {
            if found.is_none() {
                found = r.first().copied();
            }
        });
        found
    }
}
