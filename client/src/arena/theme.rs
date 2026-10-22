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

/// River Crossing: a green forest glade at dusk, the world's palette (`palette`).
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
    sky_brightness: 340.0,
    sun: SUN,
    sun_illuminance: 4_000.0,
    wilds: Wilds::Forest,
};

/// Sunken Keep: a drowned fen by moonlight. Sea-green tussocks in black water, dead snags and
/// dark willows under a cold, dim moon and blue mist, so the torches on the keep carry the
/// scene; pale bog cotton for flowers.
const FEN: Theme = Theme {
    grass: Color::srgb_u8(0x5A, 0x72, 0x62),
    meadow: Color::srgb_u8(0x6C, 0x80, 0x6E),
    fern: Color::srgb_u8(0x3D, 0x5C, 0x54),
    pine: Color::srgb_u8(0x22, 0x37, 0x38),
    slate: Color::srgb_u8(0x5C, 0x66, 0x76),
    stone: Color::srgb_u8(0x7C, 0x84, 0x8F),
    wall: Color::srgb_u8(0x5A, 0x64, 0x70),
    bark: Color::srgb_u8(0x48, 0x3F, 0x38),
    path: Color::srgb_u8(0x78, 0x7A, 0x6E),
    bank: Color::srgb_u8(0x48, 0x48, 0x40),
    riverbed: Color::srgb_u8(0x2C, 0x38, 0x3C),
    pond: Color::srgb_u8(0x48, 0x78, 0x86),
    bloom: Color::srgb_u8(0xBE, 0xC6, 0xB4),
    marsh: Color::srgb_u8(0x4C, 0x58, 0x48),
    abyss: Color::srgb_u8(0x2A, 0x31, 0x3E),
    rope: Color::srgb_u8(0x68, 0x60, 0x50),
    haze: Color::srgb_u8(0x54, 0x68, 0x80),
    sky: Color::srgb_u8(0x58, 0x76, 0xB8),
    sky_brightness: 260.0,
    sun: Color::srgb_u8(0xBC, 0xD2, 0xFF),
    sun_illuminance: 2_400.0,
    wilds: Wilds::Fen,
};

/// Sunken Bastion: a drowned fortress under a cold moon. Grey-green turf between pale stone,
/// black water in the cistern, a thin blue mist; the torches at the gates carry the color.
const BASTION: Theme = Theme {
    grass: Color::srgb_u8(0x5E, 0x6E, 0x58),
    meadow: Color::srgb_u8(0x6E, 0x7C, 0x62),
    fern: Color::srgb_u8(0x42, 0x58, 0x4C),
    pine: Color::srgb_u8(0x26, 0x38, 0x36),
    slate: Color::srgb_u8(0x66, 0x6E, 0x7E),
    stone: Color::srgb_u8(0x88, 0x8E, 0x9A),
    wall: Color::srgb_u8(0x62, 0x6A, 0x78),
    bark: Color::srgb_u8(0x46, 0x3E, 0x38),
    path: Color::srgb_u8(0x7E, 0x7C, 0x70),
    bank: Color::srgb_u8(0x4A, 0x48, 0x42),
    riverbed: Color::srgb_u8(0x28, 0x32, 0x38),
    pond: Color::srgb_u8(0x38, 0x56, 0x66),
    bloom: Color::srgb_u8(0xB0, 0xBC, 0xC8),
    marsh: Color::srgb_u8(0x4E, 0x58, 0x4C),
    abyss: Color::srgb_u8(0x26, 0x2C, 0x3A),
    rope: Color::srgb_u8(0x64, 0x5C, 0x4E),
    haze: Color::srgb_u8(0x4E, 0x62, 0x7C),
    sky: Color::srgb_u8(0x5C, 0x7A, 0xBC),
    sky_brightness: 260.0,
    sun: Color::srgb_u8(0xC2, 0xD6, 0xFF),
    sun_illuminance: 2_200.0,
    wilds: Wilds::Fen,
};

/// Hollow Wilds: a haunted wood in the last light. Deep, mossy greens gone grey-blue, violet
/// heather, slate stone, under a low, cold violet sun and a dim purple haze, so the camp's torches
/// and the fights carry the scene.
const HOLLOW: Theme = Theme {
    grass: Color::srgb_u8(0x5A, 0x82, 0x48),
    meadow: Color::srgb_u8(0x7C, 0x96, 0x50),
    fern: Color::srgb_u8(0x40, 0x72, 0x42),
    pine: Color::srgb_u8(0x2E, 0x54, 0x40),
    slate: Color::srgb_u8(0x62, 0x66, 0x78),
    stone: Color::srgb_u8(0x7E, 0x80, 0x8E),
    wall: Color::srgb_u8(0x5E, 0x60, 0x70),
    bark: Color::srgb_u8(0x4A, 0x3C, 0x36),
    path: Color::srgb_u8(0x7E, 0x78, 0x68),
    bank: Color::srgb_u8(0x4C, 0x44, 0x3C),
    riverbed: Color::srgb_u8(0x30, 0x3A, 0x44),
    pond: Color::srgb_u8(0x46, 0x6E, 0x8A),
    bloom: Color::srgb_u8(0xB0, 0x7E, 0xDC),
    marsh: Color::srgb_u8(0x50, 0x5A, 0x44),
    abyss: Color::srgb_u8(0x2E, 0x2A, 0x40),
    rope: Color::srgb_u8(0x70, 0x62, 0x50),
    haze: Color::srgb_u8(0x94, 0x84, 0xAC),
    sky: Color::srgb_u8(0x9A, 0xA0, 0xC2),
    sky_brightness: 400.0,
    sun: Color::srgb_u8(0xFF, 0xD9, 0xA8),
    sun_illuminance: 4_800.0,
    wilds: Wilds::Forest,
};

/// Riven Gorge: autumn highlands at sunset. Dry golden grass rising in terraces of red
/// sandstone, scrub and round trees in brown and rust, a gorge falling to plum shadow, under a
/// deep orange sun and dusty amber haze.
const HIGHLANDS: Theme = Theme {
    grass: Color::srgb_u8(0x96, 0x8A, 0x3E),
    meadow: Color::srgb_u8(0xB2, 0x9C, 0x44),
    fern: Color::srgb_u8(0x78, 0x5E, 0x3E),
    pine: Color::srgb_u8(0x3C, 0x45, 0x32),
    slate: Color::srgb_u8(0x88, 0x6C, 0x5A),
    stone: Color::srgb_u8(0xA0, 0x88, 0x70),
    wall: Color::srgb_u8(0x86, 0x6E, 0x5E),
    bark: Color::srgb_u8(0x58, 0x40, 0x30),
    path: Color::srgb_u8(0xA6, 0x8C, 0x68),
    bank: Color::srgb_u8(0x68, 0x4C, 0x3A),
    riverbed: Color::srgb_u8(0x48, 0x3E, 0x36),
    pond: Color::srgb_u8(0x4E, 0x78, 0x78),
    bloom: Color::srgb_u8(0xC8, 0x74, 0x38),
    marsh: Color::srgb_u8(0x68, 0x62, 0x40),
    abyss: Color::srgb_u8(0x38, 0x2A, 0x34),
    rope: Color::srgb_u8(0x88, 0x72, 0x50),
    haze: Color::srgb_u8(0xC8, 0x94, 0x68),
    sky: Color::srgb_u8(0x98, 0x90, 0xBC),
    sky_brightness: 340.0,
    sun: Color::srgb_u8(0xFF, 0x9E, 0x54),
    sun_illuminance: 4_200.0,
    wilds: Wilds::Highlands,
};
