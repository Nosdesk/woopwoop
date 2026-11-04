//! Turns Mapbox Vector Tiles into SVG, so a browser can draw a map from plain
//! images. No GPU, native renderer or font files: the browser does the drawing and
//! the text.
//!
//! A map is two layers of images. [`base`] draws the land, water and roads for one
//! tile. [`Labels`] draws place and road names in a transparent image twice the
//! tile's size, so a name near an edge overflows rather than being cut off.

mod labels;
mod mvt;
mod svg;
mod theme;
#[cfg(target_arch = "wasm32")]
mod wasm;

pub use labels::Labels;
pub use svg::base;
pub use theme::Theme;
