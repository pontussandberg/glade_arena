//! A blade's wind: what a thrusting blade leaves in the air. A faint trail follows the blade
//! itself wherever it moves fast (a lunge, a dash), so it shows where the sword just was, twice as
//! bright while its Follow Through is on; and each strike leaves a mark, one thin, sharp line of
//! wind along the blade where it was as the strike landed, lingering a moment as it fades. Air
//! casts no shadow. A strike carries on into the ground (empowered or not): as the blade
//! comes down, the ground tears open all over its hitbox (its melee lane: the rectangle it
//! hits) onto the spectral realm beneath: the void showing dark through it, cracks edged in ink
//! with wisp light welling up out of them (flaring as it breaks, then glowing low and breathing),
//! a burst of spectral mist, and chunks of
//! earth thrown up and tumbling back (see-through and self-lit, so it reads on any ground). The
//! tear lingers a while, then closes and fades. A blade whose point drags along the ground (a
//! glaive carried trailing behind) scuffs a faint line in it, fading as it goes, and rasps: now
//! and then a fine streak of spectral light, or a dark mote of the void, skips off where it
//! scrapes. The same whoever swung (who it was shows in the
//! health bar).

use arena_shared::protocol::*;
use bevy::prelude::*;
use bevy::transform::TransformSystems;

use crate::arena::{self, palette};

mod trail;
pub(crate) use trail::*;
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
        app.add_systems(Startup, load_swish);
        app.add_systems(Update, (add_trails, drive_marks, drive_impacts, throw_chunks, burn_in_rifts));
        app.add_systems(PostUpdate, (trail_blades, show_marks, strike_ground, drag_marks, shed_embers).chain().after(TransformSystems::Propagate));
        app.add_systems(Update, (fade_scuffs, fly_sparks, drift_embers));
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
    /// past the shaft; a knife's is nearly all of it.
    pub(crate) from: f32,
    pub(crate) shown: u32,
    pub(crate) quickened: bool,
    /// A strike that has just gone off: the ground breaks over its hitbox this frame.
    pub(crate) striking: bool,
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
    /// How bright its trail is and how long it lingers, as shares of the usual (1): a knife's
    /// cut is all trail, so its trail is the brightest and lasts longest.
    pub(crate) vivid: f32,
    pub(crate) linger: f32,
}

/// A blade in a fighter's other hand, part of its lead forearm (the shade's reversed knife):
/// where it runs along the forearm's own axis (meters from the elbow, its base to its point),
/// for a trail of its own (as vivid and lingering as the main blade's), and the mark of the
/// attack's second cut (`AttackDef::twice`): when (seconds) the next one lands, and where its
/// mark goes (beside the first cut's), while one is coming.
#[derive(Component)]
pub(crate) struct OffBlade {
    pub(crate) held: Entity,
    pub(crate) span: (f32, f32),
    pub(crate) vivid: f32,
    pub(crate) linger: f32,
    pub(crate) cut: Option<(f32, Vec3, Vec3)>,
}

impl OffBlade {
    /// The point `up` (0 at its base, 1 at its point) along it, in the world.
    fn at(&self, held: &GlobalTransform, up: f32) -> Vec3 {
        held.transform_point(Vec3::Y * (self.span.0 + (self.span.1 - self.span.0) * up))
    }
}

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
    fades: Vec<Handle<StandardMaterial>>,
    trail: Handle<StandardMaterial>,
    dust: Handle<Mesh>,
    chunk: Handle<Mesh>,
    wisp: Handle<Mesh>,
    /// The cracks' ink, darkest first; their spectral light, the void under them and the mist,
    /// brightest first.
    rift_fades: Vec<Handle<StandardMaterial>>,
    core_fades: Vec<Handle<StandardMaterial>>,
    rim_fades: Vec<Handle<StandardMaterial>>,
    dust_fades: Vec<Handle<StandardMaterial>>,
    wisp_glow: Handle<StandardMaterial>,
    /// A dark mote of the void, thrown off with the rasp's sparks.
    void_mote: Handle<StandardMaterial>,
    /// A scuff a dragging point leaves, and its ink fading.
    scuff: Handle<Mesh>,
    scuff_fades: Vec<Handle<StandardMaterial>>,
    /// Earth and stone, dark enough for any ground.
    chunks: [Handle<StandardMaterial>; 2],
    /// Embers flaking off a quickened blade.
    ember_glow: Handle<StandardMaterial>,
}

fn load_swish(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(SwishAssets {
        mark: meshes.add(mark_mesh()),
        fades: fades(&mut materials, AIR, MARK_ALPHA, 2.0),
        trail: materials.add(arena::translucent(AIR, 0.47, 1.8)),
        dust: meshes.add(dust_mesh()),
        chunk: meshes.add(Cuboid::new(1.0, 0.7, 0.85)),
        wisp: meshes.add(Sphere::new(1.0).mesh().ico(1).unwrap()),
        // Layered up from the void: its outline, the cracks' ink, their light, the mist.
        rim_fades: fades(&mut materials, palette::TATTERS, RIM_ALPHA, 0.0),
        rift_fades: layered(&mut materials, palette::INK, RIFT_ALPHA, 0.0, 2),
        core_fades: layered(&mut materials, palette::WISP, CORE_ALPHA, CORE_GLOW, 3),
        dust_fades: layered(&mut materials, palette::WISP, 0.15, 1.5, 4),
        wisp_glow: materials.add(arena::glow(palette::WISP, CORE_GLOW)),
        void_mote: materials.add(arena::glow(palette::TATTERS, 0.0)),
        scuff: meshes.add(scuff_mesh()),
        scuff_fades: fades(&mut materials, palette::INK, SCUFF_ALPHA, 0.0),
        chunks: [palette::HUNTER_DARK, palette::ASH].map(|color| materials.add(arena::matte(color))),
        ember_glow: materials.add(arena::glow(palette::SOUL, EMBER_GLOW)),
    });
}

/// A see-through `color` at `alpha`, fading out in `MARK_FADES` steps (one shared material each).
fn fades(materials: &mut Assets<StandardMaterial>, color: Color, alpha: f32, glow: f32) -> Vec<Handle<StandardMaterial>> {
    layered(materials, color, alpha, glow, 0)
}

/// The step of `fades` (from `fades` or `layered`) for `t` (0 to 1) of the way faded out.
fn faded(fades: &[Handle<StandardMaterial>], t: f32) -> &Handle<StandardMaterial> {
    &fades[((t * MARK_FADES as f32) as usize).min(MARK_FADES - 1)]
}

/// `fades`, drawn over the see-through layers below it in the same place (a higher `layer`):
/// they'd otherwise sort however their distances happen to fall.
fn layered(materials: &mut Assets<StandardMaterial>, color: Color, alpha: f32, glow: f32, layer: u8) -> Vec<Handle<StandardMaterial>> {
    (0..MARK_FADES)
        .map(|i| materials.add(StandardMaterial { depth_bias: layer as f32, ..arena::translucent(color, alpha * (1.0 - i as f32 / MARK_FADES as f32), glow) }))
        .collect()
}

/// A pseudo-random number in 0..1 from `seed` and `i`: the same impact breaks the same way.
pub(crate) fn noise(seed: u32, i: u32) -> f32 {
    let mut x = seed.wrapping_mul(0x9E37_79B9) ^ i.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x & 0xFFFF) as f32 / 65_535.0
}
