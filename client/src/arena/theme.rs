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

/// River Crossing: a deep forest glade at dusk, the world's palette (`palette`).
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
    sky_brightness: 300.0,
    sun: SUN,
    sun_illuminance: 2_400.0,
    wilds: Wilds::Forest,
};

/// Sunken Keep: a drowned fen in the last grey light. Sea-green tussocks in dark water, dead
/// snags, blue mist; the keep's torches carry the warmth. Bog cotton for flowers.
const FEN: Theme = Theme {
    grass: Color::srgb_u8(0x4E, 0x66, 0x54),
    meadow: Color::srgb_u8(0x5C, 0x72, 0x60),
    fern: Color::srgb_u8(0x38, 0x55, 0x4C),
    pine: Color::srgb_u8(0x24, 0x38, 0x3A),
    slate: Color::srgb_u8(0x56, 0x5E, 0x6A),
    stone: Color::srgb_u8(0x73, 0x7A, 0x84),
    wall: Color::srgb_u8(0x56, 0x5E, 0x68),
    bark: Color::srgb_u8(0x44, 0x38, 0x2E),
    path: Color::srgb_u8(0x6B, 0x6B, 0x5E),
    bank: Color::srgb_u8(0x42, 0x3F, 0x36),
    riverbed: Color::srgb_u8(0x2A, 0x34, 0x38),
    pond: Color::srgb_u8(0x3A, 0x64, 0x74),
    bloom: Color::srgb_u8(0xAE, 0xB6, 0xA6),
    marsh: Color::srgb_u8(0x44, 0x50, 0x3F),
    abyss: Color::srgb_u8(0x23, 0x2A, 0x34),
    rope: Color::srgb_u8(0x5E, 0x55, 0x45),
    haze: Color::srgb_u8(0x3C, 0x48, 0x54),
    sky: Color::srgb_u8(0x5A, 0x70, 0x98),
    sky_brightness: 280.0,
    sun: Color::srgb_u8(0xC6, 0xD6, 0xF4),
    sun_illuminance: 2_000.0,
    wilds: Wilds::Fen,
};

/// Sunken Bastion: a drowned fortress as the light fails. Grey-green turf between cold stone,
/// black water in the cistern, blue mist; the torches at the gates carry the color.
const BASTION: Theme = Theme {
    grass: Color::srgb_u8(0x53, 0x64, 0x4C),
    meadow: Color::srgb_u8(0x5F, 0x70, 0x50),
    fern: Color::srgb_u8(0x3E, 0x54, 0x44),
    pine: Color::srgb_u8(0x27, 0x3A, 0x34),
    slate: Color::srgb_u8(0x5C, 0x62, 0x70),
    stone: Color::srgb_u8(0x7E, 0x84, 0x90),
    wall: Color::srgb_u8(0x5A, 0x62, 0x70),
    bark: Color::srgb_u8(0x42, 0x39, 0x2F),
    path: Color::srgb_u8(0x6E, 0x6C, 0x60),
    bank: Color::srgb_u8(0x44, 0x42, 0x3A),
    riverbed: Color::srgb_u8(0x28, 0x33, 0x3A),
    pond: Color::srgb_u8(0x32, 0x58, 0x6C),
    bloom: Color::srgb_u8(0xA6, 0xAE, 0xBC),
    marsh: Color::srgb_u8(0x47, 0x52, 0x44),
    abyss: Color::srgb_u8(0x22, 0x28, 0x33),
    rope: Color::srgb_u8(0x5C, 0x54, 0x44),
    haze: Color::srgb_u8(0x39, 0x43, 0x4F),
    sky: Color::srgb_u8(0x56, 0x70, 0x9C),
    sky_brightness: 280.0,
    sun: Color::srgb_u8(0xCA, 0xD8, 0xF6),
    sun_illuminance: 2_100.0,
    wilds: Wilds::Fen,
};

/// Hollow Wilds: a haunted wood in failing light. Deep mossy greens, violet heather, a dim
/// purple cast to the distance; the camp's torches and the fights carry the warmth.
const HOLLOW: Theme = Theme {
    grass: Color::srgb_u8(0x4C, 0x68, 0x3E),
    meadow: Color::srgb_u8(0x5E, 0x78, 0x46),
    fern: Color::srgb_u8(0x3A, 0x5C, 0x38),
    pine: Color::srgb_u8(0x27, 0x42, 0x2E),
    slate: Color::srgb_u8(0x5A, 0x5C, 0x6A),
    stone: Color::srgb_u8(0x75, 0x77, 0x7F),
    wall: Color::srgb_u8(0x58, 0x5A, 0x66),
    bark: Color::srgb_u8(0x4A, 0x3A, 0x30),
    path: Color::srgb_u8(0x6E, 0x67, 0x54),
    bank: Color::srgb_u8(0x46, 0x3E, 0x34),
    riverbed: Color::srgb_u8(0x2C, 0x38, 0x40),
    pond: Color::srgb_u8(0x3A, 0x5C, 0x7A),
    bloom: Color::srgb_u8(0x8A, 0x68, 0xB0),
    marsh: Color::srgb_u8(0x48, 0x53, 0x40),
    abyss: Color::srgb_u8(0x26, 0x24, 0x36),
    rope: Color::srgb_u8(0x64, 0x58, 0x43),
    haze: Color::srgb_u8(0x47, 0x43, 0x5A),
    sky: Color::srgb_u8(0x6A, 0x6C, 0x96),
    sky_brightness: 300.0,
    sun: Color::srgb_u8(0xE2, 0xD8, 0xEE),
    sun_illuminance: 2_600.0,
    wilds: Wilds::Forest,
};

/// Riven Gorge: dusky moorland as the sun dies. Dry grass gone umber in terraces of dark
/// sandstone, a gorge falling to plum shadow, the last ember of a sun through dusty air.
const HIGHLANDS: Theme = Theme {
    grass: Color::srgb_u8(0x6F, 0x67, 0x3E),
    meadow: Color::srgb_u8(0x7E, 0x74, 0x44),
    fern: Color::srgb_u8(0x5A, 0x4C, 0x34),
    pine: Color::srgb_u8(0x34, 0x44, 0x34),
    slate: Color::srgb_u8(0x6A, 0x5A, 0x4C),
    stone: Color::srgb_u8(0x80, 0x70, 0x5E),
    wall: Color::srgb_u8(0x66, 0x5A, 0x4E),
    bark: Color::srgb_u8(0x4C, 0x3A, 0x2A),
    path: Color::srgb_u8(0x7C, 0x6E, 0x54),
    bank: Color::srgb_u8(0x54, 0x44, 0x36),
    riverbed: Color::srgb_u8(0x3C, 0x3A, 0x32),
    pond: Color::srgb_u8(0x35, 0x59, 0x6A),
    bloom: Color::srgb_u8(0xA0, 0x64, 0x38),
    marsh: Color::srgb_u8(0x58, 0x54, 0x33),
    abyss: Color::srgb_u8(0x2C, 0x26, 0x30),
    rope: Color::srgb_u8(0x6E, 0x5F, 0x45),
    haze: Color::srgb_u8(0x4E, 0x46, 0x44),
    sky: Color::srgb_u8(0x6E, 0x64, 0x88),
    sky_brightness: 280.0,
    sun: Color::srgb_u8(0xF0, 0xD0, 0xA8),
    sun_illuminance: 2_800.0,
    wilds: Wilds::Highlands,
};
