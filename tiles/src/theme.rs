//! Map colours.

/// The colours a map is drawn in. Each is any CSS colour.
#[derive(Debug, Clone, Copy)]
pub struct Theme<'a> {
    pub land: &'a str,
    pub water: &'a str,
    pub park: &'a str,
    pub building: &'a str,
    pub road: &'a str,
    pub casing: &'a str,
    pub major: &'a str,
    pub motorway: &'a str,
    pub rail: &'a str,
    pub text: &'a str,
    pub place: &'a str,
    pub halo: &'a str,
}

impl Theme<'static> {
    pub const LIGHT: Self = Self {
        land: "#f4f1ea",
        water: "#a9cfe6",
        park: "#cfe3c0",
        building: "#e3dfd6",
        road: "#ffffff",
        casing: "#d3cdc0",
        major: "#fbd97f",
        motorway: "#f5b95c",
        rail: "#b5b0a6",
        text: "#3d3a35",
        place: "#3d3a35",
        halo: "#ffffff",
    };

    pub const DARK: Self = Self {
        land: "#0e0f11",
        water: "#14304a",
        park: "#11291c",
        building: "#191b1f",
        road: "#26292e",
        casing: "#08090a",
        major: "#4b5058",
        motorway: "#a34c1c",
        rail: "#2a2d33",
        text: "#b8bcc0",
        place: "#f7f8f8",
        halo: "#08090a",
    };
}
