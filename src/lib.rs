//! WoopWoop is a small, fast address geocoder: "finds anywhere, even out in woop
//! woop".
//!
//! It answers one question, "where is this address?", from a single memory-mapped
//! pack file. All of Australia fits in a few hundred megabytes and resolves in
//! microseconds, so it can run inside your own process and addresses never have
//! to leave it.
//!
//! ```no_run
//! use woopwoop::{Outcome, Pack};
//!
//! let pack = Pack::open("au.pack")?;
//! match pack.search("3/12 High St, Windsor VIC 3181") {
//!     Outcome::Found(candidates) => println!("{:?}", candidates[0]),
//!     Outcome::Declined(why) => println!("no result: {why:?}"),
//! }
//! # Ok::<(), woopwoop::Error>(())
//! ```

mod automaton;
pub mod build;
pub mod gnaf;
pub mod normal;
pub mod pack;
pub mod parse;
mod street_types;

pub use build::{PackBuilder, Record};
pub use normal::Country;
pub use pack::{Candidate, Declined, Error, Outcome, Pack, Precision, Query};
pub use parse::{parse, Gazetteer, Parsed};
