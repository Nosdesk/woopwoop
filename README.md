# WoopWoop

A small, fast address geocoder. Finds anywhere, even out in woop woop.

![WoopWoop resolving Australian addresses in a terminal](docs/demo.png)

WoopWoop turns an address into coordinates. It runs inside your own process from one memory-mapped file, so addresses stay on your server and it works with no network.

All 15.7 million Australian addresses fit in a 177 MB pack. A search typically takes under 10 microseconds and uses about 70 MB of memory.

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

WoopWoop reads Australian addresses today. It finds addresses, so it won't find a landmark by name.

## Build a pack

Packs aren't included. Download [G-NAF](https://data.gov.au/data/dataset/geocoded-national-address-file-g-naf) and point the builder at its `Standard` folder:

```sh
cargo run --release -- build-gnaf "G-NAF/G-NAF AUGUST 2026/Standard" au.pack "G-NAF August 2026"
cargo run --release -- search au.pack "1 Macquarie St, Sydney"
```

A build takes about a minute and 1 GB of memory.

## Accuracy

On 3,000 fresh addresses written 12 ways, 97.0% landed within 50 m of their G-NAF position and 0.2% landed over 1 km away. Results resolve to the building, so units in large complexes account for most of the gap.

The test addresses come from G-NAF too, and their messy formats are generated. This measures how well WoopWoop reads addresses. It says nothing about how accurate G-NAF is.

Run your own with `woopwoop eval`, using `eval/formats.py` to generate the formats.

## Licence

The code is licensed under [Apache 2.0](LICENSE).

Packs carry the licence of their data. G-NAF uses the [Open G-NAF EULA](https://data.gov.au/data/dataset/geocoded-national-address-file-g-naf), based on CC BY 4.0, which restricts generating addresses for sending mail. Read it before distributing a pack.
