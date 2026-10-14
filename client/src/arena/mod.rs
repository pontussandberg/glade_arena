//! The arena's look: low-poly clearings in a ring of forest, each map in its own theme
//! (`Theme`), with water, chasms, bridges, marshes, ruined walls and torches. See
//! `docs/art-direction.md`. The scene is built from our room's map (`arena_shared::map`), tile
//! by tile, so what you see is exactly what blocks movement and shots; and built again when the
//! map changes.
//!
//! Flat-shaded, untextured, one color per face. The world stays in a calm middle band of
//! saturation; only fighters, projectiles and torch flames go above it.

use bevy::prelude::*;

use crate::cloth::Wardrobe;

mod theme;
pub use theme::*;
mod parts;
pub(crate) use parts::*;
mod javelinist;
pub use javelinist::*;
mod revenant;
pub use revenant::*;
mod frost_mage;
pub use frost_mage::*;
mod zombie;
use zombie::*;
mod shots;
pub use shots::*;
mod scene;
pub use scene::*;

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
    /// The executioner's coat, hood and cape: a cold near-black with a breath of moss in it, its
    /// darker lining and hem, and the ashen grey of its undertunic and wraps.
    pub const HUNTER: Color = Color::srgb_u8(0x1A, 0x1E, 0x1C);
    pub const HUNTER_DARK: Color = Color::srgb_u8(0x0D, 0x10, 0x0F);
    pub const HUNTER_ASH: Color = Color::srgb_u8(0x3A, 0x3E, 0x3C);
    /// Old blood gone black-brown: the executioner's leather apron and the rag on its spear.
    pub const OXBLOOD: Color = Color::srgb_u8(0x33, 0x1D, 0x1B);
    /// Pale, cold bone: the death hunter's bandana, faded.
    pub const BONE: Color = Color::srgb_u8(0xB9, 0xC2, 0xC0);
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
    /// Polished silver: the ground edges of the executioner's blade, the brightest thing on it.
    pub const SILVER: Color = Color::srgb_u8(0xDC, 0xE4, 0xE8);
    /// The glow in fighters' eyes: a pale ghostly green.
    pub const WISP: Color = Color::srgb_u8(0xB8, 0xFF, 0xD6);
    /// Eyes burning a cold, spectral white.
    pub const SOUL: Color = Color::srgb_u8(0xF2, 0xFA, 0xFF);
    pub const HAZE: Color = Color::srgb_u8(0xA9, 0xB8, 0xB6);
    /// A zombie's rotting grey-green skin, its darker rot and wounds, and the dirty linen it
    /// died in.
    pub const ROT: Color = Color::srgb_u8(0x7E, 0x8A, 0x66);
    pub const ROT_DARK: Color = Color::srgb_u8(0x4A, 0x52, 0x3B);
    pub const RAGS: Color = Color::srgb_u8(0x6E, 0x63, 0x50);
    /// HUD text.
    pub const INK: Color = Color::srgb_u8(0x1E, 0x2A, 0x23);

    // Who a fighter is to you, in what marks it (health bar, ground ring, minimap dot):
    // never in its model, its shots or its swings. You are blue, enemies red and
    // allies (once there are teams) green, monsters (zombies) a sickly violet.
    pub const YOU: Color = Color::srgb_u8(0x4C, 0x9E, 0xE0);
    pub const ENEMY: Color = Color::srgb_u8(0xD9, 0x45, 0x3B);
    pub const ALLY: Color = Color::srgb_u8(0x5C, 0xC4, 0x6A);
    pub const MONSTER: Color = Color::srgb_u8(0x9B, 0x6B, 0xC8);
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

/// Class ids that have their own figure in `fighter_mesh`. Every class in `assets/classes/` needs
/// one (and its parts in `fighter_rig` and its moves in `rig/`): the client won't start
/// without them, and a test checks every class has one.
pub const FIGHTER_LOOKS: [&str; 4] = ["javelinist", "revenant", "frost_mage", "zombie"];

/// Class ids whose shots have their own look in `shot_look` (the rest throw round bolts).
pub const SHOT_LOOKS: [&str; 2] = ["javelinist", "frost_mage"];

/// A fighter's parts that move on their own (posed by `rig/`); every class has them. Each
/// hangs from its joint at the origin; the fighter faces +X, its right (weapon) side is +Z. Drawn
/// in the fighter's own material (colors in the vertices) except the eyes.
pub struct RigMeshes {
    /// Turns to look where the fighter walks or aims.
    pub head: Mesh,
    /// An arm, hanging down (-Y) from the shoulder.
    pub arm: Mesh,
    /// A forearm hanging down from the elbow (`rig::Moves::elbow_at`), in its own space, if the arm bends
    /// there (`arm` is then only the upper arm).
    pub forearm: Option<Mesh>,
    /// The left arm, if it isn't `arm` (and `forearm`) again: an empty sleeve.
    pub lead_arm: Option<Mesh>,
    /// A leg, hanging down from the hip.
    pub leg: Mesh,
    /// A shin hanging down from the knee (`RIG_KNEE`), in its own space, if the leg bends there
    /// (`leg` is then only the thigh).
    pub shin: Option<Mesh>,
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
    /// What lights up on the held weapon (in its space), glowing like the eyes, while the
    /// attack's `follow_up` is armed (`rig::show_auras`): cracks of light through a blade.
    pub held_aura: Option<Mesh>,
    /// A sharp glint of white light catching now and then somewhere along the held blade's edge
    /// (`rig::gleam_blades`).
    pub held_glint: Option<(Mesh, Edge)>,
    /// Cloth that moves (see `cloth/`), in the fighter's space.
    pub wardrobe: Option<Wardrobe>,
}

/// Where along a blade's edge (0..1, from its base to its point) is where on the weapon, in the
/// weapon's space.
pub type Edge = fn(f32) -> Vec3;

/// Where a rig's parts attach, in the fighter's space (feet at the origin). Hips and shoulders
/// are given for the right side; the left mirrors them in Z.
pub const RIG_NECK: Vec3 = Vec3::new(0.0, 1.42, 0.0);
pub const RIG_SHOULDER: Vec3 = Vec3::new(0.0, 1.32, 0.3);
pub const RIG_HIP: Vec3 = Vec3::new(0.0, 0.8, 0.11);
/// Where the hand is, in the arm's space.
pub const RIG_HAND: Vec3 = Vec3::new(0.0, -0.52, 0.0);
/// Where the knee is, in the leg's space.
pub const RIG_KNEE: Vec3 = Vec3::new(0.0, -0.4, 0.0);
/// Where the elbow is, in the arm's space.
pub const RIG_ELBOW: Vec3 = Vec3::new(0.0, -0.26, 0.0);

/// A class's moving parts: its hood, sleeves, legs, weapon and (if it has one) cape.
pub fn fighter_rig(class_key: &str) -> RigMeshes {
    match class_key {
        "javelinist" => RigMeshes {
            head: javelinist_hood(),
            arm: javelinist_arm(),
            forearm: Some(javelinist_forearm()),
            shin: Some(javelinist_shin()),
            lead_arm: Some(javelinist_stump()),
            leg: javelinist_leg(),
            held: held_spear(),
            eyes: slit_eyes(HUNTER_EYES),
            glow: palette::SOUL,
            held_glow: None,
            body_glow: None,
            held_aura: None,
            held_glint: Some((glint(), spear_edge)),
            wardrobe: Some(javelinist_wardrobe()),
        },
        "revenant" => RigMeshes {
            head: revenant_head(),
            arm: revenant_arm(),
            forearm: None,
            shin: None,
            lead_arm: None,
            leg: booted_leg(palette::ROBE, palette::DARK_LEATHER, 0.082, 0.07),
            held: glaive(),
            eyes: slit_eyes(REVENANT_EYES),
            glow: palette::WISP,
            held_glow: Some(glaive_glow()),
            body_glow: Some(revenant_glow()),
            held_aura: Some(glaive_cracks()),
            held_glint: Some((glint(), glaive_edge)),
            wardrobe: Some(revenant_wardrobe()),
        },
        "frost_mage" => RigMeshes {
            head: frost_crowned_hood(),
            arm: frost_mage_arm(),
            forearm: None,
            shin: None,
            lead_arm: None,
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
        "zombie" => RigMeshes {
            head: zombie_head(),
            arm: zombie_arm(),
            forearm: None,
            shin: None,
            lead_arm: None,
            leg: booted_leg(palette::RAGS.darker(0.15), palette::ROT_DARK, 0.085, 0.07),
            held: zombie_claws(),
            eyes: slit_eyes(ZOMBIE_EYES),
            glow: palette::WISP,
            held_glow: None,
            body_glow: None,
            held_aura: None,
            held_glint: None,
            wardrobe: None,
        },
        _ => unreachable!("no rig for class {class_key:?} (add it to FIGHTER_LOOKS)"),
    }
}

/// A fighter's low-poly figure, feet at the origin, picked by class id so each class has its own
/// silhouette from above. Every class needs one (see `FIGHTER_LOOKS`).
pub fn fighter_mesh(class_key: &str) -> Mesh {
    // Drawn in a white material: these are the real colors, the same for every player (ground
    // rings and health bars tell who is who).
    match class_key {
        // Marksman: a death hunter in a hooded coat and a pale bandana, a knife at his belt.
        // Head, arms, legs and spear are separate, animated parts (`fighter_rig`).
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
        // Monster (no one picks it): a hunched, rotting villager in a torn shirt. Head, bare
        // arms, legs and claws are separate, animated parts (`fighter_rig`).
        "zombie" => zombie_body(),
        _ => unreachable!("no figure for class {class_key:?} (add it to FIGHTER_LOOKS)"),
    }
}

/// How far the point of a held javelin is ahead of the hand.
pub const GRIP_TO_TIP: f32 = 1.2;

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
            let shoots = class.def().shot(None).is_some() || class.def().abilities().any(|(slot, _)| class.def().shot(Some(slot)).is_some());
            assert!(shoots, "{key:?} in SHOT_LOOKS doesn't shoot");
        }
    }
}
