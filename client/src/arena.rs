//! The arena's look: low-poly clearings in a ring of forest, each map in its own theme
//! (`Theme`), with water, chasms, bridges, marshes, ruined walls and torches. See
//! `docs/art-direction.md`. The scene is built from our room's map (`arena_shared::map`), tile
//! by tile, so what you see is exactly what blocks movement and shots; and built again when the
//! map changes.
//!
//! Flat-shaded, untextured, one color per face. The world stays in a calm middle band of
//! saturation; only fighters, projectiles and torch flames go above it.

use arena_shared::classes::{ClassDef, Shot};
use arena_shared::map::{Map, MapId, Tile};
use bevy::asset::RenderAssetUsages;
use bevy::color::Mix;
use bevy::light::{CascadeShadowConfig, CascadeShadowConfigBuilder};
use bevy::mesh::PrimitiveTopology;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use crate::cloth::{Column, Drape, Wardrobe, Weave};
use crate::rooms::CurrentMap;
use crate::sculpt::{Section, bevel_box, grid, hemmed, sheet, smoothed, sweep, tube};

pub mod palette {
    use bevy::color::Color;

    // The world: below ~50% saturation, 25-72% lightness.
    pub const MOSS: Color = Color::srgb_u8(0x6F, 0x8F, 0x4E);
    pub const MEADOW: Color = Color::srgb_u8(0x8A, 0xA0, 0x5A);
    pub const FERN: Color = Color::srgb_u8(0x4E, 0x77, 0x48);
    pub const PINE: Color = Color::srgb_u8(0x2E, 0x4B, 0x3C);
    pub const SLATE: Color = Color::srgb_u8(0x6C, 0x76, 0x84);
    pub const STONE: Color = Color::srgb_u8(0x84, 0x8A, 0x93);
    pub const WALL: Color = Color::srgb_u8(0x69, 0x71, 0x7D);
    pub const BARK: Color = Color::srgb_u8(0x5E, 0x48, 0x38);
    pub const PATH: Color = Color::srgb_u8(0x95, 0x8C, 0x6E);
    pub const BANK: Color = Color::srgb_u8(0x5A, 0x51, 0x41);
    pub const RIVERBED: Color = Color::srgb_u8(0x3D, 0x4A, 0x44);
    pub const POND: Color = Color::srgb_u8(0x4F, 0x86, 0x82);
    pub const HEATHER: Color = Color::srgb_u8(0x9A, 0x86, 0xB6);
    /// Boggy ground, wetter and browner than moss.
    pub const MARSH: Color = Color::srgb_u8(0x5E, 0x66, 0x45);
    /// The bottom of a chasm, the darkest the world goes.
    pub const ABYSS: Color = Color::srgb_u8(0x3A, 0x42, 0x50);
    /// Hemp rope on the rope bridges.
    pub const ROPE: Color = Color::srgb_u8(0x7A, 0x6A, 0x50);
    /// Near-black leathers: trousers and hoods; boots and eye sockets.
    pub const LEATHER: Color = Color::srgb_u8(0x2E, 0x27, 0x24);
    pub const DARK_LEATHER: Color = Color::srgb_u8(0x17, 0x14, 0x14);
    /// Ashen fur trim (mantles, tails).
    pub const FUR: Color = Color::srgb_u8(0x55, 0x55, 0x52);
    /// Charred, ash-black antlers; pauldrons, gauntlets, sword guards.
    pub const ASH: Color = Color::srgb_u8(0x2B, 0x2A, 0x2E);
    /// The javelinist's wraith-hunter's coat: a cold near-black with a breath of moss in it, its
    /// darker lining and hem, and the ashen grey of its undertunic and wraps.
    pub const HUNTER: Color = Color::srgb_u8(0x1A, 0x1E, 0x1C);
    pub const HUNTER_DARK: Color = Color::srgb_u8(0x0D, 0x10, 0x0F);
    pub const HUNTER_ASH: Color = Color::srgb_u8(0x3A, 0x3E, 0x3C);
    /// The revenant's blackened plate, and the faded grey of its bandana.
    pub const BLACK_STEEL: Color = Color::srgb_u8(0x2A, 0x2E, 0x36);
    pub const SHROUD: Color = Color::srgb_u8(0x4B, 0x47, 0x54);
    /// The revenant's robe, a cold near-black, and its darker tatters and leg wraps.
    pub const ROBE: Color = Color::srgb_u8(0x24, 0x21, 0x29);
    pub const TATTERS: Color = Color::srgb_u8(0x17, 0x15, 0x1B);
    /// The frost mage's robe, a deep cold navy, and its darker lining; rime is its pale, frosted
    /// trim (mantle, hem, belt).
    pub const FROST_ROBE: Color = Color::srgb_u8(0x1E, 0x27, 0x36);
    pub const FROST_DARK: Color = Color::srgb_u8(0x12, 0x17, 0x21);
    pub const RIME: Color = Color::srgb_u8(0xB4, 0xC8, 0xD2);
    /// The frost mage's accent: a clear, cold blue on its sash, cuffs, front panel and staff
    /// bindings.
    pub const FROST_BLUE: Color = Color::srgb_u8(0x3A, 0x86, 0xD4);
    /// What the frost mage's eyes, staff crystals, shoulder shards and robe runes glow with.
    pub const FROST_GLOW: Color = Color::srgb_u8(0x6C, 0xC6, 0xFF);
    /// Pale, cold blade steel, and dark iron for shafts.
    pub const STEEL: Color = Color::srgb_u8(0xA9, 0xB5, 0xBC);
    pub const IRON: Color = Color::srgb_u8(0x5E, 0x66, 0x6E);
    /// Polished silver: the javelinist's spearhead, the brightest thing on it.
    pub const SILVER: Color = Color::srgb_u8(0xDC, 0xE4, 0xE8);
    /// The glow in fighters' eyes: a pale ghostly green.
    pub const WISP: Color = Color::srgb_u8(0xB8, 0xFF, 0xD6);
    /// Eyes burning a cold, spectral white.
    pub const SOUL: Color = Color::srgb_u8(0xF2, 0xFA, 0xFF);
    pub const HAZE: Color = Color::srgb_u8(0xA9, 0xB8, 0xB6);
    /// HUD text.
    pub const INK: Color = Color::srgb_u8(0x1E, 0x2A, 0x23);

    // Who a fighter is to you, in what marks it (health bar, ground ring, minimap dot):
    // never in its model, its shots or its swings. You are blue, enemies red and
    // allies (once there are teams) green.
    pub const YOU: Color = Color::srgb_u8(0x4C, 0x9E, 0xE0);
    pub const ENEMY: Color = Color::srgb_u8(0xD9, 0x45, 0x3B);
    pub const ALLY: Color = Color::srgb_u8(0x5C, 0xC4, 0x6A);
    /// Spectral blue: spirit spears (whoever throws them) and rift-step streaks glow with it.
    pub const SPIRIT: Color = Color::srgb_u8(0x8F, 0xD8, 0xFF);
    /// Ice: the frost mage's crystals, its nova, and the ice that freezes a fighter in place.
    /// Paler and whiter than spirit.
    pub const ICE: Color = Color::srgb_u8(0xC4, 0xEE, 0xFF);
    /// What a slowed fighter's colors are multiplied by: a cold blue cast.
    pub const FROSTBITE: Color = Color::srgb_u8(0x8C, 0xB8, 0xFF);

    // Light. Fire is the one warm, saturated thing in the world: small, static and flickering.
    pub const SUN: Color = Color::srgb_u8(0xFF, 0xE2, 0xC4);
    pub const SKY: Color = Color::srgb_u8(0x8F, 0xA4, 0xC2);
    pub const TORCH_FLAME: Color = Color::srgb_u8(0xFF, 0xB2, 0x57);
    pub const TORCH_LIGHT: Color = Color::srgb_u8(0xFF, 0x94, 0x43);

    // Pickups glow like spirits do: a heal's fresh green and a haste's quick blue.
    pub const HEAL: Color = Color::srgb_u8(0x7C, 0xF2, 0x9A);
    pub const HASTE: Color = Color::srgb_u8(0x58, 0xA8, 0xFF);

    /// The UI's own theme, "Bronze": panels, text and accents, apart from the world's.
    /// What marks a fighter, an ability or a pickup keeps its world color. Named by role, so a
    /// new theme only changes the values. The loading screen in `client/web/index.html` mirrors
    /// these.
    pub mod ui {
        use bevy::color::Color;

        /// The backdrop behind full-screen menus: `SELECTED` at a quarter.
        pub const BACKDROP: Color = Color::srgb_u8(0x22, 0x14, 0x0A);
        /// Panels, cards and tooltips; text on an `ACCENT` fill: `SELECTED` at 40%.
        pub const PANEL: Color = Color::srgb_u8(0x37, 0x20, 0x10);
        /// What's selected, and filled buttons that aren't the main one.
        pub const SELECTED: Color = Color::srgb_u8(0x89, 0x51, 0x29);
        /// Secondary text and thin borders.
        pub const MUTED: Color = Color::srgb_u8(0x8A, 0x6E, 0x29);
        /// Text: `ACCENT` halfway to white.
        pub const TEXT: Color = Color::srgb_u8(0xD6, 0xBC, 0x98);
        /// The one accent: the role, what's selected, the main button.
        pub const ACCENT: Color = Color::srgb_u8(0xAD, 0x7A, 0x32);
    }
}

use palette::*;

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
        match map {
            MapId::Crossing => &GLADE,
            MapId::Keep => &FEN,
            MapId::Gorge => &HIGHLANDS,
        }
    }
}

/// River Crossing: a green forest glade at dusk, the world's palette above.
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
    sky_brightness: 160.0,
    sun: SUN,
    sun_illuminance: 3_600.0,
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
    haze: Color::srgb_u8(0x6A, 0x7C, 0x8A),
    sky: Color::srgb_u8(0x6E, 0x88, 0xB4),
    sky_brightness: 210.0,
    sun: Color::srgb_u8(0xC4, 0xD4, 0xFF),
    sun_illuminance: 2_600.0,
    wilds: Wilds::Fen,
};

/// Riven Gorge: autumn highlands at sunset. Dry golden grass rising in terraces of red
/// sandstone, scrub and round trees in brown and rust, a gorge falling to plum shadow, under a
/// deep orange sun and dusty amber haze.
const HIGHLANDS: Theme = Theme {
    grass: Color::srgb_u8(0x8A, 0x84, 0x50),
    meadow: Color::srgb_u8(0xA0, 0x90, 0x5A),
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
    bloom: Color::srgb_u8(0xB6, 0x78, 0x4E),
    marsh: Color::srgb_u8(0x68, 0x62, 0x40),
    abyss: Color::srgb_u8(0x38, 0x2A, 0x34),
    rope: Color::srgb_u8(0x88, 0x72, 0x50),
    haze: Color::srgb_u8(0xB4, 0xA0, 0x8E),
    sky: Color::srgb_u8(0x98, 0x88, 0xB0),
    sky_brightness: 150.0,
    sun: Color::srgb_u8(0xFF, 0xCA, 0x9C),
    sun_illuminance: 3_400.0,
    wilds: Wilds::Highlands,
};

/// Gameplay (x, y) to world (x, height, -y): the gameplay plane is the ground, +y is "up" on
/// screen. The one place this convention lives.
pub fn to_world(p: Vec2, height: f32) -> Vec3 {
    Vec3::new(p.x, height, -p.y)
}

pub fn to_gameplay(w: Vec3) -> Vec2 {
    Vec2::new(w.x, -w.z)
}

/// Matte, so flat facets read cleanly. Vertex colors (if any) multiply `color`.
pub fn matte(color: Color) -> StandardMaterial {
    StandardMaterial {
        base_color: color,
        perceptual_roughness: 0.95,
        metallic: 0.0,
        reflectance: 0.2,
        ..default()
    }
}

/// Self-lit: shots, flames and markers that should read the same in light and shade.
pub fn glow(color: Color, strength: f32) -> StandardMaterial {
    StandardMaterial { emissive: LinearRgba::from(color) * strength, unlit: true, ..matte(color) }
}

/// See-through glow: swishes, streaks and auras.
pub fn translucent(color: Color, alpha: f32, strength: f32) -> StandardMaterial {
    StandardMaterial { alpha_mode: AlphaMode::Blend, ..glow(color.with_alpha(alpha), strength) }
}

/// One normal per face: the low-poly look.
pub fn faceted(mesh: Mesh) -> Mesh {
    mesh.with_duplicated_vertices().with_computed_flat_normals()
}

/// `part` merged into `all`, either of them indexed or not: `Mesh::merge` alone drops or
/// scrambles triangles when one side is indexed and the other isn't.
fn merge_into(all: &mut Mesh, part: &Mesh) {
    fn indexed(mesh: &mut Mesh) {
        if mesh.indices().is_none() {
            let count = mesh.count_vertices() as u32;
            mesh.insert_indices(bevy::mesh::Indices::U32((0..count).collect()));
        }
    }
    indexed(all);
    let mut part = part.clone();
    indexed(&mut part);
    all.merge(&part).expect("merged meshes share attributes");
}

/// Class ids that have their own figure in `fighter_mesh`. Every class in `classes.ron` needs
/// one (and its parts in `fighter_rig` and its moves in `rig.rs`): the client won't start
/// without them, and a test checks every class has one.
pub const FIGHTER_LOOKS: [&str; 3] = ["javelinist", "revenant", "frost_mage"];

/// Class ids whose shots have their own look in `shot_look` (the rest throw round bolts).
pub const SHOT_LOOKS: [&str; 2] = ["javelinist", "frost_mage"];

fn cylinder(r: f32, h: f32, sides: u32) -> Mesh {
    Cylinder::new(r, h).mesh().resolution(sides).build()
}

fn cone(r: f32, h: f32, sides: u32) -> Mesh {
    Cone::new(r, h).mesh().resolution(sides).build()
}

/// Faceted parts merged into one mesh.
fn merge_parts(parts: Vec<Mesh>) -> Mesh {
    sculpted(parts.into_iter().map(faceted).collect())
}

/// Parts merged into one smoothly shaded mesh (each part keeps its own smooth normals instead of
/// being faceted): the modern, simple look of fighters.
fn sculpted(parts: Vec<Mesh>) -> Mesh {
    parts
        .into_iter()
        .reduce(|mut all, p| {
            merge_into(&mut all, &p);
            all
        })
        .expect("at least one part")
}

/// A straight band (a binding, a grip wrap, a ferrule) round the Y axis from `from` to `to`.
fn band(from: f32, to: f32, radius: f32, sides: u32, color: Color) -> Mesh {
    sweep(&[Vec3::Y * from, Vec3::Y * to], &[radius, radius], sides, color)
}

/// A smooth ball: heads, knuckles, bone knobs.
fn ball(radius: f32) -> Mesh {
    Sphere::new(radius).mesh().ico(2).unwrap()
}

/// Gives every vertex `color`, so one material can carry several colors: vertex colors multiply
/// the material's base color (white on a body, tinted while it's chilled). All parts merged
/// together need it, or none.
fn tinted(mut mesh: Mesh, color: Color) -> Mesh {
    let count = mesh.count_vertices();
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![color.to_linear().to_f32_array(); count]);
    mesh
}

/// The javelinist's spear, standing along +Y with its point at the origin, carried and thrown
/// alike: a long, dark shaft (in `shaft`), faceted like worked wood and thickening a little toward
/// the head; a long leaf-shaped head in `metal`, flat in Z (so it shows from above as it flies),
/// sharp-edged with a ridge down its middle, two small barbs swept back from its base, on a
/// ridged socket bound below in `wrap`; a grip wrapped in `wrap` between two `metal` bands; and a
/// short metal butt spike.
fn javelin(shaft: Color, metal: Color, wrap: Color) -> Mesh {
    const LENGTH: f32 = 2.0;
    const HEAD: f32 = 0.44;
    let head = faceted(tube(
        &smoothed(
            &[(HEAD, 0.014, 0.02), (0.34, 0.016, 0.055), (0.2, 0.013, 0.05), (0.08, 0.008, 0.028), (0.0, 0.0, 0.0)]
                .map(|(back, depth, width)| Section::new(Vec3::Y * -back, Vec2::new(depth, width), metal).squared(1.0).exact()),
            2,
        ),
        4,
        (true, false),
    ));
    let mut parts = vec![head];
    for side in [1.0, -1.0] {
        let barb = [Vec3::new(0.0, -0.36, side * 0.045), Vec3::new(0.0, -0.42, side * 0.07), Vec3::new(0.0, -0.47, side * 0.074)];
        parts.push(sweep(&barb, &[0.011, 0.007, 0.0], 6, metal));
    }
    // The socket, ridged where it takes the head and where it grips the shaft.
    let socket = [(HEAD - 0.02, 0.022), (HEAD, 0.03), (HEAD + 0.02, 0.026), (HEAD + 0.1, 0.026), (HEAD + 0.12, 0.031), (HEAD + 0.14, 0.027)];
    parts.push(tube(&socket.map(|(back, r)| Section::round(Vec3::Y * -back, r, metal).exact()), 12, (true, true)));
    // Cord binding the socket to the shaft.
    for i in 0..4 {
        let at = HEAD + 0.16 + 0.022 * i as f32;
        parts.push(band(-at, -(at + 0.018), 0.027, 10, wrap));
    }
    let wood = faceted(tube(
        &[(HEAD + 0.1, 0.025), (0.9, 0.024), (1.5, 0.022), (LENGTH - 0.1, 0.02)]
            .map(|(back, r)| Section::round(Vec3::Y * -back, r, shaft).squared(2.2)),
        8,
        (false, false),
    ));
    parts.push(wood);
    // The grip: wrapped between two bands.
    parts.push(band(-1.1, -1.13, 0.03, 12, metal));
    for i in 0..7 {
        let at = 1.14 + 0.026 * i as f32;
        parts.push(band(-at, -(at + 0.022), 0.027, 10, wrap));
    }
    parts.push(band(-1.33, -1.36, 0.03, 12, metal));
    // The butt spike.
    parts.push(sweep(&[Vec3::Y * -(LENGTH - 0.12), Vec3::Y * -(LENGTH - 0.07), Vec3::Y * -LENGTH], &[0.024, 0.02, 0.0], 8, metal));
    sculpted(parts)
}

/// A fighter's parts that move on their own (posed by `rig.rs`); every class has them. Each
/// hangs from its joint at the origin; the fighter faces +X, its right (weapon) side is +Z. Drawn
/// in the fighter's own material (colors in the vertices) except the eyes.
pub struct RigMeshes {
    /// Turns to look where the fighter walks or aims.
    pub head: Mesh,
    /// An arm, hanging down (-Y) from the shoulder.
    pub arm: Mesh,
    /// A leg, hanging down from the hip.
    pub leg: Mesh,
    /// The weapon in the right hand, pointing up (+Y) from the grip.
    pub held: Mesh,
    /// Eyes in the head's space, drawn glowing cold white (`palette::SOUL`, everyone's).
    pub eyes: Mesh,
    /// What the glowing parts below glow with.
    pub glow: Color,
    /// Parts of the held weapon (in its space) and of the body (in the fighter's space) drawn
    /// glowing, like the eyes: crystals, runes.
    pub held_glow: Option<Mesh>,
    pub body_glow: Option<Mesh>,
    /// A see-through shell of the same glow around the held weapon, in its space: shown while the
    /// attack's `follow_up` is armed (`rig::show_auras`).
    pub held_aura: Option<Mesh>,
    /// A sharp glint of white light shown with the aura, and where it sits (the weapon's point).
    pub held_glint: Option<(Mesh, Vec3)>,
    /// Cloth that moves (see `cloth.rs`), in the fighter's space.
    pub wardrobe: Option<Wardrobe>,
}

/// Where a rig's parts attach, in the fighter's space (feet at the origin). Hips and shoulders
/// are given for the right side; the left mirrors them in Z.
pub const RIG_NECK: Vec3 = Vec3::new(0.0, 1.42, 0.0);
pub const RIG_SHOULDER: Vec3 = Vec3::new(0.0, 1.32, 0.3);
pub const RIG_HIP: Vec3 = Vec3::new(0.0, 0.8, 0.11);
/// Where the hand is, in the arm's space.
pub const RIG_HAND: Vec3 = Vec3::new(0.0, -0.52, 0.0);

/// Where the eyes sit in a `deep_hood`'s opening (and ±z, the other one).
const HOOD_EYES: Vec3 = Vec3::new(0.165, 0.2, 0.055);

/// A class's moving parts: its hood, sleeves, legs, weapon and (if it has one) cape.
pub fn fighter_rig(class_key: &str) -> RigMeshes {
    match class_key {
        "javelinist" => RigMeshes {
            head: javelinist_hood(),
            arm: javelinist_arm(),
            leg: javelinist_leg(),
            held: held_spear(),
            eyes: spectral_eyes(JAVELINIST_HOOD),
            glow: palette::SOUL,
            held_glow: None,
            body_glow: None,
            held_aura: None,
            held_glint: None,
            wardrobe: Some(javelinist_wardrobe()),
        },
        "revenant" => RigMeshes {
            head: revenant_head(),
            arm: revenant_arm(),
            leg: booted_leg(palette::ROBE, palette::DARK_LEATHER, 0.082, 0.07),
            held: sword(),
            eyes: slit_eyes(REVENANT_EYES),
            glow: palette::WISP,
            held_glow: Some(sword_glow()),
            body_glow: Some(revenant_glow()),
            held_aura: Some(blade_aura()),
            held_glint: Some((glint(), Vec3::Y * BLADE_LENGTH)),
            wardrobe: Some(revenant_wardrobe()),
        },
        "frost_mage" => RigMeshes {
            head: frost_crowned_hood(),
            arm: frost_mage_arm(),
            leg: booted_leg(palette::FROST_DARK, palette::DARK_LEATHER, 0.1, 0.08),
            held: ice_staff(),
            eyes: spectral_eyes(Vec2::ONE),
            glow: palette::FROST_GLOW,
            held_glow: Some(staff_crystals()),
            body_glow: Some(frost_mage_glow()),
            held_aura: None,
            held_glint: None,
            wardrobe: Some(frost_mage_wardrobe()),
        },
        _ => unreachable!("no rig for class {class_key:?} (add it to FIGHTER_LOOKS)"),
    }
}

/// Small, narrow, slanted eyes burning deep in a `deep_hood` drawn `hood` times its size (width,
/// height).
fn spectral_eyes(hood: Vec2) -> Mesh {
    let h = HOOD_EYES;
    slit_eyes(Vec3::new(h.x * hood.x - 0.012, h.y * hood.y - 0.01, h.z * hood.x * 0.95))
}

/// Small, narrow, slanted eyes at `at` and its mirror (-z) in the head's space. Drawn in a glow
/// material (white here).
fn slit_eyes(at: Vec3) -> Mesh {
    merge_parts(
        [1.0, -1.0]
            .map(|side| {
                let slit = Sphere::new(0.02).mesh().ico(1).unwrap().scaled_by(Vec3::new(0.5, 0.42, 1.5));
                let slant = Quat::from_rotation_x(side * 0.35);
                tinted(slit.rotated_by(slant).translated_by(at * Vec3::new(1.0, 1.0, side)), Color::WHITE)
            })
            .to_vec(),
    )
}

/// A long crystal standing on Y, base at the origin: a narrow prism ending in a point.
fn ice_shard(radius: f32, height: f32) -> Mesh {
    let body = cylinder(radius, height * 0.7, 5).translated_by(Vec3::Y * height * 0.35);
    let point = cone(radius, height * 0.3, 5).translated_by(Vec3::Y * height * 0.85);
    faceted(sculpted(vec![body, point]))
}

/// The middle of the staff's head (above the grip), and how far its crescents reach out.
const STAFF_HEAD: f32 = 1.32;
const STAFF_REACH: f32 = 0.17;
/// How far around (radians from the bottom of the head) each crescent half curls up.
const STAFF_CURL: f32 = 2.5;
/// The sides the staff's crescent halves curl up on.
const CRESCENTS: [Vec3; 4] = [Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z];

/// A point `t` (0..1) of the way along a crescent half, from the bottom of the staff's head up
/// around `side` to its tip.
fn crescent(side: Vec3, t: f32) -> Vec3 {
    let angle = STAFF_CURL * t;
    Vec3::Y * STAFF_HEAD + (side * angle.sin() - Vec3::Y * angle.cos()) * STAFF_REACH
}

/// The staff's crystals, drawn glowing: a long one floating in the middle of its head, a small
/// one above it, and a sliver growing out of each crescent's tip.
fn staff_crystals() -> Mesh {
    let crystal = gem_mesh(0.07).scaled_by(Vec3::new(0.85, 2.7, 0.85));
    let mut parts = vec![
        tinted(crystal.translated_by(Vec3::Y * (STAFF_HEAD + 0.02)), Color::WHITE),
        tinted(ice_shard(0.022, 0.1).translated_by(Vec3::Y * (STAFF_HEAD + 0.25)), Color::WHITE),
    ];
    for side in CRESCENTS {
        let tip = crescent(side, 1.0);
        let out = (side + Vec3::Y).normalize();
        let sliver = ice_shard(0.016, 0.11).rotated_by(Quat::from_rotation_arc(Vec3::Y, out));
        parts.push(tinted(sliver.translated_by(tip), Color::WHITE));
    }
    sculpted(parts)
}

/// What glows on the frost mage's body: ice shards growing out of its mantle, a crystal at its
/// sash, and runes circling the hem of its robe.
fn frost_mage_glow() -> Mesh {
    let mut parts = Vec::new();
    for side in [1.0, -1.0] {
        for (i, (out, height)) in [(0.25, 0.22), (0.6, 0.16), (0.95, 0.12)].into_iter().enumerate() {
            let shard = ice_shard(0.03, height).rotated_by(Quat::from_rotation_x(side * out));
            let at = Vec3::new(-0.06 + 0.06 * i as f32, 1.42, side * (0.2 + 0.03 * i as f32));
            parts.push(tinted(shard.translated_by(at), Color::WHITE));
        }
    }
    let gem = gem_mesh(0.045).scaled_by(Vec3::new(0.6, 1.4, 1.0));
    parts.push(tinted(gem.translated_by(Vec3::new(0.225, 0.868, 0.0)), Color::WHITE));
    sculpted(parts)
}

/// Ice locking a rooted fighter in place, around its feet at the origin: a ring of crystals
/// leaning in up to the knees, a few taller ones climbing the robe. Drawn see-through.
pub fn ice_prison_mesh() -> Mesh {
    let mut parts = Vec::new();
    for i in 0..9 {
        let around = i as f32 / 9.0 * std::f32::consts::TAU + 0.3;
        let height = 0.55 + 0.35 * ((i * 5) % 3) as f32 / 2.0;
        let shard = ice_shard(0.09, height).rotated_by(Quat::from_rotation_z(0.28));
        let placed = shard.translated_by(Vec3::new(-0.42, 0.0, 0.0)).rotated_by(Quat::from_rotation_y(around));
        parts.push(placed);
    }
    sculpted(parts)
}

/// A nova's parts, all around its center at the origin (see `render::show_novas`).
#[derive(Clone)]
pub struct NovaMeshes<M = Mesh> {
    /// Frost over the ground it reaches, thickest at the edge.
    pub disc: M,
    /// The shockwave: a band at its edge, fading inward, that races out from the center.
    pub ring: M,
    /// One shard bursting out of the ground, leaning out along +X, 1 m tall.
    pub shard: M,
    /// Ice erupting from where the staff strikes the ground: a spray of tall shards.
    pub eruption: M,
}

pub fn nova_meshes(radius: f32) -> NovaMeshes {
    let steps = 48;
    let point = |i: u32, r: f32| {
        let t = i as f32 / steps as f32 * std::f32::consts::TAU;
        Vec3::new(t.cos() * r, 0.0, -t.sin() * r)
    };
    let mut disc = FlatMesh::default();
    let mut ring = FlatMesh::default();
    for i in 0..steps {
        disc.tri_faded([Vec3::ZERO, point(i, radius), point(i + 1, radius)], [0.25, 1.0, 1.0]);
        let inner = radius * 0.72;
        ring.tri_faded([point(i, inner), point(i, radius), point(i + 1, radius)], [0.0, 1.0, 1.0]);
        ring.tri_faded([point(i, inner), point(i + 1, radius), point(i + 1, inner)], [0.0, 1.0, 0.0]);
    }
    let mut eruption = Vec::new();
    for i in 0..9 {
        let around = i as f32 / 9.0 * std::f32::consts::TAU;
        let height = 0.7 + 0.5 * ((i * 4) % 5) as f32 / 4.0;
        let lean = 0.35 + 0.25 * ((i * 2) % 3) as f32 / 2.0;
        let shard = ice_shard(0.07, height).rotated_by(Quat::from_rotation_z(-lean));
        eruption.push(shard.translated_by(Vec3::X * 0.15).rotated_by(Quat::from_rotation_y(around)));
    }
    NovaMeshes {
        disc: disc.build(),
        ring: ring.build(),
        shard: ice_shard(0.11, 1.0).rotated_by(Quat::from_rotation_z(-0.35)),
        eruption: sculpted(eruption),
    }
}

/// Frost under a slowed fighter's feet, flat on the ground: a thin ring and a six-armed
/// snowflake inside it (each arm branching twice), fading toward the middle.
pub fn frost_rune_mesh() -> Mesh {
    let mut b = FlatMesh::default();
    let at = |angle: f32, r: f32| Vec3::new(angle.cos() * r, 0.0, -angle.sin() * r);
    // The ring.
    let steps = 36;
    let (inner, outer) = (0.56, 0.64);
    for i in 0..steps {
        let (a, z) = (i as f32 / steps as f32 * std::f32::consts::TAU, (i + 1) as f32 / steps as f32 * std::f32::consts::TAU);
        b.tri_faded([at(a, inner), at(a, outer), at(z, outer)], [0.7, 1.0, 1.0]);
        b.tri_faded([at(a, inner), at(z, outer), at(z, inner)], [0.7, 1.0, 0.7]);
    }
    // A thin bar from `from` to `to`, `width` wide, opaque at `to`.
    let mut bar = |from: Vec3, to: Vec3, width: f32, alpha: [f32; 2]| {
        let along = (to - from).normalize();
        let side = Vec3::new(along.z, 0.0, -along.x) * width / 2.0;
        let corners = [from - side, to - side, to + side, from + side];
        b.tri_faded([corners[0], corners[1], corners[2]], [alpha[0], alpha[1], alpha[1]]);
        b.tri_faded([corners[0], corners[2], corners[3]], [alpha[0], alpha[1], alpha[0]]);
    };
    for i in 0..6 {
        let angle = i as f32 / 6.0 * std::f32::consts::TAU;
        bar(at(angle, 0.12), at(angle, 0.5), 0.05, [0.2, 1.0]);
        for (r, length) in [(0.26, 0.1), (0.38, 0.08)] {
            for turn in [0.7, -0.7] {
                let from = at(angle, r);
                let to = from + at(angle + turn, length);
                bar(from, to, 0.035, [0.6, 1.0]);
            }
        }
    }
    b.build()
}

/// The spear in hand, gripped a little behind its middle.
fn held_spear() -> Mesh {
    carried_javelin().translated_by(Vec3::Y * GRIP_TO_TIP)
}

/// The javelinist's javelin in its own colors, point at the origin.
fn carried_javelin() -> Mesh {
    javelin(palette::HUNTER_DARK, palette::SILVER, palette::LEATHER)
}

/// A section of a fighter part: `half` (depth, width) across at height `y`, offset `x` forward.
fn ring(x: f32, y: f32, half: (f32, f32), color: Color) -> Section {
    Section::new(Vec3::new(x, y, 0.0), Vec2::new(half.0, half.1), color)
}

/// A belt (or a sash) round the waist from `y.0` up to `y.1`, `half` (depth, width) across at its
/// edge, `x` forward: a band standing a little proud of its rolled edges.
fn belt(x: f32, y: (f32, f32), half: (f32, f32), color: Color) -> Mesh {
    let at = |y: f32, grow: f32| ring(x, y, (half.0 + grow, half.1 + grow), color).exact();
    tube(&[at(y.0, 0.0), at(y.0 + 0.012, 0.005), at(y.1 - 0.012, 0.004), at(y.1, -0.001)], 40, (true, true))
}

/// How a tabard moves: heavier than the rest, held close.
const TABARD_WEAVE: Weave = Weave { hold: (9.0, 2.0), damping: 5.0, air: 0.3, inertia: 0.3, gravity: 8.0, flutter: 0.4, joined: false, wisps: 0.0 };

/// A tabard: a narrow panel hanging from `top` (at the belt, in front) down by `drop`, `half` wide
/// at the top and at the bottom, curving back round the legs (`curve`), its end torn up to
/// `torn.0` deep in strips `torn.1` apart; in `colors` from the top down (the last for the rest).
fn tabard(top: Vec3, drop: Vec3, half: (f32, f32), curve: f32, torn: (f32, f32), colors: &[Color]) -> Drape {
    const ROWS: usize = 12;
    const COLS: usize = 8;
    let at = |v: f32| top + drop * v;
    let rows = grid(ROWS, COLS, (0.0, 1.0), |u, v| {
        let z = (u * 2.0 - 1.0) * (half.0 + (half.1 - half.0) * v);
        let tear = if v >= 1.0 { torn.0 * (u * COLS as f32 * torn.1).sin().abs() } else { 0.0 };
        at(v) + Vec3::new(-curve * z * z, -tear, z)
    });
    Drape::new(sheet(&rows, colors, 0.008), None, vec![[0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0].map(at).to_vec()], TABARD_WEAVE)
}

/// A deep hood, its rim (in `rim`) sunk over the collar, rising through `top` (the sections
/// above the brow), its opening dark (`void`) and framed by a heavy lip of cloth, in the head's
/// space (the neck at the origin, facing +X). The eyes go at `HOOD_EYES`.
fn deep_hood(cloth: Color, rim: Color, void: Color, top: &[Section]) -> Vec<Mesh> {
    let mut sections = vec![
        ring(-0.03, -0.05, (0.17, 0.19), rim),
        ring(-0.035, 0.01, (0.18, 0.195), cloth),
        ring(-0.04, 0.08, (0.19, 0.2), cloth),
    ];
    sections.extend_from_slice(top);
    let hood = tube(&smoothed(&sections, 3), 26, (true, false));
    let dark = tinted(ball(1.0).scaled_by(Vec3::new(0.07, 0.13, 0.12)).translated_by(Vec3::new(0.1, 0.19, 0.0)), void);
    vec![hood, dark, hood_brim(cloth, void.mix(&cloth, 0.45), 1.0), hood_brim(void.mix(&cloth, 0.45), void, -1.0)]
}

/// The cloth framing a `deep_hood`'s opening: a band folded back over the hood round it, deeper
/// over the brow, where it dips to a point hanging over the face, gathered in uneven creases,
/// rolled along its edge in `edge`. `layer` 1 is the outer fold; -1 the lining just inside it, narrower and set back.
fn hood_brim(cloth: Color, edge: Color, layer: f32) -> Mesh {
    const STEPS: usize = 28;
    let outer = layer > 0.0;
    let sections: Vec<Section> = (0..=STEPS)
        .map(|i| {
            let a = -1.95 + 3.9 * i as f32 / STEPS as f32;
            let brow = a.cos().max(0.0);
            // The point over the face: a dip at the top of the arch.
            let peak = (-(a / 0.32).powi(2)).exp();
            let creases = 1.0 + 0.22 * (a * 7.0 + 0.6).sin() * (0.6 + 0.4 * (a * 3.3).cos());
            // Tapering away at its ends, down at the jaw, into the hood's sides.
            let end = ((1.95 - a.abs()) / 0.6).clamp(0.0, 1.0);
            // The fold: how far it lies back over the hood, and how thick it is.
            let fold = if outer { (0.014 + 0.02 * brow + 0.012 * peak) * creases * (0.4 + 0.6 * end) } else { 0.008 };
            let thick = if outer { (0.01 + 0.007 * brow) * (0.5 + 0.5 * end) } else { 0.01 + 0.004 * brow };
            // Round the opening (curving back round the sides of the head, and leaning back with
            // the hood above the brow), the fold lying
            // back from its edge over the hood; the lining just inside the edge.
            let out = Vec3::new(0.0, a.cos() / 0.155, a.sin() / 0.135).normalize();
            let y = 0.17 + 0.15 * a.cos() - 0.045 * peak;
            let rim = Vec3::new(0.075 + 0.08 * a.cos() + 0.028 * peak - 0.3 * (y - 0.24).max(0.0), y, 0.135 * a.sin());
            let at = if outer { rim + out * thick * 0.6 - Vec3::X * fold * 0.5 } else { rim - out * 0.012 - Vec3::X * 0.014 };
            let color = if outer && (i == 0 || i == STEPS) { edge } else { cloth };
            Section::new(at, Vec2::new(fold, thick), color)
        })
        .collect();
    let brim = tube(&sections, 10, (true, true));
    if !outer {
        return brim;
    }
    // A rolled edge along the front of the fold, darker, so it reads as turned back cloth.
    let roll: Vec<Section> = sections
        .iter()
        .map(|s| Section::round(s.at + Vec3::X * s.half.x * 0.85, s.half.y * 0.75, edge))
        .collect();
    sculpted(vec![brim, tube(&roll, 8, (true, true))])
}


/// A leg hanging from the hip in `cloth`, `thigh` and `knee` thick (wide for baggy trousers),
/// into a soft `boot` with a toe.
fn booted_leg(cloth: Color, boot: Color, thigh: f32, knee: f32) -> Mesh {
    tube(
        &[
            ring(0.0, 0.05, (thigh * 0.95, thigh * 0.95), cloth),
            ring(0.0, -0.2, (thigh, thigh * 0.95), cloth),
            ring(0.01, -0.4, (knee, knee * 0.96), cloth),
            ring(0.0, -0.55, (knee * 0.92, knee * 0.88), cloth),
            ring(0.0, -0.6, (0.078, 0.076), boot),
            ring(0.015, -0.7, (0.085, 0.078), boot),
            ring(0.04, -0.76, (0.11, 0.082), boot).squared(3.0),
            ring(0.05, -0.8, (0.115, 0.08), boot).squared(3.0),
        ],
        16,
        (true, true),
    )
}

/// A fist closing below a cuff: in `color`, the hand at `RIG_HAND`, from `top` down.
fn fist(color: Color, top: f32, width: f32) -> Mesh {
    tube(
        &[
            ring(0.0, top, (width * 0.9, width * 0.9), color),
            ring(0.0, -0.45, (width, width * 0.96), color),
            ring(0.01, -0.48, (width * 1.04, width * 0.94), color).squared(2.6),
            ring(0.015, -0.55, (width * 1.08, width * 0.95), color).squared(2.6),
            ring(0.01, -0.6, (width * 0.68, width * 0.72), color),
            ring(0.0, -0.62, (0.0, 0.0), color),
        ],
        16,
        (true, false),
    )
}

/// The revenant's body, a fallen paladin's, lean and quick, under its moving cloth
/// (`revenant_wardrobe`): a fitted long coat, its bodice closed down the front with iron clasps
/// between dark lapels, a high collar flaring open at the throat, a wide belt (the coat's tails
/// hang from under it), and a single pauldron of blackened steel on the left shoulder.
fn revenant_body() -> Mesh {
    let bodice = tube(
        &smoothed(
            &[
                ring(0.0, 0.74, (0.145, 0.175), palette::ROBE),
                ring(0.005, 0.88, (0.147, 0.177), palette::ROBE),
                ring(0.015, 1.02, (0.153, 0.19), palette::ROBE).folded(0.015, 7),
                ring(0.02, 1.17, (0.155, 0.215), palette::ROBE).squared(2.2),
                // The shoulders: squared out over the shoulder joints (the sleeves come out from
                // under them), sloping up to the collar.
                ring(0.01, 1.27, (0.145, 0.27), palette::ROBE).squared(2.6),
                ring(0.0, 1.34, (0.13, 0.31), palette::ROBE).squared(3.2),
                ring(-0.01, 1.4, (0.11, 0.23), palette::ROBE).squared(2.6),
                ring(-0.01, 1.47, (0.08, 0.1), palette::TATTERS),
            ],
            3,
        ),
        44,
        (true, true),
    );
    let mut parts = vec![bodice];
    // The lapels: dark bands from the collar down to the waist, meeting in a V.
    for side in [1.0, -1.0] {
        let lapel = [(0.105, 1.4, 0.065, 0.026), (0.158, 1.25, 0.05, 0.03), (0.172, 1.08, 0.022, 0.022), (0.162, 0.92, 0.008, 0.014)]
            .map(|(x, y, z, w)| Section::new(Vec3::new(x, y, z * side), Vec2::new(0.006, w), palette::TATTERS));
        parts.push(tube(&smoothed(&lapel, 3), 8, (true, true)));
    }
    // Iron clasps down the closure.
    for y in [0.97, 1.05, 1.13] {
        let depth = 0.153 + 0.02 * (y - 0.88) / 0.3;
        parts.push(bevel_box(Vec3::new(0.012, 0.022, 0.05), 0.004, palette::IRON).translated_by(Vec3::new(depth + 0.02, y, 0.0)));
    }
    // The collar: standing high round the neck, flaring out as it rises, open at the throat.
    const OPEN: f32 = 0.5;
    let collar_rows: Vec<Vec<Vec3>> = (0..=6)
        .map(|i| i as f32 / 6.0)
        .map(|v| {
            (0..=24)
                .map(|j| OPEN + j as f32 / 24.0 * (std::f32::consts::TAU - 2.0 * OPEN))
                .map(|a| Vec3::new(-0.01 + a.cos() * (0.1 + 0.05 * v * v), 1.39 + 0.17 * v, a.sin() * (0.118 + 0.05 * v * v)))
                .collect()
        })
        .collect();
    parts.push(sheet(&collar_rows, &[palette::ROBE, palette::ROBE, palette::ROBE, palette::ROBE, palette::TATTERS, palette::TATTERS], 0.012));
    // The belt, over the top of the coat's tails, its iron buckle a little off center.
    parts.push(belt(0.005, (0.825, 0.895), (0.168, 0.198), palette::DARK_LEATHER));
    parts.push(bevel_box(Vec3::new(0.025, 0.06, 0.065), 0.008, palette::IRON).translated_by(Vec3::new(0.172, 0.86, 0.04)));
    parts.push(shoulder_plates(-1.0));
    sculpted(parts)
}

/// Small plates of blackened steel over the `side` (±1, +Z the right) shoulder: a broad cap and
/// two lames below it, each curving from the front of the shoulder over it to the back,
/// overlapping down the arm.
fn shoulder_plates(side: f32) -> Mesh {
    let mut parts = Vec::new();
    // Each plate (from, to: how far out along the shoulder), sloping down with the shoulder and
    // overlapping the next, the cap the broadest.
    // Arched clear over the coat's squared shoulder (front to back and over its top).
    for (i, (from, to)) in [(0.2, 0.31), (0.28, 0.35), (0.325, 0.385)].into_iter().enumerate() {
        let shrink = 1.0 - 0.06 * i as f32;
        let rows: Vec<Vec<Vec3>> = (0..=4)
            .map(|r| from + (to - from) * r as f32 / 4.0)
            .map(|z| {
                // Out along the shoulder it drops and curls in a little tighter.
                let down = 0.55 * (z - 0.2) + 0.015 * i as f32;
                let (reach, rise) = (0.178 * shrink - 0.05 * (z - 0.2), 0.122 * shrink - 0.04 * (z - 0.2));
                (0..=14)
                    .map(|k| -1.2 + 2.4 * k as f32 / 14.0)
                    .map(|t| Vec3::new(reach * t.sin(), 1.33 - down + rise * t.cos(), z * side))
                    .collect()
            })
            .collect();
        parts.push(sheet(&rows, &[palette::BLACK_STEEL], 0.014));
    }
    // Stacked from the lowest up, each over the one below.
    sculpted(parts.into_iter().rev().collect())
}

/// What glows on the revenant's body: the brooch at its collar, a long diamond of wisp light with
/// a smaller one either side.
fn revenant_glow() -> Mesh {
    let at = Vec3::new(0.15, 1.31, 0.0);
    let mut parts = vec![tinted(gem_mesh(0.024).scaled_by(Vec3::new(0.45, 1.8, 1.0)).translated_by(at), Color::WHITE)];
    for side in [1.0, -1.0] {
        let small = gem_mesh(0.013).scaled_by(Vec3::new(0.45, 1.4, 1.0)).rotated_by(Quat::from_rotation_x(side * 0.5));
        parts.push(tinted(small.translated_by(at + Vec3::new(-0.004, -0.008, side * 0.034)), Color::WHITE));
    }
    sculpted(parts)
}

/// Where one tail of the revenant's coat is: `side` (+1 right, -1 left), `u` (0..1) from its front
/// edge round to the split down the back, `v` (0..1) from the waist down to the hem (at the knee in
/// front, sweeping down to the ground behind); `ragged` for the cloth itself (folds and the torn
/// hem), not for the chains it hangs from.
fn coat_tail_point(side: f32, u: f32, v: f32, ragged: bool) -> Vec3 {
    // Open wide at the front, this far (radians) either side of it, and split down the back.
    const OPEN: f32 = 0.55;
    const SPLIT: f32 = 0.05;
    let span = std::f32::consts::PI - SPLIT - OPEN;
    let a = if side > 0.0 { OPEN + u * span } else { std::f32::consts::TAU - OPEN - u * span };
    let flare = v.powf(1.3);
    let (depth, width) = (0.19 + 0.12 * flare, 0.226 + 0.1 * flare);
    let hem = 0.07 + 0.36 * ((1.0 + a.cos()) / 2.0).powf(1.4);
    let mut y = 0.87 + (hem - 0.87) * v;
    let mut out = 1.0;
    if ragged {
        out += v * 0.06 * (a * 7.0 + v * 1.6).sin() * (0.6 + 0.4 * (a * 2.7).sin()) + 0.02 * (a * 5.0 + v * 6.0).sin();
        let scallop = 0.5 + 0.5 * (a * 6.0 + 0.4).sin();
        let strip = (a * 15.0 + 1.1).sin().max(0.0).powi(5);
        y -= 0.17 * (0.3 * scallop + 0.7 * strip) * v.powi(8);
    }
    Vec3::new(a.cos() * depth * out, y, a.sin() * width * out)
}

/// The revenant's moving cloth: the two long tails of its coat, split up the back and open wide
/// at the front, sweeping down behind it, their torn hems smouldering in wisp light; a tattered
/// tabard down the front.
fn revenant_wardrobe() -> Wardrobe {
    const ROWS: usize = 18;
    const COLS: usize = 22;
    let tail = |side: f32| {
        let point = |u, v| coat_tail_point(side, u, v, true);
        let chains = (0..4).map(|c| [0.06, 0.3, 0.53, 0.77, 1.0].map(|v| coat_tail_point(side, c as f32 / 3.0, v, false)).to_vec()).collect();
        Drape::new(
            sheet(&grid(ROWS, COLS, (0.0, 1.0), point), &hemmed(ROWS, 3, palette::ROBE, palette::TATTERS), 0.01),
            Some(sheet(&grid(2, COLS, (0.975, 1.0), point), &[Color::WHITE], 0.02)),
            chains,
            Weave { hold: (6.0, 1.2), damping: 5.5, air: 0.35, inertia: 0.4, gravity: 6.5, flutter: 0.7, joined: true, wisps: 3.0 },
        )
    };

    // The tabard: down the front, curving round the legs, its end torn.
    let tabard = tabard(Vec3::new(0.21, 0.84, 0.0), Vec3::new(0.1, -0.6, 0.0), (0.065, 0.055), 1.0, (0.07, 2.3), &[palette::ASH]);

    Wardrobe {
        drapes: vec![tail(1.0), tail(-1.0), tabard],
        trim: palette::WISP,
        body: vec![
            Column { center: Vec2::ZERO, half: Vec2::new(0.17, 0.205), from: 0.3, to: 0.9 },
            Column { center: Vec2::ZERO, half: Vec2::new(0.2, 0.25), from: 0.9, to: 1.4 },
        ],
        leg_radius: 0.1,
    }
}

/// The revenant's skull, from the neck up (in the head's space): (height, forward, half depth,
/// half width).
const REVENANT_SKULL: [(f32, f32, f32, f32); 9] = [
    (-0.04, 0.0, 0.05, 0.048),
    (0.04, 0.005, 0.058, 0.052),
    (0.09, 0.02, 0.085, 0.07),
    (0.15, 0.012, 0.104, 0.085),
    (0.21, 0.0, 0.112, 0.092),
    (0.27, -0.012, 0.108, 0.09),
    (0.32, -0.022, 0.085, 0.072),
    (0.355, -0.03, 0.04, 0.035),
    (0.365, -0.032, 0.0, 0.0),
];

/// The skull's section at height `y`: (forward, half depth, half width).
fn revenant_skull_at(y: f32) -> (f32, f32, f32) {
    let s = REVENANT_SKULL;
    let k = s.windows(2).position(|w| y <= w[1].0).unwrap_or(s.len() - 2);
    let (a, b) = (s[k], s[k + 1]);
    let t = ((y - a.0) / (b.0 - a.0)).clamp(0.0, 1.0);
    (a.1 + (b.1 - a.1) * t, a.2 + (b.2 - a.2) * t, a.3 + (b.3 - a.3) * t)
}

/// A band of cloth wrapped once round the head: centered `height(t)` and `half(t)` high at `t`
/// radians round from the front, `grow` times the skull's size there.
fn head_wrap(height: impl Fn(f32) -> f32, half: impl Fn(f32) -> f32, grow: f32, color: Color) -> Mesh {
    let band: Vec<Section> = (0..=36)
        .map(|i| i as f32 / 36.0 * std::f32::consts::TAU)
        .map(|t| {
            let y = height(t);
            let (x, depth, width) = revenant_skull_at(y);
            let at = Vec3::new(x + depth * grow * t.cos(), y, width * grow * t.sin());
            Section::new(at, Vec2::new(0.007, half(t)), color).folded(0.05, 3)
        })
        .collect();
    tube(&band, 8, (true, true))
}

/// The revenant's head, wrapped all over in a faded bandana but for a dark slit its eyes burn
/// white in: the crown wound in layered bands, a wrap round the brow (lower at the front) and
/// another over the jaw and mouth, tied in a knot behind.
fn revenant_head() -> Mesh {
    let cloth = palette::SHROUD;
    let fold = cloth.darker(0.04);
    // The face, dark, up to the brow (the wrapping covers the rest).
    let skull = tube(
        &smoothed(&REVENANT_SKULL[..6].iter().map(|&(y, x, depth, width)| ring(x, y, (depth, width), palette::TATTERS).exact()).collect::<Vec<_>>(), 2),
        28,
        (true, true),
    );
    // The crown, under the bands: the skull's top, wrapped close.
    let crown = tube(
        &smoothed(&REVENANT_SKULL[4..].iter().map(|&(y, x, depth, width)| ring(x, y, (depth * 1.07, width * 1.07), cloth).folded(0.02, 7)).collect::<Vec<_>>(), 2),
        28,
        (true, true),
    );
    let mut parts = vec![skull, crown];
    // Bands wound round the crown, each tilted its own way, overlapping.
    for (i, (y, tilt, side)) in [(0.262, 0.022, 0.012), (0.3, -0.02, -0.015)].into_iter().enumerate() {
        let color = if i % 2 == 0 { fold } else { cloth };
        parts.push(head_wrap(move |t| y + tilt * t.cos() + side * t.sin(), |_| 0.022, 1.1, color));
    }
    // Round the brow, just above the eyes at the front, wider behind to meet the jaw wrap.
    parts.push(head_wrap(|t| 0.226 - 0.045 * (1.0 - t.cos()) / 2.0, |t| 0.022 + 0.05 * (1.0 - t.cos()) / 2.0, 1.1, cloth));
    // Over the jaw and mouth, up to just under the eyes.
    parts.push(head_wrap(|_| 0.1, |_| 0.052, 1.08, fold));
    parts.push(head_wrap(|t| 0.13 - 0.02 * (1.0 - t.cos()) / 2.0, |_| 0.024, 1.13, cloth));
    // The knot behind: a bulge of cloth, its two cut ends just poking out below it.
    let knot = Vec3::new(-0.13, 0.215, 0.0);
    parts.push(tinted(ball(1.0).scaled_by(Vec3::new(0.036, 0.04, 0.05)).translated_by(knot), cloth));
    for side in [1.0, -1.0] {
        let end = [knot + Vec3::new(-0.01, -0.01, side * 0.018), knot + Vec3::new(-0.035, -0.045, side * 0.03), knot + Vec3::new(-0.045, -0.075, side * 0.036)];
        parts.push(sweep(&end, &[0.016, 0.012, 0.0], 6, fold));
    }
    sculpted(parts)
}

/// Where the revenant's eyes burn, in the slit its wrappings leave.
const REVENANT_EYES: Vec3 = Vec3::new(0.108, 0.175, 0.04);

/// One iron link of a chain, `at`, upright and turned across the last one when `turned`.
fn chain_link(at: Vec3, turned: bool) -> Mesh {
    let loop_points: Vec<Vec3> = (0..=12)
        .map(|i| i as f32 / 12.0 * std::f32::consts::TAU)
        .map(|a| {
            let (side, up) = (0.013 * a.sin(), 0.022 * a.cos());
            at + if turned { Vec3::new(side, up, 0.0) } else { Vec3::new(0.0, up, side) }
        })
        .collect();
    sweep(&loop_points, &[0.0055; 13], 6, palette::IRON)
}

/// A fitted coat sleeve starting under the coat's shoulder and widening a little to a turned-back,
/// tattered cuff, over a dark leather
/// gauntlet closing into a fist, an iron manacle round the wrist trailing a broken chain.
fn revenant_arm() -> Mesh {
    let sleeve = tube(
        &smoothed(
            &[
                ring(0.0, 0.0, (0.058, 0.06), palette::ROBE),
                ring(0.0, -0.1, (0.064, 0.066), palette::ROBE).folded(0.02, 6),
                ring(0.0, -0.21, (0.07, 0.07), palette::ROBE).folded(0.035, 6),
                ring(0.0, -0.27, (0.078, 0.078), palette::ROBE).folded(0.04, 6),
                ring(-0.004, -0.28, (0.09, 0.09), palette::TATTERS).folded(0.03, 6),
                ring(-0.008, -0.34, (0.096, 0.096), palette::TATTERS).torn(0.05).trailing(0.04).folded(0.04, 6),
            ],
            2,
        ),
        22,
        (true, true),
    );
    let gauntlet = tube(
        &[
            ring(0.0, -0.2, (0.062, 0.062), palette::DARK_LEATHER),
            ring(0.0, -0.36, (0.07, 0.07), palette::DARK_LEATHER),
            ring(0.0, -0.42, (0.078, 0.074), palette::DARK_LEATHER.darker(0.05)),
            ring(0.0, -0.44, (0.06, 0.058), palette::DARK_LEATHER),
        ],
        20,
        (true, false),
    );
    let manacle = tube(
        &[
            ring(0.0, -0.35, (0.08, 0.08), palette::IRON).exact(),
            ring(0.0, -0.36, (0.085, 0.085), palette::IRON).exact(),
            ring(0.0, -0.39, (0.085, 0.085), palette::IRON).exact(),
            ring(0.0, -0.4, (0.08, 0.08), palette::IRON).exact(),
        ],
        20,
        (true, true),
    );
    let mut parts = vec![sleeve, gauntlet, fist(palette::DARK_LEATHER, -0.44, 0.064), manacle];
    // The broken chain, hanging from the back of the manacle.
    for (i, y) in [-0.41, -0.445, -0.48].into_iter().enumerate() {
        parts.push(chain_link(Vec3::new(-0.092, y, 0.0), i % 2 == 1));
    }
    sculpted(parts)
}

/// The revenant's sword, gripped at the origin, blade up (+Y): a long, slender pale blade swelling
/// a little toward a symmetric point, a crossguard whose arms droop forward to points, a grip bound in ash and leather, and a
/// faceted pommel spike. A line of wisp light runs down its fuller (`sword_glow`).
fn sword() -> Mesh {
    let blade = faceted(tube(
        &smoothed(
            &BLADE.map(|(y, depth, width, z)| Section::new(Vec3::new(0.0, y, z), Vec2::new(depth, width), palette::STEEL).squared(1.0).exact()),
            2,
        ),
        4,
        (true, false),
    ));
    let mut parts = vec![blade, bevel_box(Vec3::new(0.07, 0.075, 0.1), 0.015, palette::ASH).translated_by(Vec3::Y * 0.1)];
    // The guard's arms, out from the middle and drooping forward to points.
    for side in [1.0, -1.0] {
        let arm: Vec<Vec3> = (0..=4).map(|i| i as f32 / 4.0).map(|t| Vec3::new(0.0, 0.1 - 0.07 * t * t, side * 0.19 * t)).collect();
        parts.push(sweep(&arm, &[0.03, 0.027, 0.022, 0.014, 0.0], 10, palette::ASH));
    }
    // The grip, bound in bands.
    for i in 0..6 {
        let (from, to) = (-0.17 + 0.038 * i as f32, -0.17 + 0.038 * (i + 1) as f32);
        let (r, color) = if i % 2 == 0 { (0.031, palette::ASH) } else { (0.029, palette::DARK_LEATHER) };
        parts.push(band(from, to, r, 12, color));
    }
    let pommel = faceted(tube(
        &[
            Section::round(Vec3::Y * -0.17, 0.034, palette::ASH).exact(),
            Section::round(Vec3::Y * -0.2, 0.045, palette::ASH).exact(),
            Section::round(Vec3::Y * -0.24, 0.04, palette::ASH).exact(),
            Section::round(Vec3::Y * -0.3, 0.0, palette::ASH).exact(),
        ],
        6,
        (true, false),
    ));
    parts.push(pommel);
    sculpted(parts)
}

/// The sword's blade, as (height, half depth, half width, how far its middle is set toward the
/// front edge): long and slender, swelling a little toward a symmetric point.
const BLADE: [(f32, f32, f32, f32); 6] =
    [(0.12, 0.018, 0.04, 0.0), (0.45, 0.017, 0.043, 0.0), (1.1, 0.015, 0.046, 0.0), (1.45, 0.013, 0.041, 0.0), (1.63, 0.008, 0.023, 0.0), (1.76, 0.0, 0.0, 0.0)];

/// How long the revenant's blade is, from the grip to its point.
pub const BLADE_LENGTH: f32 = BLADE[BLADE.len() - 1].0;

/// What glows on the revenant's sword: a line of wisp light down the fuller on both faces, and a
/// gem on each face of the guard.
fn sword_glow() -> Mesh {
    let mut parts = Vec::new();
    for side in [1.0, -1.0] {
        let points: Vec<Vec3> = (0..=6)
            .map(|i| 0.22 + 1.2 * i as f32 / 6.0)
            .map(|y| {
                // On the ridge of the blade at this height (between `BLADE`'s sections).
                let k = BLADE.windows(2).position(|w| y <= w[1].0).unwrap_or(0);
                let (a, b) = (BLADE[k], BLADE[k + 1]);
                let t = (y - a.0) / (b.0 - a.0);
                let (depth, z) = (a.1 + (b.1 - a.1) * t, a.3 + (b.3 - a.3) * t);
                Vec3::new(side * (depth + 0.003), y, z)
            })
            .collect();
        parts.push(sweep(&points, &[0.004, 0.0055, 0.0055, 0.0055, 0.0055, 0.005, 0.0], 6, Color::WHITE));
        let gem = gem_mesh(0.018).scaled_by(Vec3::new(0.6, 1.3, 1.0));
        parts.push(tinted(gem.translated_by(Vec3::new(side * 0.037, 0.1, 0.0)), Color::WHITE));
    }
    sculpted(parts)
}

/// A soft shell of light around the revenant's blade (`BLADE`, padded out), from the guard to a
/// little past the point.
fn blade_aura() -> Mesh {
    let mut sections: Vec<Section> = BLADE[..BLADE.len() - 1]
        .iter()
        .map(|&(y, depth, width, z)| Section::new(Vec3::new(0.0, y, z), Vec2::new(depth + 0.03, width * 2.0), Color::WHITE))
        .collect();
    sections.push(Section::new(Vec3::Y * (BLADE_LENGTH + 0.08), Vec2::ZERO, Color::WHITE));
    tube(&smoothed(&sections, 2), 12, (true, false))
}

/// A glint: a star of three long, thin crystals crossed through its middle, so it reads as a
/// sharp twinkle from any angle.
fn glint() -> Mesh {
    let ray = || gem_mesh(0.2).scaled_by(Vec3::new(0.07, 1.0, 0.07));
    let x = ray().rotated_by(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2));
    let z = ray().rotated_by(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2));
    sculpted(vec![tinted(ray(), Color::WHITE), tinted(x, Color::WHITE), tinted(z, Color::WHITE)])
}

/// The frost mage's body, under its moving cloth (`frost_mage_wardrobe`): a robe from the waist
/// up to a high collar, a blue sash over the skirt's top, and a heavy rime mantle over the
/// shoulders, its edge hanging in short icicles.
fn frost_mage_body() -> Mesh {
    let robe = tube(
        &[
            ring(0.01, 0.7, (0.18, 0.22), palette::FROST_ROBE),
            ring(0.01, 0.92, (0.19, 0.23), palette::FROST_ROBE),
            ring(0.01, 1.12, (0.19, 0.26), palette::FROST_ROBE).squared(2.4),
            ring(0.0, 1.32, (0.17, 0.27), palette::FROST_ROBE).squared(2.4),
            ring(-0.01, 1.42, (0.13, 0.2), palette::FROST_ROBE),
            ring(-0.01, 1.5, (0.09, 0.11), palette::FROST_DARK),
            ring(-0.01, 1.54, (0.08, 0.1), palette::FROST_DARK),
        ],
        28,
        (true, true),
    );
    let sash = belt(0.01, (0.82, 0.915), (0.208, 0.248), palette::FROST_BLUE);
    let mantle = tube(
        &[
            ring(-0.01, 1.53, (0.1, 0.13), palette::RIME),
            ring(-0.01, 1.47, (0.2, 0.28), palette::RIME),
            ring(-0.01, 1.38, (0.25, 0.36), palette::RIME),
            ring(-0.01, 1.3, (0.255, 0.37), palette::RIME).jagged(0.06),
        ],
        28,
        (true, true),
    );
    sculpted(vec![robe, sash, mantle])
}

/// Where the frost mage's skirt is at `a` (radians round from the front) and `v` (0..1) from the
/// sash down to its hem on the ground, flaring as it falls.
fn frost_skirt_point(a: f32, v: f32) -> Vec3 {
    let flare = v.powf(1.4);
    let (depth, width) = (0.192 + 0.16 * flare, 0.232 + 0.18 * flare);
    Vec3::new(a.cos() * depth, 0.88 - 0.85 * v, a.sin() * width)
}

/// The frost mage's moving cloth: the robe's skirt from the sash to the ground in deepening
/// folds, a blue panel down its front, a blue band above its rimed hem and a ring of glowing runes
/// just above that; and a long, narrow cape from under the mantle, rimed along its hem.
fn frost_mage_wardrobe() -> Wardrobe {
    const FOLDS: u32 = 11;
    let skirt_sections: Vec<Section> = [
        (0.0, palette::FROST_ROBE, 0.0),
        (0.45, palette::FROST_ROBE, 0.025),
        (0.9, palette::FROST_BLUE, 0.045),
        (0.95, palette::RIME, 0.05),
        (1.0, palette::RIME, 0.05),
    ]
    .iter()
    .map(|&(v, color, folds)| {
        let half = Vec2::new(frost_skirt_point(0.0, v).x, frost_skirt_point(std::f32::consts::FRAC_PI_2, v).z);
        Section::new(Vec3::Y * frost_skirt_point(0.0, v).y, half, color).folded(folds, FOLDS)
    })
    .collect();
    let skirt = tube(&smoothed(&skirt_sections, 3), 56, (true, true));
    // The panel down the front, just over the skirt.
    let panel = tube(
        &[0.04, 0.45, 0.88]
            .map(|v| Section::new(frost_skirt_point(0.0, v) + Vec3::X * 0.012, Vec2::new(0.008, 0.05 - 0.016 * (1.0 - v)), palette::FROST_BLUE)),
        8,
        (true, true),
    );
    // Runes: small upright diamonds just above the hem, following the skirt's curve.
    let runes = sculpted(
        (0..10)
            .map(|i| {
                let around = (i as f32 + 0.5) / 10.0 * std::f32::consts::TAU;
                let rune = gem_mesh(0.03).scaled_by(Vec3::new(0.35, 1.5, 1.0)).rotated_by(Quat::from_rotation_y(-around));
                tinted(rune.translated_by(frost_skirt_point(around, 0.83) * Vec3::new(1.03, 1.0, 1.03)), Color::WHITE)
            })
            .collect(),
    );
    let skirt_chains: Vec<Vec<Vec3>> = (0..12)
        .map(|c| c as f32 / 12.0 * std::f32::consts::TAU)
        .map(|a| [0.04, 0.35, 0.68, 1.0].map(|v| frost_skirt_point(a, v)).to_vec())
        .collect();
    let skirt = Drape::new(
        sculpted(vec![skirt, panel]),
        Some(runes),
        skirt_chains,
        Weave { hold: (8.0, 2.2), damping: 6.0, air: 0.3, inertia: 0.3, gravity: 7.0, flutter: 0.35, joined: true, wisps: 4.0 },
    );

    // The cape: from under the back of the mantle, falling in soft waves and widening a little to
    // a rimed hem just off the ground.
    const ROWS: usize = 16;
    const COLS: usize = 16;
    let cape_point = |u: f32, v: f32, ragged: bool| {
        let half = 0.13 + 0.08 * v;
        let z = (u * 2.0 - 1.0) * half;
        let mut x = -0.24 - 0.1 * v + 1.5 * (1.0 - 0.6 * v) * z * z;
        if ragged {
            x += 0.03 * v * (z * 32.0 + v * 2.0).sin();
        }
        Vec3::new(x, 1.36 - 1.28 * v, z)
    };
    let cape = Drape::new(
        sheet(&grid(ROWS, COLS, (0.0, 1.0), |u, v| cape_point(u, v, true)), &hemmed(ROWS, 2, palette::FROST_DARK, palette::RIME), 0.012),
        None,
        [0.1, 0.5, 0.9].map(|u| [0.02, 0.27, 0.51, 0.75, 1.0].map(|v| cape_point(u, v, false)).to_vec()).to_vec(),
        Weave { hold: (5.0, 1.3), damping: 6.0, air: 0.4, inertia: 0.4, gravity: 6.0, flutter: 0.6, joined: true, wisps: 0.0 },
    );

    Wardrobe {
        drapes: vec![skirt, cape],
        trim: palette::FROST_GLOW,
        body: vec![
            Column { center: Vec2::ZERO, half: Vec2::new(0.175, 0.215), from: 0.3, to: 1.0 },
            Column { center: Vec2::new(-0.01, 0.0), half: Vec2::new(0.21, 0.28), from: 1.0, to: 1.3 },
        ],
        leg_radius: 0.12,
    }
}

/// The frost mage's head: a deep hood rimed at the rim, empty but for the eyes, with a crown of
/// ice shards rising around it (so it reads from above).
fn frost_crowned_hood() -> Mesh {
    let cloth = palette::FROST_ROBE;
    let top = [
        ring(-0.05, 0.22, (0.19, 0.19), cloth),
        ring(-0.08, 0.36, (0.15, 0.14), cloth),
        ring(-0.13, 0.48, (0.08, 0.07), cloth),
        ring(-0.2, 0.56, (0.025, 0.022), cloth),
        ring(-0.25, 0.58, (0.0, 0.0), cloth),
    ];
    let mut parts = deep_hood(cloth, palette::RIME, palette::FROST_DARK, &top);
    // Shards fanning up and out from the crown, tallest at the front.
    for i in 0..7 {
        let around = i as f32 / 7.0 * std::f32::consts::TAU;
        let height = 0.22 + 0.1 * (around.cos() * 0.5 + 0.5);
        let shard = ice_shard(0.035, height).rotated_by(Quat::from_rotation_z(-0.45));
        let turned = shard.rotated_by(Quat::from_rotation_y(around));
        parts.push(tinted(turned.translated_by(Vec3::new(-0.06, 0.32, 0.0)), palette::ICE));
    }
    sculpted(parts)
}

/// Wide bell sleeves, banded in blue and rimed at the cuff (dark inside), over a slim dark glove.
fn frost_mage_arm() -> Mesh {
    let sleeve = tube(
        &[
            ring(0.0, 0.08, (0.085, 0.085), palette::FROST_ROBE),
            ring(0.0, -0.1, (0.09, 0.09), palette::FROST_ROBE),
            ring(0.0, -0.22, (0.1, 0.1), palette::FROST_ROBE),
            ring(-0.01, -0.36, (0.148, 0.148), palette::FROST_ROBE),
            ring(-0.01, -0.385, (0.15, 0.15), palette::FROST_BLUE),
            ring(-0.01, -0.41, (0.152, 0.152), palette::FROST_ROBE),
            ring(-0.01, -0.43, (0.155, 0.155), palette::RIME),
            ring(-0.01, -0.47, (0.158, 0.158), palette::FROST_DARK),
        ],
        18,
        (true, true),
    );
    sculpted(vec![sleeve, fist(palette::DARK_LEATHER, -0.36, 0.056)])
}

/// The frost mage's staff, gripped at the origin, standing up (+Y): a dark shaft swelling a little
/// toward the head, bound in blue and shod in a steel spike, and at the top two steel crescents
/// curving up like an open cage around its crystals (`staff_crystals`, drawn glowing).
fn ice_staff() -> Mesh {
    let wood = palette::BARK.darker(0.15);
    let mut parts = vec![
        sweep(&[Vec3::Y * -0.62, Vec3::Y * 0.2, Vec3::Y * 1.04], &[0.022, 0.024, 0.027], 12, wood),
        // The steel butt spike and its collar.
        sweep(&[Vec3::Y * -0.6, Vec3::Y * -0.66, Vec3::Y * -0.8], &[0.034, 0.03, 0.0], 12, palette::STEEL),
        // Blue bindings either side of the grip and up the shaft.
        band(0.09, 0.15, 0.031, 12, palette::FROST_BLUE),
        band(-0.15, -0.09, 0.03, 12, palette::FROST_BLUE),
        band(0.585, 0.615, 0.032, 12, palette::FROST_BLUE),
        band(0.665, 0.695, 0.032, 12, palette::FROST_BLUE),
        // The collar holding the head.
        sweep(&[Vec3::Y * 0.96, Vec3::Y * 1.02, Vec3::Y * 1.07], &[0.03, 0.04, 0.052], 12, palette::STEEL),
        sweep(&[Vec3::Y * 1.07, Vec3::Y * 1.095], &[0.054, 0.05], 12, palette::FROST_BLUE),
    ];
    for side in CRESCENTS {
        let points: Vec<Vec3> = (0..=6).map(|i| crescent(side, i as f32 / 6.0)).collect();
        let radii: Vec<f32> = (0..=6).map(|i| 0.017 - 0.007 * i as f32 / 6.0).collect();
        parts.push(sweep(&points, &radii, 10, palette::STEEL));
        parts.push(tinted(ball(0.02).translated_by(crescent(side, 1.0)), palette::STEEL));
    }
    sculpted(parts)
}

/// The javelinist's body, under its moving cloth (`javelinist_wardrobe`): a slim, fitted tunic
/// from an ashen undertunic showing at the knee up to a high collar, a wide leather belt with a
/// silver buckle, and a mantle draped over the shoulders, its torn edge hanging longer behind.
/// Sloping shoulders, no bulk. Quiet details: the coat and the spear overhead are
/// the figure.
fn javelinist_body() -> Mesh {
    const FOLDS: u32 = 9;
    let tunic = tube(
        &smoothed(
            &[
                ring(0.0, 0.42, (0.15, 0.185), palette::HUNTER_ASH).torn(0.05).folded(0.05, FOLDS),
                ring(0.0, 0.6, (0.145, 0.18), palette::HUNTER_ASH).folded(0.03, FOLDS),
                ring(0.0, 0.84, (0.145, 0.18), palette::HUNTER),
                ring(0.01, 0.98, (0.15, 0.19), palette::HUNTER),
                ring(0.015, 1.13, (0.155, 0.215), palette::HUNTER).squared(2.2),
                ring(0.0, 1.3, (0.14, 0.235), palette::HUNTER).squared(2.2),
                ring(-0.01, 1.41, (0.11, 0.18), palette::HUNTER),
                ring(-0.01, 1.49, (0.075, 0.1), palette::HUNTER_DARK),
                ring(-0.01, 1.57, (0.07, 0.095), palette::HUNTER_DARK),
            ],
            3,
        ),
        40,
        (true, true),
    );
    // The belt goes over the coat's top (the coat hangs from under it).
    let belt = belt(0.01, (0.855, 0.95), (0.183, 0.223), palette::LEATHER);
    let buckle = bevel_box(Vec3::new(0.03, 0.075, 0.07), 0.01, palette::SILVER).translated_by(Vec3::new(0.2, 0.902, 0.0));
    let mantle = tube(
        &smoothed(
            &[
                ring(-0.01, 1.58, (0.085, 0.11), palette::HUNTER),
                ring(-0.01, 1.52, (0.15, 0.21), palette::HUNTER),
                ring(-0.015, 1.43, (0.2, 0.32), palette::HUNTER).folded(0.03, 13),
                ring(-0.02, 1.3, (0.22, 0.375), palette::HUNTER).folded(0.05, 13),
                ring(-0.02, 1.17, (0.225, 0.385), palette::HUNTER_DARK).folded(0.065, 13).torn(0.1).trailing(0.12),
            ],
            3,
        ),
        48,
        (true, true),
    );
    sculpted(vec![tunic, belt, buckle, mantle])
}

/// Where the coat's skirt is, at `u` (0..1) round it from its right front edge, round the back, to
/// its left front edge, and `v` (0..1) from the belt down to the hem; `ragged` for the cloth
/// itself (folds, unevenness and the torn hem), not for the chains it hangs from.
fn coat_point(u: f32, v: f32, ragged: bool) -> Vec3 {
    // Open at the front, this far (radians) either side of it.
    const OPEN: f32 = 0.3;
    let a = OPEN + u * (std::f32::consts::TAU - 2.0 * OPEN);
    let flare = v.powf(1.25);
    let (depth, width) = (0.172 + 0.17 * flare, 0.212 + 0.17 * flare);
    // The hem: just off the ground behind, a little higher at the front.
    let hem = 0.035 + 0.075 * (1.0 + a.cos()) / 2.0;
    let mut y = 0.925 + (hem - 0.925) * v;
    let mut out = 1.0;
    if ragged {
        out += v * 0.075 * (a * 8.0 + v * 1.8).sin() * (0.6 + 0.4 * (a * 3.1).sin()) + 0.025 * (a * 5.0 + v * 7.0).sin();
        let scallop = 0.5 + 0.5 * (a * 5.0 + 0.8).sin();
        let strip = (a * 13.0 + 2.0).sin().max(0.0).powi(6);
        y -= 0.1 * (0.35 * scallop + 0.65 * strip) * v.powi(8);
    }
    Vec3::new(a.cos() * depth * out, y, a.sin() * width * out)
}

/// The javelinist's moving cloth: a long coat skirt open at the front, its torn hem glowing with
/// spirit light, and a tabard hanging from the belt between its edges.
fn javelinist_wardrobe() -> Wardrobe {
    const ROWS: usize = 18;
    const COLS: usize = 64;
    let point = |u, v| coat_point(u, v, true);
    let coat = sheet(&grid(ROWS, COLS, (0.0, 1.0), point), &hemmed(ROWS, 3, palette::HUNTER, palette::HUNTER_DARK), 0.01);
    // The glowing hem, wrapped round the cloth's last few centimeters.
    let trim = sheet(&grid(2, COLS, (0.975, 1.0), point), &[Color::WHITE], 0.02);
    let coat_chains: Vec<Vec<Vec3>> = (0..=8).map(|c| [0.07, 0.3, 0.53, 0.77, 1.0].map(|v| coat_point(c as f32 / 8.0, v, false)).to_vec()).collect();
    let coat = Drape::new(
        coat,
        Some(trim),
        coat_chains,
        Weave { hold: (6.0, 1.5), damping: 6.0, air: 0.3, inertia: 0.35, gravity: 7.0, flutter: 0.5, joined: true, wisps: 5.0 },
    );

    // The tabard: down the front, curving round the legs, its end torn; leather where it hangs
    // from the belt, darker at its end.
    let mut colors = hemmed(12, 2, palette::HUNTER, palette::HUNTER_DARK);
    colors[0] = palette::LEATHER;
    let tabard = tabard(Vec3::new(0.19, 0.9, 0.0), Vec3::new(0.042, -0.62, 0.0), (0.072, 0.064), 1.2, (0.04, 2.1), &colors);

    Wardrobe {
        drapes: vec![coat, tabard],
        trim: palette::SOUL,
        body: vec![
            Column { center: Vec2::ZERO, half: Vec2::new(0.165, 0.205), from: 0.3, to: 1.05 },
        ],
        leg_radius: 0.125,
    }
}

/// Long bell sleeves, rounded over the shoulder and falling in deepening folds to a torn cuff that
/// hangs longer underneath, over a dark-gloved hand.
fn javelinist_arm() -> Mesh {
    let sleeve = tube(
        &smoothed(
            &[
                ring(-0.01, 0.06, (0.0, 0.0), palette::HUNTER),
                ring(-0.01, 0.05, (0.045, 0.05), palette::HUNTER),
                ring(0.0, 0.0, (0.068, 0.07), palette::HUNTER),
                ring(0.0, -0.12, (0.08, 0.082), palette::HUNTER).folded(0.02, 7),
                ring(0.0, -0.28, (0.11, 0.11), palette::HUNTER).folded(0.05, 7),
                ring(-0.01, -0.4, (0.138, 0.138), palette::HUNTER).folded(0.07, 7),
                ring(-0.015, -0.44, (0.148, 0.148), palette::HUNTER_DARK).folded(0.07, 7),
                ring(-0.02, -0.47, (0.152, 0.152), palette::HUNTER_DARK).folded(0.07, 7).torn(0.09).trailing(0.1),
            ],
            3,
        ),
        32,
        (true, true),
    );
    sculpted(vec![sleeve, fist(palette::DARK_LEATHER, -0.34, 0.05)])
}

/// Baggy trousers gathered into soft, wrapped boots: seen at the coat's opening as it walks.
fn javelinist_leg() -> Mesh {
    booted_leg(palette::HUNTER_DARK, palette::DARK_LEATHER, 0.1, 0.095)
}

/// How much smaller the javelinist's hood is than a `deep_hood` (its width, and its height).
const JAVELINIST_HOOD: Vec2 = Vec2::new(0.86, 0.95);

/// A narrow hood whose crown rises to a low peak that hangs forward over the brow, soft folds down
/// its sides; inside it only darkness and the eyes.
fn javelinist_hood() -> Mesh {
    let cloth = palette::HUNTER;
    let top = [
        ring(-0.045, 0.22, (0.2, 0.19), cloth).folded(0.04, 5),
        ring(-0.045, 0.35, (0.17, 0.155), cloth).folded(0.035, 5),
        ring(-0.02, 0.45, (0.125, 0.105), cloth).folded(0.02, 5),
        ring(0.04, 0.52, (0.075, 0.06), cloth),
        ring(0.11, 0.55, (0.035, 0.028), cloth),
        ring(0.165, 0.53, (0.0, 0.0), cloth),
    ];
    let scale = Vec3::new(JAVELINIST_HOOD.x, JAVELINIST_HOOD.y, JAVELINIST_HOOD.x);
    sculpted(deep_hood(cloth, palette::HUNTER_DARK, palette::HUNTER_DARK.darker(0.05), &top)).scaled_by(scale)
}

/// A fighter's low-poly figure, feet at the origin, picked by class id so each class has its own
/// silhouette from above. Every class needs one (see `FIGHTER_LOOKS`).
pub fn fighter_mesh(class_key: &str) -> Mesh {
    // Drawn in a white material: these are the real colors, the same for every player (ground
    // rings and health bars tell who is who).
    match class_key {
        // Marksman: a tall, faceless hunter in one long near-black moss robe, a draped cowl and a
        // cord belt. Head, arms, legs and spear are separate, animated parts (`fighter_rig`).
        "javelinist" => javelinist_body(),
        // Duelist: a spectral revenant: one long, cold near-black robe from the collar to a hem
        // fraying into tatters below the knee, belted in ash with a steel buckle, under heavy ash
        // pauldrons. Hood, arms, legs, cape and sword are separate, animated parts
        // (`fighter_rig`), built to overlap the robe so no joint shows.
        "revenant" => revenant_body(),
        // Controller: a frost mage in a long, deep navy robe flaring to the ground, rimed at
        // the hem, with a blue sash, front panel and band above the hem, and a heavy rime mantle
        // over the shoulders (the ice shards growing out of it glow: `frost_mage_glow`). Head (an
        // ice-crowned hood), arms, legs, cape and staff are separate, animated parts
        // (`fighter_rig`).
        "frost_mage" => frost_mage_body(),
        _ => unreachable!("no figure for class {class_key:?} (add it to FIGHTER_LOOKS)"),
    }
}

/// A flat strip along world +X from `from` to `to`, `width` wide: the lane a shot will fly down.
pub fn lane_mesh(from: f32, to: f32, width: f32) -> Mesh {
    let mut b = FlatMesh::default();
    let side = Vec3::Z * width / 2.0;
    let (near, far) = (Vec3::X * from, Vec3::X * to);
    b.quad([near + side, far + side, far - side, near - side], Color::WHITE);
    b.build()
}

/// How a class's shot looks (see `SHOT_LOOKS`). Never in its owner's colors: a shot looks the
/// same whoever throws it.
#[derive(Clone)]
pub struct ShotLook<M = Mesh> {
    pub mesh: M,
    pub glow: ShotGlow,
    /// A thrown spear: leaves the thrower's hand where the javelin is held (gripped `GRIP_TO_TIP`
    /// behind its point) and trails wind (`wind_mesh`).
    pub thrown: bool,
    /// A see-through glow around it and trailing behind, faded by its vertex alpha (`frost_aura`).
    pub aura: Option<M>,
    /// How fast (radians per second) it turns about its flight.
    pub spin: f32,
}

/// What a shot's mesh is drawn in.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ShotGlow {
    /// Its own colors (a real weapon, drawn plain), with a white spark on its point.
    Weapon,
    /// Spectral blue: an ability.
    Spirit,
    /// Glowing ice, its colors in the mesh.
    Ice,
    /// A plain pale glow.
    Pale,
}

/// How far the point of a held javelin is ahead of the hand.
/// Turns a mesh pointing up (+Y) to point along +X: how a shot flies.
const ALONG_X: Quat = Quat::from_xyzw(0.0, 0.0, -std::f32::consts::FRAC_1_SQRT_2, std::f32::consts::FRAC_1_SQRT_2);
pub const GRIP_TO_TIP: f32 = 1.2;

/// A class's shot (auto-attack or, `ability`, Q), flying along world +X, the point that hits at
/// the origin, picked by class id: the javelinist throws the very javelin it carries (and its Q
/// is a plain spectral spear, drawn glowing); the frost mage a frostbolt; the rest round bolts of
/// the shot's radius.
pub fn shot_look(class: &ClassDef, shot: Shot, ability: bool) -> ShotLook {
    let plain = |mesh, glow| ShotLook { mesh, glow, thrown: false, aura: None, spin: 0.0 };
    match (class.id.as_str(), ability) {
        ("javelinist", false) => ShotLook {
            thrown: true,
            ..plain(carried_javelin().rotated_by(ALONG_X), ShotGlow::Weapon)
        },
        ("javelinist", true) => {
            ShotLook { thrown: true, ..plain(javelin(Color::WHITE, Color::WHITE, Color::WHITE).rotated_by(ALONG_X), ShotGlow::Spirit) }
        }
        ("frost_mage", false) => ShotLook {
            aura: Some(frost_aura(shot.radius)),
            spin: FROSTBOLT_SPIN,
            ..plain(frostbolt(shot.radius), ShotGlow::Ice)
        },
        _ => plain(gem_mesh(shot.radius), ShotGlow::Pale),
    }
}

/// A frostbolt's crystal is this many times as long as the shot's radius, and turns this fast
/// (radians per second) as it flies.
const FROSTBOLT_LENGTH: f32 = 3.6;
const FROSTBOLT_SPIN: f32 = 5.0;

/// A frostbolt, point first along +X (its point at the origin), sized by the shot's radius: a long
/// six-sided ice crystal with a ring of smaller shards splaying back from its waist like a
/// frozen burst, the core pale ice, the shards a deeper frost glow.
fn frostbolt(radius: f32) -> Mesh {
    let length = radius * FROSTBOLT_LENGTH;
    let point_up = |mesh: Mesh| mesh.rotated_by(ALONG_X);
    let core = tinted(ice_shard(radius * 0.4, length).translated_by(Vec3::Y * -length), palette::ICE);
    // A shard standing from its base `back` behind the point, leaning back and `out` (radians)
    // from the axis, turned `around` it.
    let splay = |shard: Mesh, back: f32, out: f32, around: f32| {
        shard
            .rotated_by(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2 + out))
            .translated_by(Vec3::new(-back, -radius * 0.1, 0.0))
            .rotated_by(Quat::from_rotation_x(around))
    };
    let mut parts = vec![point_up(core)];
    // Five shards round the waist, alternating long and short.
    for i in 0..5 {
        let long = if i % 2 == 0 { 1.0 } else { 0.7 };
        let shard = tinted(ice_shard(radius * 0.17, length * 0.42 * long), palette::FROST_GLOW);
        parts.push(splay(shard, length * 0.35, 0.5, i as f32 * std::f32::consts::TAU / 5.0 + 0.3));
    }
    // Two slivers riding just behind the point.
    for around in [std::f32::consts::FRAC_PI_2, -std::f32::consts::FRAC_PI_2] {
        parts.push(splay(tinted(ice_shard(radius * 0.1, length * 0.3), palette::ICE), length * 0.12, 0.25, around));
    }
    sculpted(parts)
}

/// The cold glow around a frostbolt: a faceted halo round the crystal, stretched back into a
/// trail about as long again, fading out toward its end (vertex alpha).
fn frost_aura(radius: f32) -> Mesh {
    let length = radius * FROSTBOLT_LENGTH;
    let halo = Sphere::new(1.0).mesh().ico(1).unwrap();
    let halo = halo.scaled_by(Vec3::new(length * 0.55, radius, radius)).translated_by(Vec3::X * -length * 0.45);
    let trail = cone(radius * 0.8, length * 1.4, 8).rotated_by(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2));
    let trail = trail.translated_by(Vec3::X * -length * 1.3);
    let mut aura = merge_parts(vec![halo, trail]);
    let fade = |x: f32| (1.0 + x / (length * 2.0)).clamp(0.0, 1.0).powf(1.5);
    let Some(bevy::mesh::VertexAttributeValues::Float32x3(positions)) = aura.attribute(Mesh::ATTRIBUTE_POSITION) else {
        unreachable!("meshes have positions")
    };
    let colors: Vec<[f32; 4]> = positions.iter().map(|p| [1.0, 1.0, 1.0, fade(p[0])]).collect();
    aura.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    aura
}

/// The white glow on a thrown spear's point: a small bright spark stretched along +X, at the
/// origin.
pub fn shot_tip_mesh() -> Mesh {
    ball(0.04).scaled_by(Vec3::new(3.0, 1.0, 1.0))
}

/// The wind behind a thrown spear: one thin, straight streak, 1 m long along -X from the origin
/// (stretched to length as it flies), two crossed ribbons so it shows from any angle. It keeps
/// nearly its width all the way and fades out toward the back.
pub fn wind_mesh() -> Mesh {
    let mut b = FlatMesh::default();
    // Stations down the streak: how far along it (0..1), its width and opacity there.
    let stations = [(0.0, 1.0, 0.2), (0.12, 1.0, 1.0), (0.6, 0.8, 0.45), (1.0, 0.5, 0.0)];
    for side in [Vec3::Z, Vec3::Y] {
        let at = |(t, w, a): (f32, f32, f32)| (Vec3::X * -t, side * 0.022 * w, a);
        for pair in stations.windows(2) {
            let ((near, near_half, near_alpha), (far, far_half, far_alpha)) = (at(pair[0]), at(pair[1]));
            b.tri_faded([near - near_half, far - far_half, near + near_half], [near_alpha, far_alpha, near_alpha]);
            b.tri_faded([near + near_half, far - far_half, far + far_half], [near_alpha, far_alpha, far_alpha]);
            // Both faces: it's seen from either side.
            b.tri_faded([near + near_half, far - far_half, near - near_half], [near_alpha, far_alpha, near_alpha]);
            b.tri_faded([near + near_half, far + far_half, far - far_half], [near_alpha, far_alpha, far_alpha]);
        }
    }
    b.build()
}

/// A small faceted ball: round shots, flower heads.
pub fn gem_mesh(radius: f32) -> Mesh {
    faceted(Sphere::new(radius).mesh().ico(0).unwrap())
}

/// A tile's color on the minimap: the palette of the scene, flattened.
pub fn tile_color(theme: &Theme, tile: Tile) -> Color {
    match tile {
        Tile::Grass => theme.grass,
        Tile::Path => theme.path,
        Tile::Marsh => theme.marsh,
        Tile::Ford => theme.riverbed.mix(&theme.pond, 0.6),
        Tile::Bridge => theme.stone,
        Tile::Planks => theme.bark.mix(&theme.path, 0.3),
        Tile::Water => theme.pond,
        Tile::Chasm => theme.abyss.mix(&INK, 0.3),
        Tile::Wall => theme.wall,
        Tile::Rock => theme.slate,
        Tile::Tree => theme.pine,
        Tile::Forest => match theme.wilds {
            Wilds::Forest => theme.pine.mix(&INK, 0.4),
            Wilds::Fen => theme.pond.mix(&INK, 0.45),
            Wilds::Highlands => theme.wall.mix(&INK, 0.35),
        },
    }
}

/// Haze that fades the forest ring into the sky color; goes on the camera, and takes the map's
/// color in `light_scene`.
pub fn haze() -> DistanceFog {
    DistanceFog { color: HAZE, falloff: FogFalloff::Linear { start: 40.0, end: 95.0 }, ..default() }
}

pub struct ArenaPlugin;

impl Plugin for ArenaPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(HAZE));
        app.insert_resource(GlobalAmbientLight { color: SKY, brightness: 160.0, ..default() });
        app.add_systems(Startup, spawn_sun);
        // The scene is built from our room's map, and built again when it changes.
        app.add_systems(Update, ((build_arena, light_scene).run_if(resource_exists_and_changed::<CurrentMap>), flicker_torches));
    }
}

const WATER_LEVEL: f32 = -0.3;
const RIVERBED_LEVEL: f32 = -0.65;
/// The bottom of a chasm.
const CHASM_FLOOR: f32 = -6.0;

/// Deterministic, so the arena looks the same for every player and every run.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next()
    }
}

/// Collects flat-colored triangles into one mesh.
#[derive(Default)]
pub(crate) struct FlatMesh {
    positions: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
}

impl FlatMesh {
    /// One triangle, counter-clockwise seen from its front.
    fn tri(&mut self, corners: [Vec3; 3], color: Color) {
        let c = color.to_linear();
        for p in corners {
            self.positions.push(p.to_array());
            self.colors.push([c.red, c.green, c.blue, 1.0]);
        }
    }

    /// A white triangle whose opacity at each corner is `alphas` (for see-through materials).
    pub(crate) fn tri_faded(&mut self, corners: [Vec3; 3], alphas: [f32; 3]) {
        for (p, alpha) in corners.into_iter().zip(alphas) {
            self.positions.push(p.to_array());
            self.colors.push([1.0, 1.0, 1.0, alpha]);
        }
    }

    /// `tri_faded`, facing both ways (seen from either side).
    pub(crate) fn tri_faded_both(&mut self, [a, b, c]: [Vec3; 3], [x, y, z]: [f32; 3]) {
        self.tri_faded([a, b, c], [x, y, z]);
        self.tri_faded([a, c, b], [x, z, y]);
    }

    /// A quad from four corners in order (counter-clockwise seen from its front).
    fn quad(&mut self, [a, b, c, d]: [Vec3; 4], color: Color) {
        self.tri([a, b, c], color);
        self.tri([a, c, d], color);
    }

    pub(crate) fn build(self) -> Mesh {
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colors)
            .with_computed_flat_normals()
    }
}

/// The lobby's dais, on top of its crag.
pub const DAIS_RADIUS: f32 = 1.3;
pub const DAIS_HEIGHT: f32 = 0.16;

/// The crag the lobby's stage stands on, `height` meters over the land, in `theme`'s colors: a
/// pinnacle with the dais on its narrow grassy top (the top level at the origin), tufts and a
/// flower or two around it, and under it a core of rock widening down to the ground, its sides
/// broken up by boulders darkening as they go down. Narrow, so it hides little of the land
/// below and beyond. Vertex-colored, for one matte white material.
pub fn crag_mesh(theme: &'static Theme, height: f32) -> Mesh {
    let mut rng = Lcg(11);
    let mut props = Props::new(theme);
    let dais = faceted(Cylinder::new(DAIS_RADIUS, DAIS_HEIGHT).mesh().resolution(24).build());
    props.push(dais.transformed_by(Transform::from_xyz(0.0, DAIS_HEIGHT / 2.0, 0.0)), theme.stone.mix(&theme.wall, 0.4));
    let top = faceted(Cylinder::new(CRAG_TOP, 0.5).mesh().resolution(9).build());
    props.push(top.transformed_by(Transform::from_xyz(0.0, -0.25, 0.0)), theme.grass.mix(&theme.meadow, 0.3));
    let depth = height + 4.0;
    let core = faceted(ConicalFrustum { radius_top: CRAG_TOP - 0.1, radius_bottom: 4.5, height: depth }.mesh().resolution(8).build());
    props.push(core.transformed_by(Transform::from_xyz(0.0, -depth / 2.0 - 0.3, 0.0)), theme.slate.darker(0.12));
    let mut down = 0.9;
    while down < depth {
        let count = 7;
        for k in 0..count {
            let a = (k as f32 + rng.range(-0.3, 0.3)) / count as f32 * std::f32::consts::TAU + down;
            let at = Vec2::from_angle(a) * (1.7 + down * 0.15);
            let s = rng.range(1.0, 1.4) + down * 0.06;
            let color = theme.slate.mix(&theme.wall, rng.next()).darker(0.25 * down / depth);
            let t = Transform::from_translation(to_world(at, -down))
                .with_rotation(Quat::from_euler(EulerRot::XYZ, rng.range(0.0, 3.0), rng.range(0.0, 3.0), 0.0))
                .with_scale(Vec3::new(s, s * rng.range(0.7, 1.0), s));
            props.add(Shape::Rock, color, t);
        }
        down += 1.3;
    }
    for k in 0..7 {
        let a = k as f32 / 7.0 * std::f32::consts::TAU + rng.range(-0.3, 0.3);
        let at = Vec2::from_angle(a) * rng.range(DAIS_RADIUS + 0.2, CRAG_TOP - 0.15);
        if k % 3 == 0 {
            props.flower(&mut rng, at);
        } else {
            props.bog_reeds(&mut rng, at, theme.fern, theme.path.mix(&theme.fern, 0.5));
        }
    }
    props.into_mesh()
}

/// The crag's top's radius: a little past the dais.
const CRAG_TOP: f32 = DAIS_RADIUS + 0.6;

/// A torch flame; flickers its light.
#[derive(Component)]
struct Torch {
    phase: f32,
}

const TORCH_INTENSITY: f32 = 170_000.0;

fn flicker_torches(time: Res<Time>, mut torches: Query<(&Torch, &mut PointLight, &Children)>, mut flames: Query<&mut Transform>) {
    let t = time.elapsed_secs();
    for (torch, mut light, children) in &mut torches {
        let f = 1.0 + 0.12 * (t * 7.3 + torch.phase).sin() * (t * 12.1 + torch.phase * 1.7).sin();
        light.intensity = TORCH_INTENSITY * f;
        for child in children {
            if let Ok(mut flame) = flames.get_mut(*child) {
                flame.scale = Vec3::new(1.0, f * 1.15, 1.0);
            }
        }
    }
}

/// The low sun, from the same side over every map (its color and strength are the map's).
fn spawn_sun(mut commands: Commands) {
    commands.spawn((
        DirectionalLight { color: SUN, illuminance: 3_600.0, shadow_maps_enabled: true, ..default() },
        // Low dusk sun from the upper left, so shadows are long and fall toward the camera's right.
        Transform::from_xyz(-32.0, 14.0, -18.0).looking_at(Vec3::ZERO, Vec3::Y),
        sun_shadows(SUN_SHADOWS),
    ));
}

/// How far from the camera the sun casts shadows in the arena: enough to cover what it sees at
/// full zoom-out; the far forest needs none.
pub const SUN_SHADOWS: f32 = 50.0;

/// The sun's shadows, out to `reach` meters from the camera.
pub fn sun_shadows(reach: f32) -> CascadeShadowConfig {
    CascadeShadowConfigBuilder { num_cascades: 1, maximum_distance: reach, ..default() }.build()
}

/// The map's light and air: the sun's color and strength, the sky's fill, and the haze (and the
/// clear color behind it, the same).
fn light_scene(
    current: Res<CurrentMap>,
    mut clear: ResMut<ClearColor>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut suns: Query<&mut DirectionalLight>,
    mut fogs: Query<&mut DistanceFog>,
) {
    let theme = Theme::of(current.0);
    clear.0 = theme.haze;
    ambient.color = theme.sky;
    ambient.brightness = theme.sky_brightness;
    for mut sun in &mut suns {
        sun.color = theme.sun;
        sun.illuminance = theme.sun_illuminance;
    }
    for mut fog in &mut fogs {
        fog.color = theme.haze;
    }
}

/// Everything built from the map: despawned when it changes, for the new map's.
#[derive(Component)]
struct ArenaScene;

fn build_arena(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    current: Res<CurrentMap>,
    old: Query<Entity, With<ArenaScene>>,
) {
    for entity in &old {
        commands.entity(entity).despawn();
    }
    let mut rng = Lcg(7);
    let m = current.map();
    let theme = Theme::of(current.0);

    let scene = SceneTiles::new(m);
    let vertex_colored = materials.add(matte(Color::WHITE));
    commands.spawn((ArenaScene, Mesh3d(meshes.add(terrain_mesh(m, &scene, theme, &mut rng))), MeshMaterial3d(vertex_colored.clone())));
    commands.spawn((ArenaScene, Mesh3d(meshes.add(ground_mesh(m, theme, &mut rng))), MeshMaterial3d(vertex_colored.clone())));
    if let Some(water) = water_mesh(&scene, theme.wilds == Wilds::Fen) {
        let material = StandardMaterial { perceptual_roughness: 0.35, reflectance: 0.5, ..matte(theme.pond) };
        commands.spawn((ArenaScene, Mesh3d(meshes.add(water)), MeshMaterial3d(materials.add(material))));
    }

    let mut props = Props::new(theme);
    let wall_tops = props.walls(m, &mut rng);
    props.bridges(m, &mut rng);
    props.rope_bridges(m, &mut rng);
    props.fords(m, &mut rng);
    if theme.wilds == Wilds::Fen {
        props.flagstones(m, &mut rng);
    }
    for (t, tile) in m.tiles() {
        let c = Map::center(t);
        match tile {
            Tile::Rock if theme.wilds == Wilds::Highlands => props.sandstone(&mut rng, c),
            Tile::Rock => props.boulder(&mut rng, c),
            Tile::Grass if theme.wilds == Wilds::Fen => props.fen_grass(&mut rng, c),
            Tile::Tree => props.map_tree(&mut rng, c),
            Tile::Grass if rng.next() < 0.02 => props.flower(&mut rng, c),
            Tile::Marsh => props.bog(&mut rng, c),
            _ => {}
        }
    }
    // The wilds around the clearing, kept out of water and chasms.
    let extent = scene.extent.as_vec2() - Vec2::splat(4.0);
    let open = |p: Vec2| [Vec2::ZERO, Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y].iter().all(|d| scene.get(Map::tile_of(p + *d * 1.5)) == Tile::Forest);
    let density = match theme.wilds {
        Wilds::Forest => 0.035,
        Wilds::Fen => 0.04,
        Wilds::Highlands => 0.02,
    };
    for _ in 0..(extent.x * extent.y * 4.0 * density) as usize {
        let p = Vec2::new(rng.range(-extent.x, extent.x), rng.range(-extent.y, extent.y));
        if m.edge_margin(p) > -1.2 || !open(p) {
            continue;
        }
        props.wild(&mut rng, p, wild_height(theme, m, p));
    }

    // Torches (posts go into the prop mesh; flames and lights don't).
    let flame = meshes.add(faceted(Cone::new(0.16, 0.42).mesh().resolution(5).build()));
    let flame_mat = materials.add(glow(TORCH_FLAME, 8.0));
    for (i, &p) in m.torches.iter().enumerate() {
        let base = wall_tops.get(&Map::tile_of(p)).copied().unwrap_or(0.0);
        let post_height = if base > 0.0 { 0.5 } else { 1.5 };
        props.block(Vec3::new(0.14, post_height, 0.14), theme.bark, Transform::from_translation(to_world(p, base + post_height / 2.0)));
        commands
            .spawn((
                ArenaScene,
                Torch { phase: i as f32 * 1.7 },
                PointLight { color: TORCH_LIGHT, intensity: TORCH_INTENSITY, range: 12.0, shadow_maps_enabled: false, ..default() },
                Transform::from_translation(to_world(p, base + post_height + 0.5)),
            ))
            .with_child((Mesh3d(flame.clone()), MeshMaterial3d(flame_mat.clone()), Transform::from_xyz(0.0, -0.3, 0.0)));
    }

    // All static props as one mesh: ~1,200 trees, stones and wall blocks draw as one object.
    commands.spawn((ArenaScene, Mesh3d(meshes.add(props.into_mesh())), MeshMaterial3d(vertex_colored)));
}

/// The land of the whole scene (`Map::scene_tile`) out to `extent` tiles each way from the
/// middle: the map and a ring of forest around it, out to where the haze hides everything.
/// Worked out once, as building the scene asks for each tile many times over.
struct SceneTiles {
    extent: IVec2,
    tiles: Vec<Tile>,
}

impl SceneTiles {
    fn new(m: &Map) -> SceneTiles {
        let extent = m.size / 2 + IVec2::new(30, 24);
        let tiles = (-extent.y..extent.y).flat_map(|y| (-extent.x..extent.x).map(move |x| m.scene_tile(IVec2::new(x, y)))).collect();
        SceneTiles { extent, tiles }
    }

    /// Forest past the scene's edge.
    fn get(&self, t: IVec2) -> Tile {
        let u = t + self.extent;
        let width = 2 * self.extent.x;
        if u.x < 0 || u.y < 0 || u.x >= width || u.y >= 2 * self.extent.y {
            return Tile::Forest;
        }
        self.tiles[(u.y * width + u.x) as usize]
    }
}

/// The four directions, in tiles.
const SIDES: [IVec2; 4] = [IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y];

/// Water or a chasm: what bridges cross, and banks or cliffs drop into.
fn is_gap(tile: Tile) -> bool {
    matches!(tile, Tile::Water | Tile::Ford | Tile::Chasm)
}

#[derive(Clone, Copy)]
enum Shape {
    Rock,
    Trunk,
    Cone(usize),
    Flower,
    Reed,
    Puddle,
    /// A faceted ball: canopies, scrub.
    Ball,
    /// A thin branch, a meter long.
    Stick,
}

/// Builds every static prop into one vertex-colored mesh.
struct Props {
    theme: &'static Theme,
    rock: Mesh,
    trunk: Mesh,
    cones: [Mesh; 3],
    flower: Mesh,
    reed: Mesh,
    puddle: Mesh,
    ball: Mesh,
    stick: Mesh,
    out: Option<Mesh>,
}

impl Props {
    fn new(theme: &'static Theme) -> Self {
        Props {
            theme,
            rock: faceted(Sphere::new(0.6).mesh().ico(0).unwrap()),
            trunk: faceted(Cylinder::new(0.22, 1.2).mesh().resolution(5).build()),
            cones: [0, 1, 2].map(|k| faceted(Cone::new(1.5 - k as f32 * 0.35, 1.7).mesh().resolution(7).build())),
            flower: gem_mesh(0.14),
            reed: faceted(Cone::new(0.035, 1.0).mesh().resolution(3).build()),
            puddle: faceted(Cylinder::new(1.0, 0.02).mesh().resolution(7).build()),
            ball: faceted(Sphere::new(1.0).mesh().ico(0).unwrap()),
            stick: faceted(Cylinder::new(0.05, 1.0).mesh().resolution(4).build()),
            out: None,
        }
    }

    fn add(&mut self, shape: Shape, color: Color, t: Transform) {
        let base = match shape {
            Shape::Rock => &self.rock,
            Shape::Trunk => &self.trunk,
            Shape::Cone(k) => &self.cones[k],
            Shape::Flower => &self.flower,
            Shape::Reed => &self.reed,
            Shape::Puddle => &self.puddle,
            Shape::Ball => &self.ball,
            Shape::Stick => &self.stick,
        };
        self.push(base.clone().transformed_by(t), color);
    }

    /// A cut block (stone, timber) of `size`, placed by `t` (rotation and translation only), its
    /// edges bevelled so they catch the light.
    fn block(&mut self, size: Vec3, color: Color, t: Transform) {
        let bevel = (size.min_element() * 0.12).clamp(0.02, 0.07);
        self.push(bevel_box(size, bevel, color).transformed_by(t), color);
    }

    fn push(&mut self, mut mesh: Mesh, color: Color) {
        let c = color.to_linear();
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[c.red, c.green, c.blue, 1.0]; mesh.count_vertices()]);
        match &mut self.out {
            Some(out) => merge_into(out, &mesh),
            None => self.out = Some(mesh),
        }
    }

    fn into_mesh(self) -> Mesh {
        self.out.expect("the arena has props")
    }

    /// Ruined walls: stacked stone blocks of uneven height, some with a block on top. Returns the
    /// top height per wall tile, so torches can stand on them.
    fn walls(&mut self, m: &Map, rng: &mut Lcg) -> HashMap<IVec2, f32> {
        let mut tops = HashMap::default();
        for (t, tile) in m.tiles() {
            if tile != Tile::Wall {
                continue;
            }
            let c = Map::center(t);
            let h = rng.range(0.9, 1.7);
            let color = self.theme.wall.mix(&self.theme.stone, rng.range(0.0, 0.25));
            self.block(Vec3::new(1.0, h, 1.0), color, Transform::from_translation(to_world(c, h / 2.0)));
            let mut top = h;
            if rng.next() < 0.45 {
                let s = rng.range(0.45, 0.7);
                let off = Vec2::new(rng.range(-0.2, 0.2), rng.range(-0.2, 0.2));
                let turn = Quat::from_rotation_y(rng.range(-0.3, 0.3));
                self.block(Vec3::splat(s), color, Transform::from_translation(to_world(c + off, h + s / 2.0)).with_rotation(turn));
                top += s;
            }
            tops.insert(t, top);
            // Rubble at the foot, below knee height, on dry ground only.
            for _ in 0..2 {
                let p = c + Vec2::new(rng.range(-0.9, 0.9), rng.range(-0.9, 0.9));
                if matches!(m.get(Map::tile_of(p)), Tile::Grass | Tile::Path | Tile::Marsh) {
                    self.add(Shape::Rock, self.theme.slate, Transform::from_translation(to_world(p, 0.05)).with_scale(Vec3::splat(rng.range(0.15, 0.3))));
                }
            }
        }
        tops
    }

    /// Stone bridges, tile by tile: paving slabs, a parapet along every side that drops into
    /// water or a chasm (a cap stone where it ends), and under them a deck down into the water,
    /// or over a chasm an arch, deepest by the rock it springs from.
    fn bridges(&mut self, m: &Map, rng: &mut Lcg) {
        let bridge = |t: IVec2| m.get(t) == Tile::Bridge;
        for (t, tile) in m.tiles() {
            if tile != Tile::Bridge {
                continue;
            }
            let c = Map::center(t);
            let color = if rng.next() < 0.5 { self.theme.stone } else { self.theme.slate };
            self.block(Vec3::new(0.97, 0.36, 0.97), color, Transform::from_translation(to_world(c, -0.18)));
            // How far to the nearest bank, along the bridge.
            let to_bank = SIDES
                .iter()
                .filter_map(|&d| {
                    let steps = (1..12).find(|&k| !bridge(t + d * k))?;
                    m.walkable(t + d * steps).then_some(steps)
                })
                .min()
                .unwrap_or(6);
            let over_water = SIDES.iter().any(|&d| matches!(m.scene_tile(t + d), Tile::Water | Tile::Ford));
            let depth = if over_water { -RIVERBED_LEVEL } else { 0.4 + 2.2 / to_bank as f32 };
            self.block(Vec3::new(1.0, depth, 1.0), self.theme.wall, Transform::from_translation(to_world(c, -0.36 - depth / 2.0)));
            for d in SIDES {
                if !is_gap(m.scene_tile(t + d)) {
                    continue;
                }
                let along = IVec2::new(-d.y, d.x);
                let out = d.as_vec2();
                let size = if d.x == 0 { Vec3::new(1.02, 0.7, 0.36) } else { Vec3::new(0.36, 0.7, 1.02) };
                self.block(size, self.theme.wall.mix(&self.theme.stone, 0.12), Transform::from_translation(to_world(c + out * 0.68, 0.25)));
                for side in [along, -along] {
                    let runs_on = bridge(t + side) && is_gap(m.scene_tile(t + side + d));
                    if !runs_on {
                        let at = c + out * 0.68 + side.as_vec2() * 0.5;
                        self.block(Vec3::new(0.5, 1.1, 0.5), self.theme.wall.mix(&self.theme.stone, 0.24), Transform::from_translation(to_world(at, 0.45)));
                    }
                }
            }
        }
    }

    /// Rope bridges over a chasm, tile by tile: planks laid across the way over, two beams
    /// under them, rope rails along the sides, on posts where they meet the rock.
    fn rope_bridges(&mut self, m: &Map, rng: &mut Lcg) {
        let planks = |t: IVec2| m.get(t) == Tile::Planks;
        for (t, tile) in m.tiles() {
            if tile != Tile::Planks {
                continue;
            }
            let c = Map::center(t);
            // The way over: the axis whose neighbors aren't the drop.
            let along_x = m.scene_tile(t + IVec2::X) != Tile::Chasm && m.scene_tile(t - IVec2::X) != Tile::Chasm;
            let (way, across) = if along_x { (Vec2::X, Vec2::Y) } else { (Vec2::Y, Vec2::X) };
            // Blocks are laid out along x; this turns one to run along the way over.
            let turn = if along_x { Quat::IDENTITY } else { Quat::from_rotation_y(std::f32::consts::FRAC_PI_2) };
            for k in 0..3 {
                let at = c + way * (k as f32 - 1.0) * 0.32 + across * rng.range(-0.04, 0.04);
                let color = self.theme.bark.mix(&self.theme.path, rng.range(0.0, 0.35));
                let tilt = Quat::from_rotation_y(rng.range(-0.06, 0.06));
                self.block(Vec3::new(0.26, 0.07, 1.0), color, Transform::from_translation(to_world(at, -0.04)).with_rotation(turn * tilt));
            }
            for side in [-1.0, 1.0] {
                let at = c + across * side * 0.38;
                self.block(Vec3::new(1.0, 0.1, 0.1), self.theme.bark, Transform::from_translation(to_world(at, -0.14)).with_rotation(turn));
            }
            for d in SIDES {
                if m.scene_tile(t + d) != Tile::Chasm {
                    continue;
                }
                let edge = c + d.as_vec2() * 0.46;
                for h in [0.45, 0.85] {
                    self.block(Vec3::new(1.0, 0.05, 0.05), self.theme.rope, Transform::from_translation(to_world(edge, h)).with_rotation(turn));
                }
                // A hanger from the top rope down to the planks.
                self.block(Vec3::new(0.04, 0.85, 0.04), self.theme.rope, Transform::from_translation(to_world(edge, 0.42)));
                let along = IVec2::new(-d.y, d.x);
                for side in [along, -along] {
                    if !planks(t + side) {
                        let at = edge + side.as_vec2() * 0.5;
                        self.block(Vec3::new(0.16, 1.15, 0.16), self.theme.bark, Transform::from_translation(to_world(at, 0.5)));
                    }
                }
            }
        }
    }

    /// Stepping stones across the fords.
    fn fords(&mut self, m: &Map, rng: &mut Lcg) {
        for (t, tile) in m.tiles() {
            if tile != Tile::Ford {
                continue;
            }
            for _ in 0..2 {
                let p = Map::center(t) + Vec2::new(rng.range(-0.3, 0.3), rng.range(-0.3, 0.3));
                let s = rng.range(0.55, 0.75);
                let stone = Transform::from_translation(to_world(p, -0.12))
                    .with_scale(Vec3::new(s, 0.3, s))
                    .with_rotation(Quat::from_rotation_y(rng.range(0.0, 3.0)));
                self.add(Shape::Rock, self.theme.stone, stone);
            }
        }
    }

    /// A marsh tile's look: here and there a still puddle, and tufts of reeds, all below the knee.
    fn bog(&mut self, rng: &mut Lcg, c: Vec2) {
        if rng.next() < 0.4 {
            let p = c + Vec2::new(rng.range(-0.25, 0.25), rng.range(-0.25, 0.25));
            let size = Vec3::new(rng.range(0.25, 0.45), 1.0, rng.range(0.2, 0.35));
            let t = Transform::from_translation(to_world(p, 0.012)).with_rotation(Quat::from_rotation_y(rng.range(0.0, 3.0))).with_scale(size);
            self.add(Shape::Puddle, self.theme.pond.mix(&self.theme.riverbed, 0.35), t);
        }
        if rng.next() < 0.55 {
            let p = c + Vec2::new(rng.range(-0.35, 0.35), rng.range(-0.35, 0.35));
            self.bog_reeds(rng, p, self.theme.fern, self.theme.path.mix(&self.theme.fern, 0.5));
        }
    }

    /// A tuft of reeds at `p`, below the knee, each one or the other color.
    fn bog_reeds(&mut self, rng: &mut Lcg, p: Vec2, one: Color, other: Color) {
        for _ in 0..4 {
            let at = p + Vec2::new(rng.range(-0.12, 0.12), rng.range(-0.12, 0.12));
            let h = rng.range(0.25, 0.48);
            let lean = Quat::from_euler(EulerRot::XYZ, rng.range(-0.25, 0.25), 0.0, rng.range(-0.25, 0.25));
            let color = if rng.next() < 0.5 { one } else { other };
            self.add(Shape::Reed, color, Transform::from_translation(to_world(at, h / 2.0)).with_rotation(lean).with_scale(Vec3::new(1.0, h, 1.0)));
        }
    }

    fn boulder(&mut self, rng: &mut Lcg, c: Vec2) {
        let t = Transform::from_translation(to_world(c, 0.25))
            .with_rotation(Quat::from_euler(EulerRot::XYZ, rng.range(0.0, 3.0), rng.range(0.0, 3.0), 0.0))
            .with_scale(Vec3::new(rng.range(1.2, 1.5), rng.range(0.9, 1.3), rng.range(1.2, 1.5)));
        self.add(Shape::Rock, self.theme.slate, t);
    }

    fn tree(&mut self, rng: &mut Lcg, p: Vec2, ground: f32, s: f32) {
        self.add(Shape::Trunk, self.theme.bark, Transform::from_translation(to_world(p, ground + 0.6 * s)).with_scale(Vec3::splat(s)));
        let tiers = 2 + (rng.next() * 2.0) as usize;
        for k in 0..tiers {
            let t = Transform::from_translation(to_world(p, ground + (1.5 + k as f32 * 0.85) * s))
                .with_rotation(Quat::from_rotation_y(rng.range(0.0, 3.0)))
                .with_scale(Vec3::splat(s));
            self.add(Shape::Cone(k), if k % 2 == 0 { self.theme.fern } else { self.theme.pine }, t);
        }
    }

    fn flower(&mut self, rng: &mut Lcg, c: Vec2) {
        let p = c + Vec2::new(rng.range(-0.4, 0.4), rng.range(-0.4, 0.4));
        self.add(Shape::Flower, self.theme.bloom, Transform::from_translation(to_world(p, 0.1)));
    }

    /// A map's tree (a `Tile::Tree`), as its wilds grow them.
    fn map_tree(&mut self, rng: &mut Lcg, c: Vec2) {
        match self.theme.wilds {
            Wilds::Forest => self.tree(rng, c, 0.0, 1.25),
            Wilds::Fen => self.willow(rng, c, 0.0, 1.1),
            Wilds::Highlands => self.round_tree(rng, c, 0.0, 1.0),
        }
    }

    /// Something out in the wilds at `p`, on land `ground` high.
    fn wild(&mut self, rng: &mut Lcg, p: Vec2, ground: f32) {
        let pick = rng.next();
        let s = rng.range(0.9, 1.8);
        match self.theme.wilds {
            Wilds::Forest => self.tree(rng, p, ground, s),
            Wilds::Fen if pick < 0.1 => self.snag(rng, p, ground, s),
            Wilds::Fen if pick < 0.18 => self.willow(rng, p, ground, s * 0.8),
            Wilds::Fen => self.reed_bed(rng, p, ground),
            Wilds::Highlands if pick < 0.2 => self.round_tree(rng, p, ground, s * 0.75),
            Wilds::Highlands if pick < 0.32 => self.pillar(rng, p, ground),
            Wilds::Highlands if pick < 0.37 => self.spire(rng, p, ground),
            Wilds::Highlands if pick < 0.4 => self.arch(rng, p, ground),
            Wilds::Highlands if pick < 0.44 => self.slab(rng, p, ground),
            Wilds::Highlands if pick < 0.47 => self.butte(rng, p, ground),
            Wilds::Highlands if pick < 0.55 => {
                let t = Transform::from_translation(to_world(p, ground + 0.2 * s))
                    .with_rotation(Quat::from_euler(EulerRot::XYZ, rng.range(0.0, 3.0), rng.range(0.0, 3.0), 0.0))
                    .with_scale(Vec3::new(1.3, 0.9, 1.2) * s);
                self.add(Shape::Rock, self.theme.slate.mix(&self.theme.wall, rng.next()), t);
            }
            Wilds::Highlands => self.shrub(rng, p, ground),
        }
    }

    /// A dead tree: a grey, bare trunk with a few crooked branches.
    fn snag(&mut self, rng: &mut Lcg, p: Vec2, ground: f32, s: f32) {
        let wood = self.theme.bark.mix(&self.theme.slate, 0.5);
        let tall = 2.0 * s;
        let lean = Quat::from_euler(EulerRot::XYZ, rng.range(-0.12, 0.12), 0.0, rng.range(-0.12, 0.12));
        self.add(Shape::Trunk, wood, Transform::from_translation(to_world(p, ground + 0.6 * tall)).with_rotation(lean).with_scale(Vec3::new(0.7 * s, tall, 0.7 * s)));
        for _ in 0..2 + (rng.next() * 2.0) as usize {
            let from = to_world(p, ground + tall * rng.range(0.6, 1.1));
            let out = Quat::from_rotation_y(rng.range(0.0, std::f32::consts::TAU)) * Quat::from_rotation_z(rng.range(0.6, 1.1));
            let long = rng.range(0.7, 1.2) * s;
            let t = Transform::from_translation(from + out * Vec3::Y * long / 2.0).with_rotation(out).with_scale(Vec3::new(s, long, s));
            self.add(Shape::Stick, wood, t);
        }
    }

    /// A low, wide willow: a short trunk under a heap of dark, drooping foliage.
    fn willow(&mut self, rng: &mut Lcg, p: Vec2, ground: f32, s: f32) {
        self.add(Shape::Trunk, self.theme.bark, Transform::from_translation(to_world(p, ground + 0.6 * s)).with_scale(Vec3::new(1.2 * s, s, 1.2 * s)));
        for k in 0..3 {
            let off = Vec2::from_angle(k as f32 * 2.1 + rng.range(0.0, 1.0)) * 0.5 * s;
            let t = Transform::from_translation(to_world(p + off, ground + rng.range(1.3, 1.7) * s))
                .with_rotation(Quat::from_rotation_y(rng.range(0.0, 3.0)))
                .with_scale(Vec3::new(1.2, 0.85, 1.2) * s);
            self.add(Shape::Ball, if k % 2 == 0 { self.theme.pine } else { self.theme.fern }, t);
        }
    }

    /// A clump of tall reeds standing in the shallows.
    fn reed_bed(&mut self, rng: &mut Lcg, p: Vec2, ground: f32) {
        for _ in 0..3 + (rng.next() * 4.0) as usize {
            let c = p + Vec2::new(rng.range(-0.9, 0.9), rng.range(-0.9, 0.9));
            for _ in 0..5 {
                let at = c + Vec2::new(rng.range(-0.15, 0.15), rng.range(-0.15, 0.15));
                let h = rng.range(0.9, 1.5);
                let lean = Quat::from_euler(EulerRot::XYZ, rng.range(-0.2, 0.2), 0.0, rng.range(-0.2, 0.2));
                let color = [self.theme.fern, self.theme.path.mix(&self.theme.fern, 0.5), self.theme.marsh][(rng.next() * 3.0) as usize % 3];
                self.add(Shape::Reed, color, Transform::from_translation(to_world(at, ground + h / 2.0)).with_rotation(lean).with_scale(Vec3::new(1.4, h, 1.4)));
            }
        }
    }

    /// A round broadleaf in autumn: a trunk under a few faceted balls of brown, rust and gold.
    fn round_tree(&mut self, rng: &mut Lcg, p: Vec2, ground: f32, s: f32) {
        self.add(Shape::Trunk, self.theme.bark, Transform::from_translation(to_world(p, ground + 0.9 * s)).with_scale(Vec3::new(0.8 * s, 1.5 * s, 0.8 * s)));
        let leaves = [self.theme.fern, self.theme.bloom, self.theme.meadow.mix(&self.theme.bloom, 0.35)];
        for k in 0..3 {
            let off = Vec2::from_angle(k as f32 * 2.1 + rng.range(0.0, 1.0)) * 0.45 * s;
            let t = Transform::from_translation(to_world(p + off, ground + rng.range(2.1, 2.7) * s))
                .with_rotation(Quat::from_rotation_y(rng.range(0.0, 3.0)))
                .with_scale(Vec3::splat(rng.range(0.85, 1.1) * s));
            self.add(Shape::Ball, leaves[(rng.next() * 3.0) as usize % 3], t);
        }
    }

    /// A low bush of dry scrub.
    fn shrub(&mut self, rng: &mut Lcg, p: Vec2, ground: f32) {
        let s = rng.range(0.35, 0.6);
        let t = Transform::from_translation(to_world(p, ground + 0.15)).with_rotation(Quat::from_rotation_y(rng.range(0.0, 3.0))).with_scale(Vec3::new(s, s * 0.7, s));
        self.add(Shape::Ball, self.theme.fern.mix(&self.theme.grass, rng.next() * 0.5), t);
    }

    /// A hoodoo: boulders stacked, narrowing and jostled, in bands of color, under a wider cap.
    fn pillar(&mut self, rng: &mut Lcg, p: Vec2, ground: f32) {
        let count = 3 + (rng.next() * 3.0) as usize;
        let first = (rng.next() * 4.0) as usize;
        let mut at = p;
        for k in 0..count {
            let w = 1.9 - k as f32 * rng.range(0.15, 0.3);
            at += Vec2::new(rng.range(-0.12, 0.12), rng.range(-0.12, 0.12));
            let t = Transform::from_translation(to_world(at, ground + 0.5 + k as f32 * 1.05))
                .with_rotation(Quat::from_rotation_y(rng.range(0.0, 3.0)))
                .with_scale(Vec3::new(w, 1.3, w));
            self.add(Shape::Rock, rock_band(self.theme, first + k), t);
        }
        let cap = Transform::from_translation(to_world(at, ground + 0.6 + count as f32 * 1.05))
            .with_rotation(Quat::from_rotation_z(rng.range(-0.15, 0.15)))
            .with_scale(Vec3::new(rng.range(1.8, 2.4), 0.7, rng.range(1.8, 2.4)));
        self.add(Shape::Rock, self.theme.slate.darker(0.1), cap);
    }

    /// A tall, thin needle of rock, leaning a little.
    fn spire(&mut self, rng: &mut Lcg, p: Vec2, ground: f32) {
        let tall = rng.range(4.0, 6.5);
        let lean = Quat::from_euler(EulerRot::XYZ, rng.range(-0.1, 0.1), rng.range(0.0, 3.0), rng.range(-0.1, 0.1));
        let first = (rng.next() * 4.0) as usize;
        for k in 0..3 {
            let up = tall * (0.2 + k as f32 * 0.3);
            let w = 1.3 - k as f32 * 0.35;
            let t = Transform::from_translation(to_world(p, ground) + lean * Vec3::Y * up).with_rotation(lean).with_scale(Vec3::new(w, tall * 0.32, w));
            self.add(Shape::Rock, rock_band(self.theme, first + k), t);
        }
    }

    /// A natural arch: two stacks of rock with a long one laid across their tops.
    fn arch(&mut self, rng: &mut Lcg, p: Vec2, ground: f32) {
        let across = Vec2::from_angle(rng.range(0.0, std::f32::consts::PI));
        let (span, tall) = (rng.range(1.8, 2.6), rng.range(2.6, 3.4));
        for side in [-1.0, 1.0] {
            let foot = p + across * side * span;
            for k in 0..3 {
                let t = Transform::from_translation(to_world(foot, ground + 0.4 + k as f32 * tall / 3.0))
                    .with_rotation(Quat::from_rotation_y(rng.range(0.0, 3.0)))
                    .with_scale(Vec3::new(1.4 - k as f32 * 0.1, tall / 2.6, 1.4 - k as f32 * 0.1));
                self.add(Shape::Rock, rock_band(self.theme, k), t);
            }
        }
        let turn = Quat::from_rotation_y(-across.to_angle());
        let lintel = Transform::from_translation(to_world(p, ground + tall + 0.5)).with_rotation(turn).with_scale(Vec3::new(span * 2.2, 0.9, 1.3));
        self.add(Shape::Rock, rock_band(self.theme, 3), lintel);
    }

    /// A slab of rock stood on end, tipped over to one side.
    fn slab(&mut self, rng: &mut Lcg, p: Vec2, ground: f32) {
        let tilt = Quat::from_euler(EulerRot::YXZ, rng.range(0.0, 3.0), rng.range(-0.35, 0.35), rng.range(-0.2, 0.2));
        let t = Transform::from_translation(to_world(p, ground + 1.3)).with_rotation(tilt).with_scale(Vec3::new(0.7, rng.range(3.5, 4.8), rng.range(2.4, 3.2)));
        self.add(Shape::Rock, rock_band(self.theme, (rng.next() * 4.0) as usize), t);
    }

    /// A squat butte: a wide, flat-topped heap of banded rock with dry grass on top.
    fn butte(&mut self, rng: &mut Lcg, p: Vec2, ground: f32) {
        let w = rng.range(3.5, 5.0);
        for k in 0..3 {
            let t = Transform::from_translation(to_world(p, ground + 0.5 + k as f32 * 0.9))
                .with_rotation(Quat::from_rotation_y(rng.range(0.0, 3.0)))
                .with_scale(Vec3::new(w - k as f32 * 0.4, 1.0, w - k as f32 * 0.4));
            self.add(Shape::Rock, rock_band(self.theme, k), t);
        }
        let top = Transform::from_translation(to_world(p, ground + 2.65)).with_scale(Vec3::new(w * 0.5, 0.18, w * 0.5));
        self.add(Shape::Ball, self.theme.meadow, top);
    }

    /// A rock on the highlands' fight floor: sandstone in one of a few forms, all about a tile
    /// across like any rock there.
    fn sandstone(&mut self, rng: &mut Lcg, c: Vec2) {
        let turn = Quat::from_rotation_y(rng.range(0.0, 3.0));
        match (rng.next() * 3.0) as usize {
            // Flat-topped, with a smaller one balanced on it.
            0 => {
                self.add(Shape::Rock, rock_band(self.theme, 0), Transform::from_translation(to_world(c, 0.3)).with_rotation(turn).with_scale(Vec3::new(1.5, 0.9, 1.4)));
                self.add(Shape::Rock, rock_band(self.theme, 2), Transform::from_translation(to_world(c, 0.9)).with_rotation(turn.inverse()).with_scale(Vec3::new(0.8, 0.7, 0.9)));
            }
            // Split in two, a crack between them.
            1 => {
                let side = Vec2::from_angle(rng.range(0.0, 3.0)) * 0.28;
                for (k, s) in [(1, 1.0), (3, -1.0)] {
                    let t = Transform::from_translation(to_world(c + side * s, 0.35)).with_rotation(turn).with_scale(Vec3::new(0.9, 1.3, 1.4));
                    self.add(Shape::Rock, rock_band(self.theme, k), t);
                }
            }
            // A stack of two, banded.
            _ => {
                self.add(Shape::Rock, rock_band(self.theme, 1), Transform::from_translation(to_world(c, 0.3)).with_rotation(turn).with_scale(Vec3::new(1.4, 0.9, 1.4)));
                self.add(Shape::Rock, rock_band(self.theme, 0), Transform::from_translation(to_world(c, 0.95)).with_rotation(turn).with_scale(Vec3::new(1.1, 0.8, 1.1)));
            }
        }
    }

    /// What grows on the fen's fight floor here and there: tufts of sedge, bog cotton and pads of
    /// moss, all below the ankle.
    fn fen_grass(&mut self, rng: &mut Lcg, c: Vec2) {
        let pick = rng.next();
        let p = c + Vec2::new(rng.range(-0.3, 0.3), rng.range(-0.3, 0.3));
        if pick < 0.06 {
            self.bog_reeds(rng, p, self.theme.fern, self.theme.marsh);
        } else if pick < 0.1 {
            self.flower(rng, c);
        } else if pick < 0.15 {
            let size = Vec3::new(rng.range(0.3, 0.5), 0.05, rng.range(0.25, 0.4));
            let t = Transform::from_translation(to_world(p, 0.0)).with_rotation(Quat::from_rotation_y(rng.range(0.0, 3.0))).with_scale(size);
            self.add(Shape::Ball, self.theme.fern.mix(&self.theme.meadow, 0.3), t);
        }
    }

    /// Worn paving on the fen's path tiles: each tile two to four slabs, uneven, some sunk or
    /// gone, moss showing between them.
    fn flagstones(&mut self, m: &Map, rng: &mut Lcg) {
        for (t, tile) in m.tiles() {
            if tile != Tile::Path {
                continue;
            }
            let c = Map::center(t);
            let split = if rng.next() < 0.5 { Vec2::X } else { Vec2::Y };
            let pieces = 1 + (rng.next() * 2.0) as usize;
            for k in 0..pieces {
                if rng.next() < 0.12 {
                    continue;
                }
                let len = 1.0 / pieces as f32;
                let along = (k as f32 + 0.5) * len - 0.5;
                let size = split * (len - 0.07) + split.perp().abs() * 0.92;
                let color = self.theme.stone.mix(&self.theme.wall, rng.range(0.2, 0.7)).darker(rng.range(0.0, 0.12));
                let turn = Quat::from_rotation_y(rng.range(-0.04, 0.04));
                let at = to_world(c + split * along, rng.range(0.005, 0.025));
                self.block(Vec3::new(size.x, 0.05, size.y), color, Transform::from_translation(at).with_rotation(turn));
            }
        }
    }
}

/// Height of the land outside the clearing (0 at its edge), before roughness: the forest rolls
/// up into low hills, a fen sinks under the water with tussocks breaking it, highlands climb in
/// terraces.
fn wild_height(theme: &Theme, m: &Map, p: Vec2) -> f32 {
    let out = -m.edge_margin(p);
    match theme.wilds {
        Wilds::Forest => (out * 0.3).clamp(0.0, 3.0),
        Wilds::Fen => {
            // Broad, low tussocks: where slow waves running four ways crest together.
            let hump = (waves(p, [0.31, 0.27, 0.19, 0.23], [0.7, 1.3, 0.0, 2.0]) - 0.15).clamp(0.0, 0.5) * 2.0;
            let deep = (out / 3.0).clamp(0.0, 1.0);
            (-0.7 + hump * 0.95) * deep
        }
        Wilds::Highlands => {
            let rise = (out * 0.32).clamp(0.0, 14.0);
            let f = (rise / TERRACE).fract();
            (rise / TERRACE).floor() * TERRACE + TERRACE * ((f - 0.6) / 0.25).clamp(0.0, 1.0)
        }
    }
}

/// The bands of color the highlands' rock is laid down in, from `k` up.
fn rock_band(theme: &Theme, k: usize) -> Color {
    [theme.wall, theme.slate, theme.stone, theme.bank][k % 4]
}

/// How high each of the highlands' terraces climbs.
const TERRACE: f32 = 3.0;

/// The color of a facet of the wilds `height` high, `flat` (its normal's upward part: 1 flat, 0
/// sheer), `pick` a random number for variety.
fn wild_color(theme: &Theme, height: f32, flat: f32, pick: f32) -> Color {
    match theme.wilds {
        Wilds::Forest if height > 1.4 => {
            if pick < 0.5 { theme.fern } else { theme.pine }
        }
        Wilds::Forest if pick < 0.15 => theme.path,
        Wilds::Forest if pick < 0.6 => theme.grass,
        Wilds::Forest => theme.fern,
        Wilds::Fen if pick < 0.4 => theme.marsh,
        Wilds::Fen if pick < 0.8 => theme.grass.mix(&theme.marsh, 0.4),
        Wilds::Fen => theme.fern,
        // Sheer faces in bands of rock by height; dry grass on the shelves.
        Wilds::Highlands if flat < 0.8 => rock_band(theme, (height / 1.3).floor().max(0.0) as usize),
        Wilds::Highlands if pick < 0.5 => theme.grass,
        Wilds::Highlands if pick < 0.8 => theme.meadow,
        Wilds::Highlands => theme.path,
    }
}

/// Meters between the terrain's vertices. Twice a tile: faceted like low-poly land, not like
/// the tile grid.
const TERRAIN_CELL: i32 = 2;

/// The land under and around the map, on a grid of `TERRAIN_CELL` meters, its vertices on tile
/// corners: flat just under the ground tiles, down to the riverbed under water and to the floor
/// under a chasm (a vertex goes down if any tile around it within a cell does, so the whole of
/// every water or chasm tile is low), and out into the wilds around it (`wild_height`), its
/// vertices jostled where there's nothing but wilds around them.
fn terrain_mesh(m: &Map, scene: &SceneTiles, theme: &Theme, rng: &mut Lcg) -> Mesh {
    let cells = scene.extent / TERRAIN_CELL;
    let width = (2 * cells.x + 1) as usize;
    let mut grid = Vec::with_capacity(width * (2 * cells.y + 1) as usize);
    for y in -cells.y..=cells.y {
        for x in -cells.x..=cells.x {
            let v = IVec2::new(x, y) * TERRAIN_CELL;
            let side = 2 * TERRAIN_CELL;
            let around: [Tile; (4 * TERRAIN_CELL * TERRAIN_CELL) as usize] =
                std::array::from_fn(|k| scene.get(v - IVec2::splat(TERRAIN_CELL) + IVec2::new(k as i32 % side, k as i32 / side)));
            let mut p = v.as_vec2();
            let h = if around.contains(&Tile::Chasm) {
                CHASM_FLOOR + rng.range(-0.3, 0.3)
            } else if around.iter().any(|t| matches!(t, Tile::Water | Tile::Ford)) {
                RIVERBED_LEVEL
            } else if around.iter().all(|t| *t != Tile::Forest) {
                -0.05
            } else if around.iter().all(|t| *t == Tile::Forest) && m.edge_margin(p) < -1.0 {
                // Forest floor is rough; the fen's tussocks and the highlands' shelves smoother.
                let (shift, rough) = match theme.wilds {
                    Wilds::Forest => (0.5, (-0.3, 0.6)),
                    Wilds::Fen => (0.3, (-0.05, 0.05)),
                    Wilds::Highlands => (0.3, (-0.15, 0.15)),
                };
                p += Vec2::new(rng.range(-shift, shift), rng.range(-shift, shift));
                wild_height(theme, m, p) + rng.range(rough.0, rough.1)
            } else {
                wild_height(theme, m, p)
            };
            grid.push(to_world(p, h));
        }
    }
    let at = |i: usize, j: usize| grid[j * width + i];
    let mut b = FlatMesh::default();
    for j in 0..(2 * cells.y) as usize {
        for i in 0..width - 1 {
            let (a, r, u, ru) = (at(i, j), at(i + 1, j), at(i, j + 1), at(i + 1, j + 1));
            for tri in [[a, r, u], [r, ru, u]] {
                let height = (tri[0].y + tri[1].y + tri[2].y) / 3.0;
                let flat = (tri[1] - tri[0]).cross(tri[2] - tri[0]).normalize_or_zero().y.abs();
                let base = if height < -1.2 {
                    // Down a chasm: rock darkening toward the floor.
                    theme.slate.mix(&theme.abyss, ((-height - 1.2) / 3.5).min(1.0))
                } else if height < -0.3 {
                    theme.riverbed
                } else if height < 0.05 {
                    theme.grass
                } else {
                    wild_color(theme, height, flat, rng.next())
                };
                b.tri(tri, Color::from(base.to_linear() * (1.0 + rng.range(-0.04, 0.04))));
            }
        }
    }
    b.build()
}

/// 0 to 1 over the ground in broad, wandering patches.
fn patches(p: Vec2) -> f32 {
    (waves(p, [0.37, 0.41, 0.23, 0.29], [1.1, 0.4, 0.0, 2.3]) / 2.0 + 0.5).clamp(0.0, 1.0)
}

/// -1 to 1: slow waves running four ways (along x, y and both diagonals, at `freq`, shifted by
/// `phase`), high where they crest together.
fn waves(p: Vec2, freq: [f32; 4], phase: [f32; 4]) -> f32 {
    let along = [p.x, p.y, p.x + p.y, p.x - p.y];
    (0..4).map(|i| (along[i] * freq[i] + phase[i]).sin()).sum::<f32>() / 4.0
}

/// The walkable ground of the clearing: one quad per tile, one color per tile (a faint checker
/// keeps the grid readable), plus earthen banks down to the water, and cliffs down a chasm in
/// bands of rock.
fn ground_mesh(m: &Map, theme: &Theme, rng: &mut Lcg) -> Mesh {
    let mut b = FlatMesh::default();
    for (t, tile) in m.tiles() {
        if matches!(tile, Tile::Water | Tile::Bridge | Tile::Planks | Tile::Ford | Tile::Chasm | Tile::Forest) {
            continue;
        }
        let c = Map::center(t);
        let checker = if (t.x + t.y) % 2 == 0 { 1.03 } else { 0.97 };
        let base = match tile {
            // Paved: the slabs go on top (`Props::flagstones`); this is the moss between them.
            Tile::Path if theme.wilds == Wilds::Fen => theme.fern.mix(&theme.marsh, rng.next() * 0.4),
            Tile::Path => theme.path.mix(&theme.grass, rng.next() * 0.25),
            Tile::Marsh => theme.marsh.mix(&theme.bank, rng.next() * 0.35),
            // Damp ground: wet hollows and drier rises wandering across the grass.
            _ if theme.wilds == Wilds::Fen => theme.meadow.mix(&theme.marsh, patches(c)).mix(&theme.grass, rng.next() * 0.3),
            _ => theme.grass.mix(&theme.meadow, rng.next() * 0.25),
        };
        let color = Color::from(base.to_linear() * checker);
        let (lo, hi) = (c - Vec2::splat(0.5), c + Vec2::splat(0.5));
        b.quad([to_world(lo, 0.0), to_world(Vec2::new(hi.x, lo.y), 0.0), to_world(hi, 0.0), to_world(Vec2::new(lo.x, hi.y), 0.0)], color);
        // Bank or cliff faces toward the gaps around it.
        for (dir, a, z) in [
            (IVec2::X, Vec2::new(hi.x, lo.y), Vec2::new(hi.x, hi.y)),
            (-IVec2::X, Vec2::new(lo.x, hi.y), Vec2::new(lo.x, lo.y)),
            (IVec2::Y, Vec2::new(hi.x, hi.y), Vec2::new(lo.x, hi.y)),
            (-IVec2::Y, Vec2::new(lo.x, lo.y), Vec2::new(hi.x, lo.y)),
        ] {
            match m.scene_tile(t + dir) {
                Tile::Water | Tile::Ford => {
                    b.quad([to_world(a, 0.0), to_world(a, RIVERBED_LEVEL), to_world(z, RIVERBED_LEVEL), to_world(z, 0.0)], theme.bank);
                }
                Tile::Chasm => {
                    // Topsoil, then grey rock in uneven bands, darkening down into the dark.
                    let bands = [
                        (0.0, -0.5, theme.bank),
                        (-0.5, -1.6, theme.slate),
                        (-1.6, -2.4, theme.wall),
                        (-2.4, -4.0, theme.slate.mix(&theme.abyss, 0.5)),
                        (-4.0, CHASM_FLOOR - 0.3, theme.abyss),
                    ];
                    for (top, bottom, color) in bands {
                        let shade = Color::from(color.to_linear() * (1.0 + rng.range(-0.06, 0.06)));
                        b.quad([to_world(a, top), to_world(a, bottom), to_world(z, bottom), to_world(z, top)], shade);
                    }
                }
                _ => {}
            }
        }
    }
    b.build()
}

/// The water's surface: over every water tile of the scene (the map's and the wilds' beyond it),
/// and over the wilds within a terrain cell of those too, so where the land rises out of it the
/// water runs in under its slope; if the wilds are `flooded`, one sheet over the whole scene (the
/// land rises out of it). `None` if there's no water.
fn water_mesh(scene: &SceneTiles, flooded: bool) -> Option<Mesh> {
    let covers = |tile: Tile| matches!(tile, Tile::Water | Tile::Ford);
    let extent = scene.extent;
    let mut b = FlatMesh::default();
    if flooded {
        let (lo, hi) = ((-extent).as_vec2(), extent.as_vec2());
        b.quad([lo, Vec2::new(hi.x, lo.y), hi, Vec2::new(lo.x, hi.y)].map(|p| to_world(p, WATER_LEVEL)), Color::WHITE);
        return Some(b.build());
    }
    for y in -extent.y..extent.y {
        for x in -extent.x..extent.x {
            let t = IVec2::new(x, y);
            let tile = scene.get(t);
            let near = || tile == Tile::Forest && (-2..=2).any(|dy| (-2..=2).any(|dx| covers(scene.get(t + IVec2::new(dx, dy)))));
            if !(covers(tile) || near()) {
                continue;
            }
            let (lo, hi) = (t.as_vec2(), t.as_vec2() + Vec2::ONE);
            let corners = [lo, Vec2::new(hi.x, lo.y), hi, Vec2::new(lo.x, hi.y)];
            b.quad(corners.map(|p| to_world(p, WATER_LEVEL)), Color::WHITE);
        }
    }
    (!b.positions.is_empty()).then(|| b.build())
}

#[cfg(test)]
mod tests {
    use arena_shared::protocol::ClassId;

    use super::{FIGHTER_LOOKS, SHOT_LOOKS};

    #[test]
    fn every_class_has_its_own_figure() {
        for class in ClassId::all() {
            let key = &class.def().id;
            assert!(FIGHTER_LOOKS.contains(&key.as_str()), "class {key:?} has no figure in fighter_mesh");
        }
    }

    #[test]
    fn shot_looks_are_projectile_classes() {
        for key in SHOT_LOOKS {
            let class = ClassId::by_key(key).unwrap_or_else(|| panic!("{key:?} in SHOT_LOOKS isn't a class"));
            let shoots = class.def().shot(false).or(class.def().shot(true)).is_some();
            assert!(shoots, "{key:?} in SHOT_LOOKS doesn't shoot");
        }
    }
}
