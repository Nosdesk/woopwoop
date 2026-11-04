//! Finds tiles in a PMTiles archive without loading its directories into objects.
//!
//! One directory in a planet file holds tens of thousands of entries. Parsing them
//! all to read one costs far more than the read, and keeping them parsed overflows a
//! small runtime. So this keeps directories as bytes and scans them for one entry.
//!
//! The caller does the reading, which keeps this free of any I/O: `locate` says what
//! to fetch next.

use std::collections::VecDeque;

/// Directories kept, about 320 KB each in a planet file.
const DIRECTORIES: usize = 24;
/// Nothing in a tile archive inflates beyond this; anything larger is damaged.
const MAX_INFLATED: usize = 32 * 1024 * 1024;

/// What to do next to find a tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// No such tile in the archive.
    Missing,
    /// Read these bytes: they're the tile, compressed.
    Tile { offset: u64, length: u64 },
    /// Read these bytes and pass them to `add_directory`, then locate again.
    Directory { offset: u64, length: u64 },
}

pub struct Archive {
    root: Vec<u8>,
    directories_at: u64,
    tiles_at: u64,
    cached: VecDeque<(u64, Vec<u8>)>,
}

fn u64_at(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

/// Inflates gzip data. `None` if it's damaged.
pub fn inflate(data: &[u8]) -> Option<Vec<u8>> {
    let flags = *data.get(3)?;
    if data.get(..3)? != [0x1f, 0x8b, 8] {
        return None;
    }
    let mut at = 10;
    if flags & 4 != 0 {
        at += 2 + usize::from(u16::from_le_bytes(data.get(at..at + 2)?.try_into().ok()?));
    }
    for flag in [8, 16] {
        if flags & flag != 0 {
            at += data.get(at..)?.iter().position(|b| *b == 0)? + 1;
        }
    }
    if flags & 2 != 0 {
        at += 2;
    }
    miniz_oxide::inflate::decompress_to_vec_with_limit(data.get(at..)?, MAX_INFLATED).ok()
}

/// A tile's position along the archive's Hilbert curve.
pub fn tile_id(z: u8, x: u32, y: u32) -> u64 {
    let start = ((1u128 << (2 * u32::from(z))) - 1) / 3;
    let (mut tx, mut ty) = (i64::from(x), i64::from(y));
    let mut d: u64 = 0;
    let mut s: i64 = (1i64 << z) / 2;
    while s > 0 {
        let rx = i64::from(tx & s > 0);
        let ry = i64::from(ty & s > 0);
        d += (s * s) as u64 * ((3 * rx) ^ ry) as u64;
        if ry == 0 {
            if rx == 1 {
                tx = s - 1 - tx;
                ty = s - 1 - ty;
            }
            std::mem::swap(&mut tx, &mut ty);
        }
        s /= 2;
    }
    start as u64 + d
}

struct Varints<'a> {
    data: &'a [u8],
    at: usize,
}

impl Varints<'_> {
    fn next(&mut self) -> Option<u64> {
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

    fn skip(&mut self, count: u64) -> Option<()> {
        for _ in 0..count {
            self.next()?;
        }
        Some(())
    }
}

struct Entry {
    run: u64,
    offset: u64,
    length: u64,
    covers: bool,
}

/// Finds the entry for `id`. A directory stores its ids, run lengths, sizes and offsets
/// as four columns, so this walks the columns instead of building rows.
fn find(directory: &[u8], id: u64) -> Option<Entry> {
    let mut v = Varints {
        data: directory,
        at: 0,
    };
    let count = v.next()?;
    let (mut current, mut index, mut matched) = (0u64, None, 0u64);
    for i in 0..count {
        current = current.checked_add(v.next()?)?;
        if current <= id {
            index = Some(i);
            matched = current;
        }
    }
    let index = index?;
    v.skip(index)?;
    let run = v.next()?;
    v.skip(count - index - 1)?;
    let lengths_at = v.at;
    v.skip(count)?;
    // An offset of zero means "straight after the previous entry", so walk the
    // offsets and lengths together.
    let mut lengths = Varints {
        data: directory,
        at: lengths_at,
    };
    let (mut offset, mut length) = (0u64, 0u64);
    for i in 0..=index {
        let stored = v.next()?;
        let previous_end = offset.checked_add(length)?;
        length = lengths.next()?;
        offset = if stored == 0 && i > 0 {
            previous_end
        } else {
            stored.checked_sub(1)?
        };
    }
    Some(Entry {
        run,
        offset,
        length,
        covers: run == 0 || id - matched < run,
    })
}

impl Archive {
    /// Opens an archive from its first 16 KB, which hold the header and root directory.
    pub fn open(head: &[u8]) -> Option<Self> {
        if head.get(..7)? != b"PMTiles" || *head.get(7)? != 3 {
            return None;
        }
        let root_at = usize::try_from(u64_at(head, 8)?).ok()?;
        let root_len = usize::try_from(u64_at(head, 16)?).ok()?;
        let root = inflate(head.get(root_at..root_at.checked_add(root_len)?)?)?;
        Some(Self {
            root,
            directories_at: u64_at(head, 40)?,
            tiles_at: u64_at(head, 56)?,
            cached: VecDeque::new(),
        })
    }

    /// Says what to do next to find a tile.
    pub fn locate(&self, z: u8, x: u32, y: u32) -> Step {
        let id = tile_id(z, x, y);
        let mut directory: &[u8] = &self.root;
        for _ in 0..4 {
            let Some(e) = find(directory, id) else {
                return Step::Missing;
            };
            if !e.covers {
                return Step::Missing;
            }
            if e.run > 0 {
                return Step::Tile {
                    offset: self.tiles_at + e.offset,
                    length: e.length,
                };
            }
            let offset = self.directories_at + e.offset;
            match self.cached.iter().find(|(o, _)| *o == offset) {
                Some((_, bytes)) => directory = bytes,
                None => {
                    return Step::Directory {
                        offset,
                        length: e.length,
                    }
                }
            }
        }
        Step::Missing
    }

    /// Stores a directory that `locate` asked for, given its compressed bytes.
    pub fn add_directory(&mut self, offset: u64, compressed: &[u8]) {
        if self.cached.iter().any(|(o, _)| *o == offset) {
            return;
        }
        // A damaged directory is stored empty, so the search ends instead of repeating.
        let bytes = inflate(compressed).unwrap_or_else(|| vec![0]);
        if self.cached.len() >= DIRECTORIES {
            self.cached.pop_front();
        }
        self.cached.push_back((offset, bytes));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_ids_follow_the_spec() {
        assert_eq!(tile_id(0, 0, 0), 0);
        assert_eq!(tile_id(1, 0, 0), 1);
        assert_eq!(tile_id(1, 0, 1), 2);
        assert_eq!(tile_id(1, 1, 1), 3);
        assert_eq!(tile_id(1, 1, 0), 4);
        assert_eq!(tile_id(2, 0, 0), 5);
    }

    fn directory(entries: &[(u64, u64, u64, u64)]) -> Vec<u8> {
        fn put(out: &mut Vec<u8>, mut v: u64) {
            while v >= 0x80 {
                out.push((v as u8) | 0x80);
                v >>= 7;
            }
            out.push(v as u8);
        }
        let mut b = Vec::new();
        put(&mut b, entries.len() as u64);
        let mut last = 0;
        for e in entries {
            put(&mut b, e.0 - last);
            last = e.0;
        }
        entries.iter().for_each(|e| put(&mut b, e.1));
        entries.iter().for_each(|e| put(&mut b, e.3));
        for (i, e) in entries.iter().enumerate() {
            let follows = i > 0 && e.2 == entries[i - 1].2 + entries[i - 1].3;
            put(&mut b, if follows { 0 } else { e.2 + 1 });
        }
        b
    }

    #[test]
    fn finds_entries_and_runs() {
        // id, run, offset, length
        let d = directory(&[(1, 1, 0, 10), (2, 3, 10, 20), (9, 1, 500, 5)]);
        let e = find(&d, 3).expect("in a run");
        assert!(e.covers);
        assert_eq!((e.offset, e.length), (10, 20));
        assert!(!find(&d, 7).expect("past the run").covers);
        assert_eq!(find(&d, 9).expect("last").offset, 500);
        assert!(find(&d, 0).is_none());
    }

    #[test]
    fn damaged_data_is_refused_quietly() {
        assert!(Archive::open(b"not a pmtiles file at all").is_none());
        assert!(inflate(&[0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 3, 255, 255]).is_none());
        assert!(find(&[200], 1).is_none());
    }
}
