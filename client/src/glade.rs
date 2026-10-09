//! "The Glade" art direction: a low-poly forest clearing at dusk, with a river, a stone bridge,
//! ruined walls and torches. See `docs/art-direction.md`. The layout itself comes from the shared
//! `arena_shared::map`, so what you see is exactly what blocks movement and shots.
//!
//! Flat-shaded, untextured, one color per face. The world stays in a calm middle band of
//! saturation; only fighters, projectiles and torch flames go above it.

use arena_shared::classes::{ClassDef, Shot};
use arena_shared::map::{MAP_HALF_EXTENTS, Map, RIVER_HALF_WIDTH, Tile, clearing_margin, map, river_x};
use bevy::asset::RenderAssetUsages;
use bevy::color::Mix;
use bevy::light::CascadeShadowConfigBuilder;
use bevy::mesh::PrimitiveTopology;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

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
    /// Antlers, skulls, bindings.
    /// Pale and cold, like old bone in moonlight.
    pub const BONE: Color = Color::srgb_u8(0xB9, 0xC2, 0xC0);
    /// Near-black leathers: trousers and hoods; boots and eye sockets.
    pub const LEATHER: Color = Color::srgb_u8(0x2E, 0x27, 0x24);
    pub const DARK_LEATHER: Color = Color::srgb_u8(0x17, 0x14, 0x14);
    /// Ashen fur trim (mantles, tails).
    pub const FUR: Color = Color::srgb_u8(0x55, 0x55, 0x52);
    /// Charred, ash-black antlers; pauldrons, gauntlets, sword guards.
    pub const ASH: Color = Color::srgb_u8(0x2B, 0x2A, 0x2E);
    /// The javelinist's hunter's robe: a dark, muted moss green, and its darker lining.
    pub const HUNTER: Color = Color::srgb_u8(0x1F, 0x24, 0x1D);
    pub const HUNTER_DARK: Color = Color::srgb_u8(0x12, 0x15, 0x11);
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
    /// The glow in fighters' eyes: a pale ghostly green, the same for everyone.
    pub const WISP: Color = Color::srgb_u8(0xB8, 0xFF, 0xD6);
    pub const HAZE: Color = Color::srgb_u8(0xA9, 0xB8, 0xB6);
    /// HUD text.
    pub const INK: Color = Color::srgb_u8(0x1E, 0x2A, 0x23);

    // Fighters: the only saturated colors. You are the only blue; rivals are warm.
    pub const YOU: Color = Color::srgb_u8(0x4C, 0x9E, 0xE0);
    /// Spectral blue: spirit spears (whoever throws them) and rift-step streaks glow with it.
    pub const SPIRIT: Color = Color::srgb_u8(0x8F, 0xD8, 0xFF);
    /// Ice: the frost mage's crystals, its nova, and the ice that freezes a fighter in place.
    /// Paler and whiter than spirit.
    pub const ICE: Color = Color::srgb_u8(0xC4, 0xEE, 0xFF);
    /// What a slowed fighter's colors are multiplied by: a cold blue cast.
    pub const FROSTBITE: Color = Color::srgb_u8(0x8C, 0xB8, 0xFF);
    pub const RIVALS: [Color; 3] = [
        Color::srgb_u8(0xE8, 0x80, 0x3A), // ember
        Color::srgb_u8(0xE6, 0xB2, 0x3A), // marigold
        Color::srgb_u8(0xE3, 0x5F, 0x5A), // coral
    ];

    // Light. Fire is the one warm, saturated thing in the world: small, static and flickering.
    pub const SUN: Color = Color::srgb_u8(0xFF, 0xE2, 0xC4);
    pub const SKY: Color = Color::srgb_u8(0x8F, 0xA4, 0xC2);
    pub const TORCH_FLAME: Color = Color::srgb_u8(0xFF, 0xB2, 0x57);
    pub const TORCH_LIGHT: Color = Color::srgb_u8(0xFF, 0x94, 0x43);
}

use palette::*;

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

/// See-through glow: swing flashes and telegraphs.
pub fn translucent(color: Color, alpha: f32, strength: f32) -> StandardMaterial {
    StandardMaterial { alpha_mode: AlphaMode::Blend, ..glow(color.with_alpha(alpha), strength) }
}

/// One normal per face: the low-poly look.
pub fn faceted(mesh: Mesh) -> Mesh {
    mesh.with_duplicated_vertices().with_computed_flat_normals()
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

/// A cone pointing down: tatters, tails, capes.
fn hanging_cone(r: f32, h: f32, sides: u32) -> Mesh {
    cone(r, h, sides).rotated_by(Quat::from_rotation_x(std::f32::consts::PI))
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
            all.merge(&p).expect("parts share attributes");
            all
        })
        .expect("at least one part")
}

/// A smooth ball: heads, knuckles, bone knobs.
fn ball(radius: f32) -> Mesh {
    Sphere::new(radius).mesh().ico(2).unwrap()
}

/// Gives every vertex `color`, so one material can carry several colors: vertex colors multiply
/// the material's base color (the team color on a body, white on bone-and-wood trim). All parts
/// merged together need it, or none.
fn tinted(mut mesh: Mesh, color: Color) -> Mesh {
    let count = mesh.count_vertices();
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![color.to_linear().to_f32_array(); count]);
    mesh
}

/// The javelinist's spear, standing along +Y with its point at the origin: a thin metal frame
/// holding a heavy blade, carried and thrown alike.
fn frame_spear(shaft: Color, frame: Color) -> Mesh {
    const LENGTH: f32 = 2.0;
    /// How far back from the tip the solid point runs.
    const POINT: f32 = 0.26;
    // The head (flat in Z, so it shows from above as it flies): an open diamond of thin rods, a
    // spine down the middle, a collar where it meets the shaft, and filling its front a solid,
    // heavy point, flat-faced so its edges and center ridge catch the light.
    let (base, widest, half) = (-0.7, -0.46, 0.11);
    let corners = [Vec3::Y * base, Vec3::new(0.0, widest, half), Vec3::ZERO, Vec3::new(0.0, widest, -half)];
    let mut parts: Vec<Mesh> = (0..4).map(|i| rod(corners[i], corners[(i + 1) % 4], 0.012, frame)).collect();
    // Knobs closing the joints at the back and sides.
    for corner in [corners[0], corners[1], corners[3]] {
        parts.push(tinted(ball(0.017).translated_by(corner), frame));
    }
    parts.push(rod(Vec3::Y * base, Vec3::Y * -POINT, 0.009, frame));
    let point = taper(half * POINT / -widest + 0.012, 0.0, POINT, 4, Vec3::Y * -POINT / 2.0, frame);
    parts.push(faceted(point.scaled_by(Vec3::new(0.7, 1.0, 1.0))));
    parts.push(taper(0.038, 0.03, 0.08, 14, Vec3::Y * (base - 0.035), frame));
    // The shaft, bound in silver at the grip, and a short spike at its butt.
    parts.push(rod(Vec3::Y * (base - 0.07), Vec3::Y * -(LENGTH - 0.12), 0.024, shaft));
    for at in [-1.12, -1.32] {
        parts.push(taper(0.03, 0.03, 0.035, 14, Vec3::Y * at, frame));
    }
    parts.push(tinted(hanging_cone(0.022, 0.12, 10).translated_by(Vec3::Y * -(LENGTH - 0.06)), frame));
    sculpted(parts)
}

/// A thin round rod from `a` to `b`.
fn rod(a: Vec3, b: Vec3, radius: f32, color: Color) -> Mesh {
    let along = b - a;
    let mesh = Cylinder::new(radius, along.length()).mesh().resolution(10).build();
    let turn = Quat::from_rotation_arc(Vec3::Y, along.normalize());
    tinted(mesh.rotated_by(turn).translated_by((a + b) / 2.0), color)
}

/// A box of `size` centered at `at`, in one color.
fn block(size: Vec3, at: Vec3, color: Color) -> Mesh {
    tinted(Cuboid::from_size(size).mesh().build().translated_by(at), color)
}

/// A tapered prism (`sides` sides; with enough, a smooth taper) standing on Y, centered at `at`, in one color: what
/// low-poly figures are made of (limbs narrowing to wrists and ankles, a chest wider than the
/// waist).
fn taper(bottom: f32, top: f32, height: f32, sides: u32, at: Vec3, color: Color) -> Mesh {
    let mesh = ConicalFrustum { radius_top: top, radius_bottom: bottom, height }.mesh().resolution(sides).build();
    tinted(mesh.translated_by(at), color)
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
    /// Eyes in the head's space, drawn glowing in `glow`.
    pub eyes: Mesh,
    /// What the eyes, and the glowing parts below, glow with.
    pub glow: Color,
    /// Parts of the held weapon (in its space) and of the body (in the fighter's space) drawn
    /// glowing, like the eyes: crystals, runes.
    pub held_glow: Option<Mesh>,
    pub body_glow: Option<Mesh>,
    /// Something that hangs from the upper back and swings on its own (a tail, a cape),
    /// hanging down (-Y) from `RIG_TAIL`.
    pub tail: Option<Mesh>,
}

/// Where a rig's parts attach, in the fighter's space (feet at the origin). Hips and shoulders
/// are given for the right side; the left mirrors them in Z.
pub const RIG_NECK: Vec3 = Vec3::new(0.0, 1.42, 0.0);
pub const RIG_SHOULDER: Vec3 = Vec3::new(0.0, 1.32, 0.3);
pub const RIG_HIP: Vec3 = Vec3::new(0.0, 0.8, 0.11);
/// Where the hand is, in the arm's space.
pub const RIG_HAND: Vec3 = Vec3::new(0.0, -0.52, 0.0);
/// Where a tail or cape hangs from, on the upper back.
pub const RIG_TAIL: Vec3 = Vec3::new(-0.15, 1.38, 0.0);

/// Where the javelinist's eyes sit in its mask (and ±z, the other one).
const EYE: Vec3 = Vec3::new(0.2, 0.26, 0.07);

/// A class's moving parts. The javelinist's are smooth (a plague-masked hood, robe sleeves
/// ending in bone claws, baggy trousers with bone knee plates, the frame
/// spear); the revenant's are smooth too (a void hood, tattered sleeves and ash gauntlets,
/// wrapped legs, a tattered cape, a sword).
pub fn fighter_rig(class_key: &str) -> RigMeshes {
    match class_key {
        "javelinist" => RigMeshes {
            head: bone_mask_head(),
            // Robe sleeves with a dark cuff and a leather bracer, ending in bony claws.
            arm: sculpted(vec![
                taper(0.07, 0.092, 0.3, 18, Vec3::Y * -0.15, palette::HUNTER),
                taper(0.1, 0.078, 0.07, 18, Vec3::Y * -0.31, palette::HUNTER_DARK),
                taper(0.062, 0.072, 0.15, 18, Vec3::Y * -0.41, palette::DARK_LEATHER),
                tinted(ball(0.06).scaled_by(Vec3::new(1.0, 0.8, 0.9)).translated_by(RIG_HAND), palette::DARK_LEATHER),
                claws(),
            ]),
            // Baggy trousers gathered at the ankle into narrow boots, a bone plate on the knee.
            leg: sculpted(vec![
                taper(0.13, 0.105, 0.56, 18, Vec3::Y * -0.28, palette::LEATHER),
                taper(0.088, 0.105, 0.06, 18, Vec3::Y * -0.58, palette::DARK_LEATHER),
                taper(0.08, 0.088, 0.14, 18, Vec3::Y * -0.68, palette::DARK_LEATHER),
                tinted(ball(0.085).scaled_by(Vec3::new(1.9, 0.55, 1.0)).translated_by(Vec3::new(0.07, -0.76, 0.0)), palette::DARK_LEATHER),
                tinted(ball(0.07).scaled_by(Vec3::new(0.55, 1.0, 1.0)).translated_by(Vec3::new(0.1, -0.3, 0.0)), palette::BONE),
            ]),
            held: held_spear(),
            eyes: eye_slits(EYE.x + 0.01, EYE.y, EYE.z),
            glow: palette::WISP,
            held_glow: None,
            body_glow: None,
            tail: None,
        },
        "revenant" => RigMeshes {
            head: hooded_void(),
            // Robe sleeves with a tattered hang, an ash gauntlet.
            arm: sculpted(vec![
                taper(0.075, 0.105, 0.32, 16, Vec3::Y * -0.15, palette::ROBE),
                tinted(hanging_cone(0.09, 0.22, 10).translated_by(Vec3::new(-0.04, -0.28, 0.0)), palette::TATTERS),
                taper(0.065, 0.08, 0.2, 16, Vec3::Y * -0.39, palette::ASH),
                tinted(ball(0.062).translated_by(RIG_HAND), palette::ASH),
            ]),
            // Wrapped legs, boots.
            leg: sculpted(vec![
                taper(0.07, 0.095, 0.48, 16, Vec3::Y * -0.24, palette::TATTERS),
                taper(0.08, 0.088, 0.2, 16, Vec3::Y * -0.6, palette::DARK_LEATHER),
                tinted(ball(0.085).scaled_by(Vec3::new(1.7, 0.55, 0.9)).translated_by(Vec3::new(0.06, -0.755, 0.0)), palette::DARK_LEATHER),
            ]),
            held: sword(),
            eyes: eye_slits(0.13, 0.2, 0.06),
            glow: palette::WISP,
            held_glow: None,
            body_glow: None,
            // The tattered cape.
            tail: Some(tinted(
                hanging_cone(0.22, 0.9, 14).scaled_by(Vec3::new(0.45, 1.0, 1.1)).translated_by(Vec3::Y * -0.45),
                palette::TATTERS,
            )),
        },
        "frost_mage" => RigMeshes {
            head: frost_crowned_hood(),
            // Wide bell sleeves, banded in blue and rimed at the cuff, over a slim dark glove.
            arm: sculpted(vec![
                taper(0.07, 0.095, 0.3, 16, Vec3::Y * -0.15, palette::FROST_ROBE),
                taper(0.15, 0.08, 0.16, 16, Vec3::Y * -0.36, palette::FROST_ROBE),
                taper(0.149, 0.144, 0.045, 16, Vec3::Y * -0.41, palette::FROST_BLUE),
                taper(0.155, 0.15, 0.03, 16, Vec3::Y * -0.45, palette::RIME),
                taper(0.05, 0.06, 0.1, 16, Vec3::Y * -0.46, palette::DARK_LEATHER),
                tinted(ball(0.055).translated_by(RIG_HAND), palette::DARK_LEATHER),
            ]),
            // Mostly under the robe: dark legs and soft boots.
            leg: sculpted(vec![
                taper(0.075, 0.1, 0.5, 16, Vec3::Y * -0.25, palette::FROST_DARK),
                taper(0.075, 0.085, 0.2, 16, Vec3::Y * -0.62, palette::DARK_LEATHER),
                tinted(ball(0.08).scaled_by(Vec3::new(1.7, 0.55, 0.9)).translated_by(Vec3::new(0.06, -0.755, 0.0)), palette::DARK_LEATHER),
            ]),
            held: ice_staff(),
            eyes: eye_slits(0.14, 0.19, 0.06),
            glow: palette::FROST_GLOW,
            held_glow: Some(staff_crystals()),
            body_glow: Some(frost_mage_glow()),
            // A long, narrow cape, rimed along its hem.
            tail: Some(sculpted(vec![
                tinted(hanging_cone(0.22, 1.05, 14).scaled_by(Vec3::new(0.4, 1.0, 1.15)).translated_by(Vec3::Y * -0.52), palette::FROST_DARK),
            ])),
        },
        _ => unreachable!("no rig for class {class_key:?} (add it to FIGHTER_LOOKS)"),
    }
}

/// Narrow, slanted slits (a scowl) at (`x`, `y`, ±`z`) in the head's space, raised enough to
/// glow from above. Drawn in a glow material (white here).
fn eye_slits(x: f32, y: f32, z: f32) -> Mesh {
    merge_parts(
        [1.0, -1.0]
            .map(|side| {
                let slit = Sphere::new(0.032).mesh().ico(1).unwrap().scaled_by(Vec3::new(0.7, 0.55, 1.5));
                let slant = Quat::from_rotation_x(side * 0.4);
                tinted(slit.rotated_by(slant).translated_by(Vec3::new(x, y, side * z)), Color::WHITE)
            })
            .to_vec(),
    )
}

/// The revenant's head: a tall, pointed hood with nothing inside but the eyes.
fn hooded_void() -> Mesh {
    sculpted(vec![
        taper(0.21, 0.05, 0.58, 18, Vec3::new(-0.05, 0.25, 0.0), palette::ROBE),
        tinted(ball(0.12).scaled_by(Vec3::new(0.5, 1.0, 0.9)).translated_by(Vec3::new(0.08, 0.18, 0.0)), palette::TATTERS),
    ])
}

/// The frost mage's head: a pointed hood, empty but for the eyes, with a crown of ice shards
/// rising around it (so it reads from above).
fn frost_crowned_hood() -> Mesh {
    let mut parts = vec![
        taper(0.2, 0.05, 0.5, 18, Vec3::new(-0.05, 0.24, 0.0), palette::FROST_ROBE),
        tinted(ball(0.12).scaled_by(Vec3::new(0.5, 1.0, 0.9)).translated_by(Vec3::new(0.08, 0.18, 0.0)), palette::FROST_DARK),
        taper(0.205, 0.2, 0.04, 18, Vec3::new(-0.04, 0.03, 0.0), palette::RIME),
    ];
    // Shards fanning up and out from the crown, tallest at the front.
    for i in 0..7 {
        let around = i as f32 / 7.0 * std::f32::consts::TAU;
        let height = 0.22 + 0.1 * (around.cos() * 0.5 + 0.5);
        let shard = ice_shard(0.035, height).rotated_by(Quat::from_rotation_z(-0.45));
        let turned = shard.rotated_by(Quat::from_rotation_y(around));
        parts.push(tinted(turned.translated_by(Vec3::new(-0.04, 0.32, 0.0)), palette::ICE));
    }
    sculpted(parts)
}

/// A long crystal standing on Y, base at the origin: a narrow prism ending in a point.
fn ice_shard(radius: f32, height: f32) -> Mesh {
    let body = cylinder(radius, height * 0.7, 5).translated_by(Vec3::Y * height * 0.35);
    let point = cone(radius, height * 0.3, 5).translated_by(Vec3::Y * height * 0.85);
    faceted(sculpted(vec![body, point]))
}

/// The frost mage's staff, gripped at the origin, standing up (+Y): a dark shaft bound in blue and
/// shod in steel, and at the top two steel crescents crossing like an open cage around its
/// crystals (`staff_crystals`, drawn glowing).
fn ice_staff() -> Mesh {
    let wood = palette::BARK.darker(0.15);
    let mut parts = vec![
        rod(Vec3::Y * -0.62, Vec3::Y * 1.04, 0.024, wood),
        // The steel butt spike and its collar.
        tinted(hanging_cone(0.03, 0.16, 10).translated_by(Vec3::Y * -0.7), palette::STEEL),
        taper(0.034, 0.03, 0.05, 12, Vec3::Y * -0.6, palette::STEEL),
        // Blue bindings either side of the grip and up the shaft.
        taper(0.032, 0.032, 0.06, 12, Vec3::Y * 0.12, palette::FROST_BLUE),
        taper(0.032, 0.032, 0.06, 12, Vec3::Y * -0.12, palette::FROST_BLUE),
        taper(0.03, 0.03, 0.03, 12, Vec3::Y * 0.6, palette::FROST_BLUE),
        taper(0.03, 0.03, 0.03, 12, Vec3::Y * 0.68, palette::FROST_BLUE),
        // The collar holding the head.
        taper(0.03, 0.05, 0.1, 12, Vec3::Y * 1.02, palette::STEEL),
        taper(0.052, 0.052, 0.025, 12, Vec3::Y * 1.08, palette::FROST_BLUE),
    ];
    for side in CRESCENTS {
        let steps = 7;
        for i in 0..steps {
            let (a, b) = (crescent(side, i as f32 / steps as f32), crescent(side, (i + 1) as f32 / steps as f32));
            parts.push(rod(a, b, 0.013, palette::STEEL));
        }
        parts.push(tinted(ball(0.022).translated_by(crescent(side, 1.0)), palette::STEEL));
    }
    sculpted(parts)
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
    let crystal = faceted(Sphere::new(0.07).mesh().ico(0).unwrap()).scaled_by(Vec3::new(0.85, 2.7, 0.85));
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
    let gem = faceted(Sphere::new(0.045).mesh().ico(0).unwrap()).scaled_by(Vec3::new(0.6, 1.4, 1.0));
    parts.push(tinted(gem.translated_by(Vec3::new(0.165, 0.88, 0.0)), Color::WHITE));
    // Runes: small upright diamonds just above the hem, following the robe's curve.
    for i in 0..10 {
        let around = (i as f32 + 0.5) / 10.0 * std::f32::consts::TAU;
        let (sin, cos) = around.sin_cos();
        let rune = faceted(Sphere::new(0.03).mesh().ico(0).unwrap()).scaled_by(Vec3::new(0.35, 1.5, 1.0));
        let turned = rune.rotated_by(Quat::from_rotation_y(-around));
        parts.push(tinted(turned.translated_by(Vec3::new(cos * 0.385 * 0.85, 0.17, sin * 0.385)), Color::WHITE));
    }
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

/// A long, straight sword, gripped at the origin, blade up (+Y).
fn sword() -> Mesh {
    let tip = cone(0.06, 0.2, 4).scaled_by(Vec3::new(0.6, 1.0, 1.6));
    sculpted(vec![
        tinted(ball(0.045).translated_by(Vec3::Y * -0.17), palette::ASH),
        tinted(cylinder(0.032, 0.24, 12).translated_by(Vec3::Y * -0.04), palette::DARK_LEATHER),
        block(Vec3::new(0.07, 0.05, 0.3), Vec3::Y * 0.1, palette::ASH),
        block(Vec3::new(0.035, 1.0, 0.11), Vec3::Y * 0.62, palette::STEEL),
        tinted(tip.translated_by(Vec3::Y * 1.22), palette::STEEL),
    ])
}

/// A deep hood with nothing in it but a bone plague mask: glowing slit eyes in sunken sockets,
/// a long beak curving down to a point, a strap around the hood, and a charred, branching
/// antler crown growing out of it, so the javelinist reads from above.
fn bone_mask_head() -> Mesh {
    use std::f32::consts::FRAC_PI_2;
    let mut parts = vec![
        // The hood, and the darkness inside it.
        taper(0.22, 0.06, 0.5, 18, Vec3::new(-0.07, 0.24, 0.0), palette::HUNTER_DARK),
        tinted(ball(0.15).translated_by(Vec3::new(0.0, 0.2, 0.0)), palette::TATTERS),
        // The mask's face plate, and a strap holding it on around the hood.
        tinted(ball(0.15).scaled_by(Vec3::new(0.55, 1.1, 0.85)).translated_by(Vec3::new(0.12, 0.22, 0.0)), palette::BONE),
        taper(0.165, 0.16, 0.035, 20, Vec3::new(0.0, 0.31, 0.0), palette::DARK_LEATHER),
    ];
    // The beak: two segments, the second bending further down to a point.
    let mut at = Vec3::new(0.19, 0.17, 0.0);
    for (down, length, (from, to)) in [(0.25, 0.2, (0.07, 0.048)), (0.75, 0.2, (0.048, 0.006))] {
        let dir = Vec3::new(f32::cos(down), -f32::sin(down), 0.0);
        let segment = taper(from, to, length, 18, Vec3::ZERO, palette::BONE)
            .rotated_by(Quat::from_rotation_z(-(FRAC_PI_2 + down)))
            .translated_by(at + dir * length / 2.0);
        parts.push(segment);
        at += dir * length;
    }
    for side in [1.0, -1.0] {
        // A sunken socket around the glowing eye.
        let socket = ball(0.045).scaled_by(Vec3::new(0.6, 0.8, 1.3));
        parts.push(tinted(socket.translated_by(EYE.with_z(side * EYE.z) - Vec3::X * 0.01), palette::DARK_LEATHER));
        // Main beam: three segments narrowing to a point, rising and curving out wide to the
        // side (a crown from above) and sweeping back, the tip hooking forward; pointed tines
        // rise off its joints, and a brow tine juts forward from the base.
        let mut at = Vec3::new(0.06, 0.36, side * 0.09);
        let radii = [0.05, 0.038, 0.027, 0.01];
        let bends = [(0.7, 0.35), (1.0, 0.18), (1.25, -0.08)];
        let lengths = [0.32, 0.3, 0.26];
        let mut joints = Vec::new();
        for (i, ((out, back), length)) in bends.into_iter().zip(lengths).enumerate() {
            let tilt = Quat::from_rotation_x(side * out) * Quat::from_rotation_z(back);
            let segment = taper(radii[i], radii[i + 1], length, 8, Vec3::Y * length / 2.0, palette::ASH);
            parts.push(segment.rotated_by(tilt).translated_by(at));
            at += tilt * Vec3::Y * length;
            joints.push(at);
        }
        let tine = |length: f32, tilt: Quat, from: Vec3| {
            let point = taper(0.024, 0.003, length, 8, Vec3::Y * length / 2.0, palette::ASH);
            point.rotated_by(tilt).translated_by(from)
        };
        let upward = Quat::from_rotation_x(side * 0.3) * Quat::from_rotation_z(-0.5);
        parts.push(tine(0.3, upward, joints[0]));
        parts.push(tine(0.26, upward, joints[1]));
        let brow = Quat::from_rotation_x(side * 0.45) * Quat::from_rotation_z(-1.15);
        parts.push(tine(0.2, brow, Vec3::new(0.1, 0.4, side * 0.11)));
    }
    sculpted(parts)
}

/// Bony claws for fingers, curling down and forward from the hand.
fn claws() -> Mesh {
    let mut parts = Vec::new();
    for (i, side) in [-0.035, 0.0, 0.035].into_iter().enumerate() {
        let length = 0.1 + 0.02 * (i % 2) as f32;
        let claw = hanging_cone(0.016, length, 6).rotated_by(Quat::from_rotation_z(0.35));
        parts.push(tinted(claw.translated_by(RIG_HAND + Vec3::new(0.04, -0.07, side)), palette::BONE));
    }
    sculpted(parts)
}

/// The spear in hand, gripped a little behind its middle.
fn held_spear() -> Mesh {
    frame_spear(palette::IRON, palette::SILVER).translated_by(Vec3::Y * GRIP_TO_TIP)
}

/// A fighter's low-poly figure, feet at the origin, picked by class id so each class has its own
/// silhouette from above. Every class needs one (see `FIGHTER_LOOKS`).
pub fn fighter_mesh(class_key: &str) -> Mesh {
    // Drawn in a white material: these are the real colors, the same for every player (rings,
    // health bars and shots tell teams apart).
    match class_key {
        // Hunter, dark and bony: a long, belted hunter's robe in near-black moss green, split up
        // the front; bone ribs strapped over the chest, an executioner's spiked iron shoulder
        // plates, vertebrae down the spine. Smoothly shaded. Head, arms, legs, tail and spear
        // are separate, animated parts (`fighter_rig`).
        "javelinist" => {
            let deep = Vec3::new(0.64, 1.0, 1.0);
            let mut parts = vec![
                taper(0.2, 0.28, 0.56, 20, Vec3::Y * 1.1, palette::HUNTER).scaled_by(deep),
                taper(0.33, 0.21, 0.68, 20, Vec3::Y * 0.54, palette::HUNTER).scaled_by(Vec3::new(0.74, 1.0, 1.0)),
                block(Vec3::new(0.04, 0.6, 0.1), Vec3::new(0.21, 0.52, 0.0), palette::HUNTER_DARK),
                taper(0.215, 0.215, 0.08, 20, Vec3::Y * 0.9, palette::DARK_LEATHER).scaled_by(deep),
                tinted(ball(0.04).translated_by(Vec3::new(0.14, 0.9, 0.0)), palette::BONE),
            ];
            // Ribs: three curved bones across the chest, longest at the top.
            for (i, y) in [1.28, 1.16, 1.04].into_iter().enumerate() {
                let rib = taper(0.018, 0.018, 0.34 - 0.04 * i as f32, 8, Vec3::ZERO, palette::BONE)
                    .rotated_by(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2));
                parts.push(rib.translated_by(Vec3::new(0.17, y, 0.0)));
            }
            parts.push(taper(0.022, 0.022, 0.3, 8, Vec3::new(0.18, 1.15, 0.0), palette::BONE));
            // An executioner's shoulders: a dark iron plate sloping off each shoulder, studded
            // along the edge, a single spike curving up and back.
            let iron = palette::IRON.darker(0.18);
            for side in [1.0, -1.0] {
                let slope = Quat::from_rotation_x(side * 0.35);
                let plate = ball(0.12).scaled_by(Vec3::new(1.0, 0.36, 1.05)).rotated_by(slope);
                parts.push(tinted(plate.translated_by(Vec3::new(0.0, 1.44, side * 0.24)), iron));
                for x in [-0.06, 0.0, 0.06] {
                    parts.push(tinted(ball(0.013).translated_by(Vec3::new(x, 1.41, side * 0.34)), palette::STEEL));
                }
                let spike = taper(0.03, 0.003, 0.2, 10, Vec3::Y * 0.1, iron)
                    .rotated_by(Quat::from_rotation_x(side * 0.35) * Quat::from_rotation_z(0.45));
                parts.push(spike.translated_by(Vec3::new(-0.02, 1.47, side * 0.24)));
            }
            // Vertebrae down the spine, each with a spur.
            for i in 0..5 {
                let y = 1.45 - 0.11 * i as f32;
                parts.push(tinted(ball(0.035).translated_by(Vec3::new(-0.17, y, 0.0)), palette::BONE));
                let spur = cone(0.022, 0.09, 6).rotated_by(Quat::from_rotation_z(1.2)).translated_by(Vec3::new(-0.22, y + 0.02, 0.0));
                parts.push(tinted(spur, palette::BONE));
            }
            sculpted(parts)
        }
        // Duelist: a spectral revenant in a cold near-black robe fraying into tatters below the
        // knee, a tattered cape and ash pauldrons. Head, arms, legs and sword are separate,
        // animated parts (`fighter_rig`).
        "revenant" => {
            let mut parts = vec![
                taper(0.2, 0.29, 0.5, 18, Vec3::Y * 1.12, palette::ROBE).scaled_by(Vec3::new(0.66, 1.0, 1.0)),
                taper(0.36, 0.22, 0.5, 20, Vec3::Y * 0.62, palette::ROBE),
                taper(0.23, 0.23, 0.06, 20, Vec3::Y * 0.88, palette::ASH).scaled_by(Vec3::new(0.7, 1.0, 1.0)),
            ];
            for side in [1.0, -1.0] {
                let pauldron = ball(0.14).scaled_by(Vec3::new(1.0, 0.7, 1.0));
                parts.push(tinted(pauldron.translated_by(Vec3::new(0.0, 1.4, side * 0.25)), palette::ASH));
            }
            for i in 0..10 {
                let around = Quat::from_rotation_y(i as f32 * std::f32::consts::TAU / 10.0 + 0.2);
                let shard = hanging_cone(0.065, 0.18 + 0.07 * (i % 3) as f32, 8);
                parts.push(tinted(shard.translated_by(Vec3::new(0.33, 0.34, 0.0)).rotated_by(around), palette::TATTERS));
            }
            sculpted(parts)
        }
        // Controller: a frost mage in a long, deep navy robe flaring to the ground, rimed at
        // the hem, with a blue sash, front panel and band above the hem, and a heavy rime mantle
        // over the shoulders (the ice shards growing out of it glow: `frost_mage_glow`). Head (an
        // ice-crowned hood), arms, legs, cape and staff are separate, animated parts
        // (`fighter_rig`).
        "frost_mage" => {
            let parts = vec![
                taper(0.19, 0.27, 0.5, 18, Vec3::Y * 1.12, palette::FROST_ROBE).scaled_by(Vec3::new(0.66, 1.0, 1.0)),
                taper(0.42, 0.21, 0.84, 22, Vec3::Y * 0.44, palette::FROST_ROBE).scaled_by(Vec3::new(0.85, 1.0, 1.0)),
                // The front panel, down the slope of the robe.
                rod(Vec3::new(0.35, 0.06, 0.0), Vec3::new(0.18, 0.86, 0.0), 0.032, palette::FROST_BLUE),
                taper(0.43, 0.42, 0.05, 22, Vec3::Y * 0.03, palette::RIME).scaled_by(Vec3::new(0.85, 1.0, 1.0)),
                taper(0.415, 0.405, 0.05, 22, Vec3::Y * 0.08, palette::FROST_BLUE).scaled_by(Vec3::new(0.85, 1.0, 1.0)),
                taper(0.22, 0.22, 0.07, 20, Vec3::Y * 0.88, palette::FROST_BLUE).scaled_by(Vec3::new(0.7, 1.0, 1.0)),
                tinted(ball(0.3).scaled_by(Vec3::new(0.7, 0.3, 1.1)).translated_by(Vec3::Y * 1.38), palette::RIME),
            ];
            sculpted(parts)
        }
        _ => unreachable!("no figure for class {class_key:?} (add it to FIGHTER_LOOKS)"),
    }
}

/// A flat fan for a melee swing: `reach` long, `arc_degrees` wide, pointing along world +X.
pub fn swing_mesh(reach: f32, arc_degrees: f32) -> Mesh {
    let mut b = FlatMesh::default();
    let half = arc_degrees.to_radians() / 2.0;
    let steps = 12;
    let point = |t: f32| Vec3::new(t.cos() * reach, 0.0, -t.sin() * reach);
    for i in 0..steps {
        let (a, z) = (-half + 2.0 * half * i as f32 / steps as f32, -half + 2.0 * half * (i + 1) as f32 / steps as f32);
        b.tri([Vec3::ZERO, point(a), point(z)], Color::WHITE);
    }
    b.build()
}

/// A flat strip along world +X from `from` to `to`, `width` wide: the lane a shot will fly down.
pub fn lane_mesh(from: f32, to: f32, width: f32) -> Mesh {
    let mut b = FlatMesh::default();
    let side = Vec3::Z * width / 2.0;
    let (near, far) = (Vec3::X * from, Vec3::X * to);
    b.quad([near + side, far + side, far - side, near - side], Color::WHITE);
    b.build()
}

/// How a class's shot looks (see `SHOT_LOOKS`).
#[derive(Clone)]
pub struct ShotLook<M = Mesh> {
    pub mesh: M,
    /// The mesh carries its own colors (a real weapon, drawn plain); otherwise it's drawn as a
    /// glow.
    pub colored: bool,
    /// A thrown spear: leaves the thrower's hand where the javelin is held (gripped `GRIP_TO_TIP`
    /// behind its point) and trails wind (`wind_mesh`).
    pub thrown: bool,
}

/// How far the point of a held javelin is ahead of the hand.
/// Turns a mesh pointing up (+Y) to point along +X: how a shot flies.
const ALONG_X: Quat = Quat::from_xyzw(0.0, 0.0, -std::f32::consts::FRAC_1_SQRT_2, std::f32::consts::FRAC_1_SQRT_2);
pub const GRIP_TO_TIP: f32 = 1.2;

/// A class's shot (auto-attack or, `ability`, Q), flying along world +X, the point that hits at
/// the origin, picked by class id: the javelinist throws the very javelin it carries (and its Q
/// is a plain spectral spear, drawn glowing); the frost mage an ice crystal; the rest round
/// bolts of the shot's radius.
pub fn shot_look(class: &ClassDef, shot: Shot, ability: bool) -> ShotLook {
    match (class.id.as_str(), ability) {
        ("javelinist", false) => ShotLook {
            mesh: frame_spear(palette::IRON, palette::SILVER).rotated_by(ALONG_X),
            colored: true,
            thrown: true,
        },
        ("javelinist", true) => {
            ShotLook { mesh: frame_spear(Color::WHITE, Color::WHITE).rotated_by(ALONG_X), colored: false, thrown: true }
        }
        // A frostbolt: a long ice crystal, point first.
        ("frost_mage", false) => {
            let length = shot.radius * 5.0;
            let bolt = ice_shard(shot.radius * 0.7, length).translated_by(Vec3::Y * -length).rotated_by(ALONG_X);
            ShotLook { mesh: bolt, colored: false, thrown: false }
        }
        _ => ShotLook { mesh: gem_mesh(shot.radius), colored: false, thrown: false },
    }
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
pub fn tile_color(tile: Tile) -> Color {
    match tile {
        Tile::Grass => MOSS,
        Tile::Path => PATH,
        Tile::Ford => RIVERBED.mix(&POND, 0.6),
        Tile::Bridge => STONE,
        Tile::Water => POND,
        Tile::Wall => WALL,
        Tile::Rock => SLATE,
        Tile::Tree => PINE,
        Tile::Forest => PINE.mix(&INK, 0.4),
    }
}

/// Haze that fades the forest ring into the sky color; goes on the camera.
pub fn haze() -> DistanceFog {
    DistanceFog { color: HAZE, falloff: FogFalloff::Linear { start: 40.0, end: 95.0 }, ..default() }
}

pub struct GladePlugin;

impl Plugin for GladePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(HAZE));
        app.insert_resource(GlobalAmbientLight { color: SKY, brightness: 160.0, ..default() });
        app.add_systems(Startup, build_glade);
        app.add_systems(Update, flicker_torches);
    }
}

const WATER_LEVEL: f32 = -0.3;
const RIVERBED_LEVEL: f32 = -0.65;

/// Deterministic, so the glade looks the same for every player and every run.
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
struct FlatMesh {
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
    fn tri_faded(&mut self, corners: [Vec3; 3], alphas: [f32; 3]) {
        for (p, alpha) in corners.into_iter().zip(alphas) {
            self.positions.push(p.to_array());
            self.colors.push([1.0, 1.0, 1.0, alpha]);
        }
    }

    /// A quad from four corners in order (counter-clockwise seen from its front).
    fn quad(&mut self, [a, b, c, d]: [Vec3; 4], color: Color) {
        self.tri([a, b, c], color);
        self.tri([a, c, d], color);
    }

    fn build(self) -> Mesh {
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colors)
            .with_computed_flat_normals()
    }
}

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

fn build_glade(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut rng = Lcg(7);
    let m = map();

    commands.spawn((
        DirectionalLight { color: SUN, illuminance: 3_600.0, shadow_maps_enabled: true, ..default() },
        // Low dusk sun from the upper left, so shadows are long and fall toward the camera's right.
        Transform::from_xyz(-32.0, 14.0, -18.0).looking_at(Vec3::ZERO, Vec3::Y),
        // Enough to cover what the camera sees at full zoom-out; the far forest needs no shadows.
        CascadeShadowConfigBuilder { num_cascades: 1, maximum_distance: 50.0, ..default() }.build(),
    ));

    let vertex_colored = materials.add(matte(Color::WHITE));
    commands.spawn((Mesh3d(meshes.add(terrain_mesh(&mut rng))), MeshMaterial3d(vertex_colored.clone())));
    commands.spawn((Mesh3d(meshes.add(ground_mesh(&mut rng))), MeshMaterial3d(vertex_colored.clone())));
    commands.spawn((
        Mesh3d(meshes.add(water_mesh())),
        MeshMaterial3d(materials.add(StandardMaterial { perceptual_roughness: 0.35, reflectance: 0.5, ..matte(POND) })),
    ));

    let mut props = Props::new();
    let wall_tops = props.walls(&mut rng);
    props.bridge(&mut rng);
    props.fords(&mut rng);
    for (t, tile) in m.tiles() {
        let c = Map::center(t);
        match tile {
            Tile::Rock => props.boulder(&mut rng, c),
            Tile::Tree => props.tree(&mut rng, c, 0.0, 1.25),
            Tile::Grass if rng.next() < 0.02 => props.flower(&mut rng, c),
            _ => {}
        }
    }
    // Forest ring: dense trees outside the clearing, kept out of the river.
    for _ in 0..320 {
        let p = Vec2::new(rng.range(-58.0, 58.0), rng.range(-40.0, 40.0));
        if clearing_margin(p) > -1.2 || (p.x - river_x(p.y)).abs() < RIVER_HALF_WIDTH + 1.5 {
            continue;
        }
        let scale = rng.range(0.9, 1.8);
        props.tree(&mut rng, p, ground_height(p), scale);
    }

    // Torches on the bridge and the ruins (posts go into the prop mesh; flames and lights don't).
    let flame = meshes.add(faceted(Cone::new(0.16, 0.42).mesh().resolution(5).build()));
    let flame_mat = materials.add(glow(TORCH_FLAME, 8.0));
    for (i, &p) in m.torches.iter().enumerate() {
        let base = wall_tops.get(&Map::tile_of(p)).copied().unwrap_or(0.0);
        let post_height = if base > 0.0 { 0.5 } else { 1.5 };
        props.add(Shape::Post, BARK, Transform::from_translation(to_world(p, base + post_height / 2.0)).with_scale(Vec3::new(1.0, post_height, 1.0)));
        commands
            .spawn((
                Torch { phase: i as f32 * 1.7 },
                PointLight { color: TORCH_LIGHT, intensity: TORCH_INTENSITY, range: 12.0, shadow_maps_enabled: false, ..default() },
                Transform::from_translation(to_world(p, base + post_height + 0.5)),
            ))
            .with_child((Mesh3d(flame.clone()), MeshMaterial3d(flame_mat.clone()), Transform::from_xyz(0.0, -0.3, 0.0)));
    }

    // All static props as one mesh: ~1,200 trees, stones and wall blocks draw as one object.
    commands.spawn((Mesh3d(meshes.add(props.into_mesh())), MeshMaterial3d(vertex_colored)));
}

#[derive(Clone, Copy)]
enum Shape {
    Rock,
    Trunk,
    Cone(usize),
    Block,
    Flower,
    Post,
}

/// Builds every static prop into one vertex-colored mesh.
struct Props {
    rock: Mesh,
    trunk: Mesh,
    cones: [Mesh; 3],
    block: Mesh,
    flower: Mesh,
    post: Mesh,
    out: Option<Mesh>,
}

impl Props {
    fn new() -> Self {
        Props {
            rock: faceted(Sphere::new(0.6).mesh().ico(0).unwrap()),
            trunk: faceted(Cylinder::new(0.22, 1.2).mesh().resolution(5).build()),
            cones: [0, 1, 2].map(|k| faceted(Cone::new(1.5 - k as f32 * 0.35, 1.7).mesh().resolution(7).build())),
            block: faceted(Cuboid::new(1.0, 1.0, 1.0).mesh().build()),
            flower: gem_mesh(0.14),
            post: faceted(Cylinder::new(0.08, 1.0).mesh().resolution(5).build()),
            out: None,
        }
    }

    fn add(&mut self, shape: Shape, color: Color, t: Transform) {
        let base = match shape {
            Shape::Rock => &self.rock,
            Shape::Trunk => &self.trunk,
            Shape::Cone(k) => &self.cones[k],
            Shape::Block => &self.block,
            Shape::Flower => &self.flower,
            Shape::Post => &self.post,
        };
        let mut mesh = base.clone().transformed_by(t);
        let c = color.to_linear();
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![[c.red, c.green, c.blue, 1.0]; mesh.count_vertices()]);
        match &mut self.out {
            Some(out) => out.merge(&mesh).expect("prop meshes share attributes"),
            None => self.out = Some(mesh),
        }
    }

    fn into_mesh(self) -> Mesh {
        self.out.expect("the glade has props")
    }

    /// Ruined walls: stacked stone blocks of uneven height, some with a block on top. Returns the
    /// top height per wall tile, so torches can stand on them.
    fn walls(&mut self, rng: &mut Lcg) -> HashMap<IVec2, f32> {
        let mut tops = HashMap::default();
        for (t, tile) in map().tiles() {
            if tile != Tile::Wall {
                continue;
            }
            let c = Map::center(t);
            let h = rng.range(0.9, 1.7);
            let color = WALL.mix(&STONE, rng.range(0.0, 0.25));
            self.add(Shape::Block, color, Transform::from_translation(to_world(c, h / 2.0)).with_scale(Vec3::new(1.0, h, 1.0)));
            let mut top = h;
            if rng.next() < 0.45 {
                let s = rng.range(0.45, 0.7);
                let off = Vec2::new(rng.range(-0.2, 0.2), rng.range(-0.2, 0.2));
                self.add(Shape::Block, color, Transform::from_translation(to_world(c + off, h + s / 2.0)).with_scale(Vec3::splat(s)));
                top += s;
            }
            tops.insert(t, top);
            // Rubble at the foot, below knee height.
            for _ in 0..2 {
                let p = c + Vec2::new(rng.range(-0.9, 0.9), rng.range(-0.9, 0.9));
                if map().walkable_at(p) {
                    self.add(Shape::Rock, SLATE, Transform::from_translation(to_world(p, 0.05)).with_scale(Vec3::splat(rng.range(0.15, 0.3))));
                }
            }
        }
        tops
    }

    /// The stone bridge: paving slabs on the bridge tiles, parapets along both sides, and piers
    /// down into the river.
    fn bridge(&mut self, rng: &mut Lcg) {
        let tiles: Vec<Vec2> = map().tiles().filter(|(_, t)| *t == Tile::Bridge).map(|(t, _)| Map::center(t)).collect();
        for &c in &tiles {
            let color = if rng.next() < 0.5 { STONE } else { SLATE };
            self.add(Shape::Block, color, Transform::from_translation(to_world(c, -0.18)).with_scale(Vec3::new(0.97, 0.36, 0.97)));
        }
        let span = tiles.iter().map(|c| Rect::from_center_size(*c, Vec2::ONE)).reduce(|a, b| a.union(b)).expect("the map has a bridge");
        let mid = span.center();
        for y in [span.min.y - 0.18, span.max.y + 0.18] {
            // Parapet, with a cap stone on each end.
            let parapet = Transform::from_translation(to_world(Vec2::new(mid.x, y), 0.25)).with_scale(Vec3::new(span.width() + 0.8, 0.7, 0.36));
            self.add(Shape::Block, WALL.mix(&STONE, 0.12), parapet);
            for x in [span.min.x - 0.3, span.max.x + 0.3] {
                self.add(Shape::Block, WALL.mix(&STONE, 0.24), Transform::from_translation(to_world(Vec2::new(x, y), 0.45)).with_scale(Vec3::new(0.5, 1.1, 0.5)));
            }
        }
        for x in [mid.x - 1.2, mid.x + 1.2] {
            let pier = Transform::from_translation(to_world(Vec2::new(x, mid.y), (RIVERBED_LEVEL - 0.36) / 2.0))
                .with_scale(Vec3::new(0.8, -RIVERBED_LEVEL, span.height()));
            self.add(Shape::Block, WALL, pier);
        }
    }

    /// Stepping stones across the fords.
    fn fords(&mut self, rng: &mut Lcg) {
        for (t, tile) in map().tiles() {
            if tile != Tile::Ford {
                continue;
            }
            for _ in 0..2 {
                let p = Map::center(t) + Vec2::new(rng.range(-0.3, 0.3), rng.range(-0.3, 0.3));
                let s = rng.range(0.55, 0.75);
                let stone = Transform::from_translation(to_world(p, -0.12))
                    .with_scale(Vec3::new(s, 0.3, s))
                    .with_rotation(Quat::from_rotation_y(rng.range(0.0, 3.0)));
                self.add(Shape::Rock, STONE, stone);
            }
        }
    }

    fn boulder(&mut self, rng: &mut Lcg, c: Vec2) {
        let t = Transform::from_translation(to_world(c, 0.25))
            .with_rotation(Quat::from_euler(EulerRot::XYZ, rng.range(0.0, 3.0), rng.range(0.0, 3.0), 0.0))
            .with_scale(Vec3::new(rng.range(1.2, 1.5), rng.range(0.9, 1.3), rng.range(1.2, 1.5)));
        self.add(Shape::Rock, SLATE, t);
    }

    fn tree(&mut self, rng: &mut Lcg, p: Vec2, ground: f32, s: f32) {
        self.add(Shape::Trunk, BARK, Transform::from_translation(to_world(p, ground + 0.6 * s)).with_scale(Vec3::splat(s)));
        let tiers = 2 + (rng.next() * 2.0) as usize;
        for k in 0..tiers {
            let t = Transform::from_translation(to_world(p, ground + (1.5 + k as f32 * 0.85) * s))
                .with_rotation(Quat::from_rotation_y(rng.range(0.0, 3.0)))
                .with_scale(Vec3::splat(s));
            self.add(Shape::Cone(k), if k % 2 == 0 { FERN } else { PINE }, t);
        }
    }

    fn flower(&mut self, rng: &mut Lcg, c: Vec2) {
        let p = c + Vec2::new(rng.range(-0.4, 0.4), rng.range(-0.4, 0.4));
        self.add(Shape::Flower, HEATHER, Transform::from_translation(to_world(p, 0.1)));
    }
}

/// Height of the faceted terrain outside the clearing (0-ish inside it).
fn ground_height(p: Vec2) -> f32 {
    let margin = clearing_margin(p);
    if margin >= 0.0 { -0.05 } else { (-margin * 0.3).min(3.0) }
}

/// The rolling forest ring around the clearing, with the river channel cut through it.
fn terrain_mesh(rng: &mut Lcg) -> Mesh {
    const SIZE: Vec2 = Vec2::new(124.0, 88.0);
    const CELLS: (usize, usize) = (62, 44);
    let corner = |i: usize, j: usize| Vec2::new(-SIZE.x / 2.0 + SIZE.x * i as f32 / CELLS.0 as f32, -SIZE.y / 2.0 + SIZE.y * j as f32 / CELLS.1 as f32);
    let mut grid = Vec::with_capacity((CELLS.0 + 1) * (CELLS.1 + 1));
    for j in 0..=CELLS.1 {
        for i in 0..=CELLS.0 {
            let mut p = corner(i, j);
            let in_channel = (p.x - river_x(p.y)).abs() < RIVER_HALF_WIDTH + 1.2;
            let rough = clearing_margin(p) < -1.0 && !in_channel;
            if rough {
                p += Vec2::new(rng.range(-0.5, 0.5), rng.range(-0.5, 0.5));
            }
            let h = if in_channel { RIVERBED_LEVEL } else { ground_height(p) + if rough { rng.range(-0.3, 0.6) } else { 0.0 } };
            grid.push(to_world(p, h));
        }
    }
    let at = |i: usize, j: usize| grid[j * (CELLS.0 + 1) + i];
    let mut b = FlatMesh::default();
    for j in 0..CELLS.1 {
        for i in 0..CELLS.0 {
            let (a, r, u, ru) = (at(i, j), at(i + 1, j), at(i, j + 1), at(i + 1, j + 1));
            for tri in [[a, r, u], [r, ru, u]] {
                let height = (tri[0].y + tri[1].y + tri[2].y) / 3.0;
                let pick = rng.next();
                let base = if height < -0.3 {
                    RIVERBED
                } else if height < 0.05 {
                    MOSS
                } else if height > 1.4 {
                    if pick < 0.5 { FERN } else { PINE }
                } else if pick < 0.15 {
                    PATH
                } else if pick < 0.6 {
                    MOSS
                } else {
                    FERN
                };
                b.tri(tri, Color::from(base.to_linear() * (1.0 + rng.range(-0.04, 0.04))));
            }
        }
    }
    b.build()
}

/// The walkable ground of the clearing: one quad per tile, one color per tile (a faint checker
/// keeps the grid readable), plus earthen banks where the ground meets the river.
fn ground_mesh(rng: &mut Lcg) -> Mesh {
    let m = map();
    let mut b = FlatMesh::default();
    for (t, tile) in m.tiles() {
        if matches!(tile, Tile::Water | Tile::Bridge | Tile::Ford | Tile::Forest) {
            continue;
        }
        let c = Map::center(t);
        let checker = if (t.x + t.y) % 2 == 0 { 1.03 } else { 0.97 };
        let base = match tile {
            Tile::Path => PATH.mix(&MOSS, rng.next() * 0.25),
            _ => MOSS.mix(&MEADOW, rng.next() * 0.25),
        };
        let color = Color::from(base.to_linear() * checker);
        let (lo, hi) = (c - Vec2::splat(0.5), c + Vec2::splat(0.5));
        b.quad([to_world(lo, 0.0), to_world(Vec2::new(hi.x, lo.y), 0.0), to_world(hi, 0.0), to_world(Vec2::new(lo.x, hi.y), 0.0)], color);
        // Bank faces toward river tiles.
        for (dir, a, z) in [
            (IVec2::X, Vec2::new(hi.x, lo.y), Vec2::new(hi.x, hi.y)),
            (-IVec2::X, Vec2::new(lo.x, hi.y), Vec2::new(lo.x, lo.y)),
            (IVec2::Y, Vec2::new(hi.x, hi.y), Vec2::new(lo.x, hi.y)),
            (-IVec2::Y, Vec2::new(lo.x, lo.y), Vec2::new(hi.x, lo.y)),
        ] {
            if matches!(m.get(t + dir), Tile::Water | Tile::Ford) {
                b.quad([to_world(a, 0.0), to_world(a, RIVERBED_LEVEL), to_world(z, RIVERBED_LEVEL), to_world(z, 0.0)], BANK);
            }
        }
    }
    b.build()
}

/// The river surface, following its centerline through the whole map and into the forest.
fn water_mesh() -> Mesh {
    let mut b = FlatMesh::default();
    let half = RIVER_HALF_WIDTH + 1.2;
    let extent = MAP_HALF_EXTENTS.y + 22.0;
    let mut y = -extent;
    while y < extent {
        let y2 = y + 1.0;
        let (x1, x2) = (river_x(y), river_x(y2));
        b.quad(
            [
                to_world(Vec2::new(x1 - half, y), WATER_LEVEL),
                to_world(Vec2::new(x1 + half, y), WATER_LEVEL),
                to_world(Vec2::new(x2 + half, y2), WATER_LEVEL),
                to_world(Vec2::new(x2 - half, y2), WATER_LEVEL),
            ],
            Color::WHITE,
        );
        y = y2;
    }
    b.build()
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
