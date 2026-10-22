//! The maps' looks: each theme's ground, wilds and light.

use super::*;

use arena_shared::map::MapId;
use super::palette::*;

/// What a map's world is made of and lit by: the scene's colors, the sun, the sky's fill and the
/// haze. Fighters, shots, pickups, torches and the UI look the same on every map.
pub struct Theme {
    pub grass: Color,
    pub meadow: Color,
    pub fern: Color,
    pub pine: Color,
    pub slate: Color,
    pub stone: Color,
    pub wall: Color,
    pub bark: Color,
    pub path: Color,
    pub bank: Color,
    pub riverbed: Color,
    pub pond: Color,
    pub bloom: Color,
    pub marsh: Color,
    pub abyss: Color,
    pub rope: Color,
    pub haze: Color,
    pub sky: Color,
    pub sky_brightness: f32,
    pub sun: Color,
    pub sun_illuminance: f32,
    /// What lies around the clearing, and what its trees are.
    pub wilds: Wilds,
}

/// The land around a map's clearing out to the haze, and what grows on it (the map's own trees
/// too).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Wilds {
    /// Pine forest rolling up into low hills.
    Forest,
    /// A flooded moor: still dark water with grassy tussocks breaking it, reed beds, dead snags
    /// and low willows.
    Fen,
    /// Bare highland: dry grass rising in terraces of banded rock, with rock pillars, scrub,
    /// boulders and a few round autumn trees.
    Highlands,
}

impl Theme {
    pub fn of(map: MapId) -> &'static Theme {
        match map.base() {
            MapId::Crossing => &GLADE,
            MapId::Keep => &FEN,
            MapId::Gorge => &HIGHLANDS,
            MapId::Wilds => &HOLLOW,
            _ => &BASTION,
        }
    }
}

/// River Crossing: a bright green forest glade, the world's palette (`palette`).
const GLADE: Theme = Theme {
    grass: MOSS,
    meadow: MEADOW,
    fern: FERN,
    pine: PINE,
    slate: SLATE,
    stone: STONE,
    wall: WALL,
    bark: BARK,
    path: PATH,
    bank: BANK,
    riverbed: RIVERBED,
    pond: POND,
    bloom: HEATHER,
    marsh: MARSH,
    abyss: ABYSS,
    rope: ROPE,
    haze: HAZE,
    sky: SKY,
    sky_brightness: 700.0,
    sun: SUN,
    sun_illuminance: 9_000.0,
    wilds: Wilds::Forest,
};

/// Sunken Keep: a bright fen on a cool, clear morning. Sea-green tussocks in glittering teal
/// water, pale mist far off, bog cotton for flowers.
const FEN: Theme = Theme {
    grass: Color::srgb_u8(0x6F, 0xAE, 0x78),
    meadow: Color::srgb_u8(0x86, 0xBE, 0x7E),
    fern: Color::srgb_u8(0x54, 0xA0, 0x66),
    pine: Color::srgb_u8(0x3C, 0x8A, 0x62),
    slate: Color::srgb_u8(0x92, 0xA0, 0xB0),
    stone: Color::srgb_u8(0xB2, 0xBC, 0xC6),
    wall: Color::srgb_u8(0x9A, 0xA6, 0xB2),
    bark: Color::srgb_u8(0x7E, 0x62, 0x48),
    path: Color::srgb_u8(0xC2, 0xBA, 0x9A),
    bank: Color::srgb_u8(0x8A, 0x82, 0x68),
    riverbed: Color::srgb_u8(0x6E, 0x8C, 0x88),
    pond: Color::srgb_u8(0x4F, 0xB6, 0xC8),
    bloom: Color::srgb_u8(0xE2, 0xE8, 0xD8),
    marsh: Color::srgb_u8(0x6E, 0x9A, 0x64),
    abyss: Color::srgb_u8(0x4A, 0x56, 0x68),
    rope: Color::srgb_u8(0xA0, 0x8A, 0x64),
    haze: Color::srgb_u8(0xC8, 0xD8, 0xE4),
    sky: Color::srgb_u8(0xB0, 0xC8, 0xE4),
    sky_brightness: 650.0,
    sun: Color::srgb_u8(0xE8, 0xF0, 0xFF),
    sun_illuminance: 7_000.0,
    wilds: Wilds::Fen,
};

/// Sunken Bastion: a pale fortress on a crisp, clear morning. Fresh turf between chalk-white
/// stone, bright water in the cistern, the mist far beyond the walls.
const BASTION: Theme = Theme {
    grass: Color::srgb_u8(0x78, 0xAC, 0x62),
    meadow: Color::srgb_u8(0x90, 0xBE, 0x6A),
    fern: Color::srgb_u8(0x5C, 0x9C, 0x5A),
    pine: Color::srgb_u8(0x44, 0x84, 0x5C),
    slate: Color::srgb_u8(0x9A, 0xA4, 0xB2),
    stone: Color::srgb_u8(0xC2, 0xC8, 0xD0),
    wall: Color::srgb_u8(0xA8, 0xB0, 0xBC),
    bark: Color::srgb_u8(0x7E, 0x62, 0x48),
    path: Color::srgb_u8(0xC8, 0xBE, 0x9C),
    bank: Color::srgb_u8(0x8A, 0x82, 0x68),
    riverbed: Color::srgb_u8(0x68, 0x86, 0x8C),
    pond: Color::srgb_u8(0x4F, 0xAE, 0xCE),
    bloom: Color::srgb_u8(0xD8, 0xDE, 0xE8),
    marsh: Color::srgb_u8(0x6E, 0x9A, 0x5E),
    abyss: Color::srgb_u8(0x4A, 0x56, 0x68),
    rope: Color::srgb_u8(0xA0, 0x8A, 0x64),
    haze: Color::srgb_u8(0xC6, 0xD4, 0xE2),
    sky: Color::srgb_u8(0xAE, 0xC6, 0xE6),
    sky_brightness: 650.0,
    sun: Color::srgb_u8(0xF4, 0xF6, 0xFF),
    sun_illuminance: 7_500.0,
    wilds: Wilds::Fen,
};

/// Hollow Wilds: a fresh green wood with violet heather, pale lavender where the distance goes,
/// under a warm bright sun.
const HOLLOW: Theme = Theme {
    grass: Color::srgb_u8(0x76, 0xB2, 0x54),
    meadow: Color::srgb_u8(0x92, 0xC4, 0x5E),
    fern: Color::srgb_u8(0x58, 0xA0, 0x52),
    pine: Color::srgb_u8(0x3E, 0x84, 0x56),
    slate: Color::srgb_u8(0x94, 0xA0, 0xB6),
    stone: Color::srgb_u8(0xB4, 0xBA, 0xC8),
    wall: Color::srgb_u8(0x9E, 0xA6, 0xB6),
    bark: Color::srgb_u8(0x80, 0x60, 0x3E),
    path: Color::srgb_u8(0xCC, 0xBC, 0x94),
    bank: Color::srgb_u8(0x8A, 0x7E, 0x60),
    riverbed: Color::srgb_u8(0x6E, 0x8C, 0x84),
    pond: Color::srgb_u8(0x55, 0xAE, 0xD6),
    bloom: Color::srgb_u8(0xB0, 0x7E, 0xDC),
    marsh: Color::srgb_u8(0x7A, 0xA2, 0x58),
    abyss: Color::srgb_u8(0x50, 0x56, 0x6C),
    rope: Color::srgb_u8(0xA4, 0x8C, 0x62),
    haze: Color::srgb_u8(0xCC, 0xC2, 0xE0),
    sky: Color::srgb_u8(0xBC, 0xC0, 0xE4),
    sky_brightness: 680.0,
    sun: Color::srgb_u8(0xFF, 0xF0, 0xDE),
    sun_illuminance: 8_500.0,
    wilds: Wilds::Forest,
};

/// Riven Gorge: golden highlands on a bright afternoon. Sun-dried grass in terraces of light
/// sandstone, round trees in fresh green, a gorge falling to cool shadow, pale peach dust far
/// off.
const HIGHLANDS: Theme = Theme {
    grass: Color::srgb_u8(0xC2, 0xB4, 0x5E),
    meadow: Color::srgb_u8(0xD4, 0xC4, 0x68),
    fern: Color::srgb_u8(0x9A, 0x90, 0x48),
    pine: Color::srgb_u8(0x5E, 0x90, 0x50),
    slate: Color::srgb_u8(0xC0, 0x98, 0x78),
    stone: Color::srgb_u8(0xD4, 0xB4, 0x94),
    wall: Color::srgb_u8(0xBE, 0x9C, 0x80),
    bark: Color::srgb_u8(0x8A, 0x62, 0x40),
    path: Color::srgb_u8(0xDC, 0xC8, 0x9A),
    bank: Color::srgb_u8(0xA8, 0x88, 0x6A),
    riverbed: Color::srgb_u8(0x8C, 0x8A, 0x6E),
    pond: Color::srgb_u8(0x55, 0xB2, 0xC2),
    bloom: Color::srgb_u8(0xD8, 0x8A, 0x52),
    marsh: Color::srgb_u8(0xA8, 0xA0, 0x58),
    abyss: Color::srgb_u8(0x5A, 0x54, 0x68),
    rope: Color::srgb_u8(0xB0, 0x94, 0x5E),
    haze: Color::srgb_u8(0xE4, 0xD4, 0xBC),
    sky: Color::srgb_u8(0xC4, 0xBC, 0xE0),
    sky_brightness: 620.0,
    sun: Color::srgb_u8(0xFF, 0xE8, 0xC0),
    sun_illuminance: 8_500.0,
    wilds: Wilds::Highlands,
};
