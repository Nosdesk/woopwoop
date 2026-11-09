# woopwoop-tiles

Turns vector map tiles into SVG, so a browser can draw a map from plain images. It needs no GPU, map renderer or font files: the browser does the drawing and the text.

A map is two layers of images. `base` draws one tile's land, water and roads. `Labels` draws names in a transparent image twice the tile's size, so a name near an edge overflows instead of being cut off. Each tile decides its labels alone, and neighbouring tiles still agree, so names don't repeat or collide.

```rust
use woopwoop_tiles::{base, Labels, Palette, Theme};

let svg = base(&tile, 15, &Theme::DARK);

let mut labels = Labels::new();
labels.add(&tile, 15, x, y); // and the eight tiles around it
let svg = labels.draw(15, x, y, &Theme::DARK);
```

Colours chosen at run time come in as a `Palette`: twelve six-digit lowercase hex colours run together, in the order of `Theme`'s fields. Anything else is refused, since the colours land in the SVG.

```rust
let palette = Palette::parse(text).ok_or("not a palette")?;
let svg = base(&tile, 15, &palette.theme());
```

`pmtiles::Archive` finds tiles in a PMTiles archive. It keeps directories as bytes and scans them for one entry, so a planet file fits in a small runtime. It does no I/O itself: `locate` says what to read next.

The crate also builds to a WebAssembly module with no imports, about 55 KB gzipped, for Cloudflare Workers and similar runtimes:

```sh
cargo build -p woopwoop-tiles --target wasm32-unknown-unknown --profile wasm
```

Tiles are Protomaps basemap tiles at extent 4096. Maps drawn from OpenStreetMap data must credit OpenStreetMap contributors.
