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

/// A theme read from text, for colours chosen at run time. The text is twelve
/// colours of six lowercase hex digits each, run together with nothing between them,
/// in the order of [`Theme`]'s fields: land, water, park, building, road, casing,
/// major, motorway, rail, text, place, halo. Anything else is refused, as the
/// colours are written into SVG.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    colours: [String; COLOURS],
}

const COLOURS: usize = 12;

impl Palette {
    /// The length of a palette's text.
    pub const LEN: usize = COLOURS * 6;

    pub fn parse(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        if bytes.len() != Self::LEN
            || !bytes
                .iter()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
        {
            return None;
        }
        let colours = std::array::from_fn(|i| format!("#{}", &text[i * 6..i * 6 + 6]));
        Some(Self { colours })
    }

    pub fn theme(&self) -> Theme<'_> {
        let c = &self.colours;
        Theme {
            land: &c[0],
            water: &c[1],
            park: &c[2],
            building: &c[3],
            road: &c[4],
            casing: &c[5],
            major: &c[6],
            motorway: &c[7],
            rail: &c[8],
            text: &c[9],
            place: &c[10],
            halo: &c[11],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn light_text() -> String {
        let t = Theme::LIGHT;
        [
            t.land, t.water, t.park, t.building, t.road, t.casing, t.major, t.motorway, t.rail,
            t.text, t.place, t.halo,
        ]
        .iter()
        .map(|c| c.trim_start_matches('#'))
        .collect()
    }

    #[test]
    fn reads_twelve_colours_in_field_order() {
        let p = Palette::parse(&light_text()).unwrap();
        let t = p.theme();
        assert_eq!(t.land, Theme::LIGHT.land);
        assert_eq!(t.motorway, Theme::LIGHT.motorway);
        assert_eq!(t.halo, Theme::LIGHT.halo);
    }

    #[test]
    fn refuses_anything_but_lowercase_hex() {
        let good = light_text();
        assert!(Palette::parse(&good).is_some());
        assert!(Palette::parse("").is_none());
        assert!(Palette::parse(&good[..good.len() - 1]).is_none());
        assert!(Palette::parse(&format!("{good}0")).is_none());
        assert!(Palette::parse(&good.to_uppercase()).is_none());
        for bad in ["g", "#", ";", "\"", "<", " ", "\u{e9}"] {
            let mut t = good.clone();
            t.replace_range(0..bad.len(), bad);
            assert!(Palette::parse(&t).is_none(), "{bad}");
        }
        // Multibyte text of the right byte length is refused, not sliced.
        let wide = "\u{e9}".repeat(Palette::LEN / 2);
        assert_eq!(wide.len(), Palette::LEN);
        assert!(Palette::parse(&wide).is_none());
    }
}
