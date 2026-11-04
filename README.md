# WoopWoop

A small, fast address geocoder. Finds anywhere, even out in woop woop.

![WoopWoop resolving Australian addresses in a terminal](docs/demo.png)

WoopWoop turns an address into coordinates. It runs inside your own process from one memory-mapped file, so addresses stay on your server and it works with no network.

Australia's 15.7 million addresses fit in a 182 MB pack, and America's 125 million in 1.9 GB. A search typically takes well under 50 microseconds.

("Woop woop" is Australian slang for the middle of nowhere.)

## Use it

```rust
use woopwoop::{Outcome, Pack};

let pack = Pack::open("au.pack")?;

match pack.search("Level 3, 100 St Georges Tce, Perth") {
    Outcome::Found(candidates) => println!("{:?}", candidates[0]),
    Outcome::Declined(why) => println!("no result: {why:?}"),
}
```

Each candidate has a position, a label and a `precision` of `House`, `Street` or `Locality`. Show anything coarser than `House` as approximate.

- Units, levels, shops, number ranges and business names are all understood.
- Typos in street names are corrected.
- Ambiguous input, like a street that exists in several suburbs, returns each match.
- A match in a neighbouring suburb has `place_confirmed` set to false.
- PO boxes and unreadable text return `Outcome::Declined`.

WoopWoop reads addresses from Australia, New Zealand, Canada, the United States and the United Kingdom. It finds addresses, so it won't find a landmark by name.

`search_all` searches several packs and keeps the most precise answer. That keeps sources with different licences in separate files, and improves a region by adding a pack.

## Build a pack

Packs aren't included.

For Australia, download [G-NAF](https://data.gov.au/data/dataset/geocoded-national-address-file-g-naf) and point the builder at its `Standard` folder:

```sh
cargo run --release -- build-gnaf "G-NAF/G-NAF AUGUST 2026/Standard" au.pack "G-NAF August 2026"
cargo run --release -- search au.pack "1 Macquarie St, Sydney"
```

For America, Canada and New Zealand, export addresses, towns and roads from [Overture Maps](https://docs.overturemaps.org/guides/addresses/) using the scripts in `eval/`, which need `duckdb`:

```sh
python eval/overture_export.py 2026-10-21.0 US us.tsv
python eval/overture_places.py 2026-10-21.0 US us-places.tsv
python eval/overture_roads.py 2026-10-21.0 us-roads.tsv -125,24.4,-66.5,49.5 -170,51,-130,71.5 -161,18.8,-154,22.4
cargo run --release -- build-overture us.tsv us-places.tsv us-roads.tsv us.pack "Overture 2026-10-21.0" US
```

The UK isn't in Overture. Build it from OpenStreetMap addresses, listed by `eval/osm_addresses`, and [Code-Point Open](https://www.ordnancesurvey.co.uk/products/code-point-open), converted by `eval/codepoint.py`. Add its postcode rows to the places file. Each postcode covers about 15 homes, so an address OpenStreetMap lacks still lands close by.

Building takes under a minute for most countries, and about four minutes with 6 GB of memory for America.

## Maps

[`woopwoop-tiles`](tiles) is a second crate in this repository. It turns vector map tiles into SVG, so a browser can draw a map from plain images. It reads PMTiles archives, and runs natively or from a small WebAssembly module.

## Accuracy

| Held-out addresses, written 8 to 12 ways | Within 50 m | Over 1 km out | Landmarks |
| --- | --- | --- | --- |
| Australia | 97.1% | 0.1% | |
| New Zealand | 98.7% | 0.2% | 7 of 8 |
| Canada | 98.8% | 0.6% | 13 of 13 |
| United Kingdom | 98.2% | 0.5% | 12 of 12 |
| United States | 92.9% | 3.0% | 25 of 28 |

These scores come from addresses in the same data as the pack, with generated formats. They show how well WoopWoop reads addresses. They can't show how accurate the data is or what's missing, so they don't compare countries fairly.

Coverage varies. The UK pack holds 5.1 million addresses out of roughly 30 million. American data falls about 29 million addresses short, unevenly: New Hampshire and Hawaii have none in Overture, and Nevada and Georgia are far from complete. An OpenStreetMap layer helps, and the rest resolve to the street or town.

`eval/landmarks.py` checks well-known public places against positions from outside the data. The lists are small and hand-written.

Run your own with `woopwoop eval`, using the `eval/formats` scripts to generate the formats. Keep a sample you've never looked at, because fixing misses from a sample makes its score meaningless.

## Licence

The code is licensed under [Apache 2.0](LICENSE).

Packs carry the licences of their data. G-NAF uses the [Open G-NAF EULA](https://data.gov.au/data/dataset/geocoded-national-address-file-g-naf), based on CC BY 4.0, which restricts generating addresses for sending mail. Overture's addresses come from many sources with [different licences](https://docs.overturemaps.org/attribution/). OpenStreetMap data is share-alike under the ODbL, so keep it in its own pack. Read them before distributing a pack.
