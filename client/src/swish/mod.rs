//! A blade's wind: what a thrusting blade leaves in the air. A faint trail follows the blade
//! itself wherever it moves fast (a lunge, a dash), so it shows where the sword just was, twice as
//! bright while its Follow Through is on; and each strike leaves a mark, one thin, sharp line of
//! wind along its lane, lying on what the blade comes down on, lingering a moment as it fades.
//! Air casts no shadow. A strike carries on into the ground (empowered or not): as the blade
//! comes down, the ground tears open all over its hitbox (its melee lane: the rectangle it
//! hits), lying on whatever's under it in pieces, one on each surface (`lay`), onto the spectral
//! realm beneath: the void showing dark through it, cracks edged in ink
//! with wisp light welling up out of them (flaring as it breaks, then glowing low and breathing),
//! a burst of spectral mist, and chunks of
//! earth thrown up and tumbling back (see-through and self-lit, so it reads on any ground). The
//! tear lingers a while, then closes and fades. A blade whose point drags along the ground (a
//! glaive carried trailing behind) scuffs a faint line in it, fading as it goes, and rasps: now
//! and then a fine streak of spectral light, or a dark mote of the void, skips off where it
//! scrapes. The same whoever swung (who it was shows in the
//! health bar).

use arena_shared::map::Map;
use arena_shared::protocol::*;
use bevy::prelude::*;
use bevy::transform::TransformSystems;

use crate::arena::{self, palette};
use crate::plain::PlainMaterial;

mod trail;
pub(crate) use trail::*;
mod lay;
pub(crate) use lay::*;
#[cfg(test)]
mod bake;
#[cfg(test)]
pub(crate) use bake::*;
mod lying;
pub(crate) use lying::*;
mod marks;
use marks::*;
mod rift;
pub(crate) use rift::*;
mod drag;
use drag::*;
mod embers;
use embers::*;

pub struct SwishPlugin;

impl Plugin for SwishPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(LyingPlugin);
        app.add_systems(Startup, load_swish);
        app.add_systems(Update, (add_trails, drive_marks, (lay_impacts, drive_impacts).chain(), throw_chunks, burn_in_rifts));
        // The trails laid out among the effects' parts, once those are (`fx::DrawParts`).
        app.add_systems(PostUpdate, trail_blades.after(crate::fx::DrawParts).after(TransformSystems::Propagate));
        app.add_systems(PostUpdate, (show_marks, strike_ground, drag_marks, shed_embers).chain().after(TransformSystems::Propagate));
        app.add_systems(Update, (fly_sparks, drift_embers));
    }
}

/// On a fighter holding a blade out (`rig/`): the blade (its grip at the origin, the blade
/// along +Y), how long it is, the last swing a mark was drawn for, and whether its attack's
/// `follow_up` is on (`rig::show_auras`), which brightens its trail.
#[derive(Component)]
pub(crate) struct Blade {
    pub(crate) held: Entity,
    pub(crate) length: f32,
    /// Where along it the blade starts (a fraction of its length): a glaive's is its outer half,
    /// past the shaft.
    pub(crate) from: f32,
    pub(crate) shown: u32,
    pub(crate) quickened: bool,
    /// A strike that has just gone off: the ground breaks over its hitbox this frame, on what's
    /// under it (`Underfoot`), if it has one.
    pub(crate) striking: bool,
    pub(crate) struck: Option<Underfoot>,
    /// Where on the ground its point last left a scuff, while it's dragging there (`drag_marks`).
    pub(crate) dragged: Option<Vec3>,
    /// The outer corner of its edge, in its own space: a broad blade reaches the ground there
    /// before its point does (`lowest`).
    pub(crate) corner: Vec3,
    /// Counts its scuffs, to pick which throw sparks.
    pub(crate) rasp: u32,
    /// While it's quickened: when (seconds) it sheds its next ember, and how many it has shed
    /// (`shed_embers`).
    pub(crate) embers: (f32, u32),
}

pub(crate) use arena_shared::sim::{REACH_DOWN, REACH_UP, struck_surface};

impl Blade {
    /// How low the end of the blade comes (meters): its point or its edge's corner, whichever.
    fn lowest(&self, held: &GlobalTransform) -> f32 {
        self.at(held, 1.0).y.min(held.transform_point(self.corner).y)
    }

    /// The point `up` (a fraction of its length) up the blade, in the world (stretched as the
    /// blade is when it smears).
    fn at(&self, held: &GlobalTransform, up: f32) -> Vec3 {
        held.transform_point(Vec3::Y * self.length * up)
    }
}

#[derive(Resource)]
struct SwishAssets {
    mark: Handle<Mesh>,
    /// Brightest first.
    fades: Vec<Handle<PlainMaterial>>,
    chunk: Handle<Mesh>,
    wisp: Handle<Mesh>,
    wisp_glow: Handle<PlainMaterial>,
    /// A dark mote of the void, thrown off with the rasp's sparks.
    void_mote: Handle<PlainMaterial>,
    /// A scuff a dragging point leaves.
    scuff: Handle<Mesh>,
    /// Earth and stone, dark enough for any ground.
    chunks: [Handle<PlainMaterial>; 2],
    /// Embers flaking off a quickened blade.
    ember_glow: Handle<PlainMaterial>,
    /// A strike's mark lying along its lane, and its tear's layers, wherever they lie.
    lane_mark: (Handle<Mesh>, Handle<LyingMaterial>),
    tear: TearLooks,
}

fn load_swish(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<PlainMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut lying: ResMut<Assets<LyingMaterial>>,
) {
    let heights = LyingHeights::new(&mut images);
    commands.insert_resource(SwishAssets {
        lane_mark: (meshes.add(lane_mark_grid()), lying.add(LyingMaterial::new(AIR.with_alpha(MARK_ALPHA), MARK_HEIGHT, false, &heights, (&Handle::default(), UVec2::ONE, LyingShape::Line), 0.0))),
        tear: TearLooks::new(&mut meshes, &mut images, &mut lying, &heights),
        mark: meshes.add(mark_shape().build()),
        fades: fades(&mut materials, AIR, MARK_ALPHA, 2.0),
        chunk: meshes.add(Cuboid::new(1.0, 0.7, 0.85)),
        wisp: meshes.add(Sphere::new(1.0).mesh().ico(1).unwrap()),
        wisp_glow: materials.add(arena::glow(palette::WISP, CORE_GLOW)),
        void_mote: materials.add(arena::glow(palette::TATTERS, 0.0)),
        scuff: meshes.add(scuff_mesh()),
        chunks: [palette::HUNTER_DARK, palette::ASH].map(|color| materials.add(arena::matte(color))),
        ember_glow: materials.add(arena::glow(palette::SOUL, EMBER_GLOW)),
    });
    commands.insert_resource(heights);
}

/// A see-through `color` at `alpha`, fading out in `MARK_FADES` steps (one shared material each).
fn fades(materials: &mut Assets<PlainMaterial>, color: Color, alpha: f32, glow: f32) -> Vec<Handle<PlainMaterial>> {
    (0..MARK_FADES).map(|i| materials.add(arena::translucent(color, alpha * (1.0 - i as f32 / MARK_FADES as f32), glow))).collect()
}

/// The step of `fades` for `t` (0 to 1) of the way faded out.
fn faded(fades: &[Handle<PlainMaterial>], t: f32) -> &Handle<PlainMaterial> {
    &fades[((t * MARK_FADES as f32) as usize).min(MARK_FADES - 1)]
}

/// A pseudo-random number in 0..1 from `seed` and `i`: the same impact breaks the same way.
pub(crate) fn noise(seed: u32, i: u32) -> f32 {
    let mut x = seed.wrapping_mul(0x9E37_79B9) ^ i.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x & 0xFFFF) as f32 / 65_535.0
}
