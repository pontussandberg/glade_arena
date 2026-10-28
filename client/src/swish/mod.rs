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

use arena_shared::map::Map;
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
    /// A strike that has just gone off: the ground breaks over its hitbox this frame, over the
    /// stretch of it the blade meets (`struck_stretch`), if it meets any.
    pub(crate) striking: bool,
    pub(crate) struck: Option<[(f32, f32); 2]>,
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

/// How far below a striker's feet (meters) its blade still reaches the ground coming down (down
/// a slope, over a drop), and how far above them it meets it (up a slope, on a step): past the
/// one it strikes only air, and the other stops it short.
pub(crate) const REACH_DOWN: f32 = 0.6;
pub(crate) const REACH_UP: f32 = 1.2;

/// What a blade coming down at `p` meets, for a striker with its feet `feet` high: the ground,
/// or the top of something standing on it, as high up as the blade comes from.
pub(crate) fn struck_surface(map: &Map, p: Vec2, feet: f32) -> f32 {
    map.surface_at(p, feet + REACH_UP)
}

/// How far apart (meters) a strike's lane is looked along for the ground it meets; how much
/// the ground may change from one look to the next and still be one stretch of it (more is a
/// step); and how far off the straight line between a stretch's ends it may lie and the tear
/// still lie flat on it.
const LANE_STEP: f32 = 0.2;
const LANE_JUMP: f32 = 0.25;
const LANE_BEND: f32 = 0.08;

/// Where along a strike's lane, `near` to `far` (meters out along `dir` from `pos`), its blade
/// meets the ground, for a striker with its feet `feet` high: the longest straight stretch of
/// ground within its reach (`REACH_DOWN`, `REACH_UP`), as how far along the lane each end of it is
/// and how high the ground is there. Up a slope or onto a step it meets what rises; down one or
/// out over a drop only as far as it reaches: nothing if the whole lane is out of its reach.
pub(crate) fn struck_stretch(map: &Map, pos: Vec2, dir: Vec2, feet: f32, near: f32, far: f32) -> Option<[(f32, f32); 2]> {
    let n = (((far - near) / LANE_STEP).ceil() as usize).max(1);
    let along = |i: usize| near + (far - near) * i as f32 / n as f32;
    let reached = |i: usize| Some(struck_surface(map, pos + dir * along(i), feet)).filter(|h| (-REACH_DOWN..=REACH_UP).contains(&(h - feet)));
    // Unbroken runs of looks: in reach, no step from one to the next; each split where it
    // bends (`straight`).
    let (mut best, mut run, mut from) = (None, Vec::<f32>::new(), 0);
    for i in 0..=n + 1 {
        let h = if i <= n { reached(i) } else { None };
        if !matches!((h, run.last()), (Some(h), Some(&was)) if (h - was).abs() <= LANE_JUMP) {
            if run.len() > 1 {
                straight(&run, from, &mut best);
            }
            run.clear();
            from = i;
        }
        run.extend(h);
    }
    best.map(|(a, b, low, high)| [(along(a), low), (along(b), high)])
}

/// The pieces of a run of the ground's `heights` along a lane (from look `from` on, two or more),
/// split where it bends away from the straight line between its ends by more than `LANE_BEND`
/// until each lies on one: the longest of them (the nearest of the longest) kept in `best`, as
/// its first and last looks and how high the ground is at each.
fn straight(heights: &[f32], from: usize, best: &mut Option<(usize, usize, f32, f32)>) {
    let last = heights.len() - 1;
    let (low, high) = (heights[0], heights[last]);
    let off = |i: usize| (heights[i] - (low + (high - low) * i as f32 / last as f32)).abs();
    match (1..last).max_by(|&i, &j| off(i).total_cmp(&off(j))) {
        Some(k) if off(k) > LANE_BEND => {
            straight(&heights[..=k], from, best);
            straight(&heights[k..], from + k, best);
        }
        _ if best.is_none_or(|(a, b, ..)| last > b - a) => *best = Some((from, from + last, low, high)),
        _ => {}
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

#[cfg(test)]
mod tests {
    use super::*;
    use arena_shared::map::{CRATE, MapId};

    /// Where along the yard's lane from `pos` along `dir` a blade struck by a striker with its
    /// feet `feet` high meets the ground, `near` to `far` out.
    fn struck(pos: Vec2, dir: Vec2, feet: f32, near: f32, far: f32) -> Option<[(f32, f32); 2]> {
        struck_stretch(MapId::Yard.map(), pos, dir, feet, near, far)
    }

    #[test]
    fn on_level_ground_the_whole_lane_is_struck() {
        assert_eq!(struck(Vec2::new(-34.5, -0.5), Vec2::X, 0.0, 0.5, 3.0), Some([(0.5, 0.0), (3.0, 0.0)]));
    }

    #[test]
    fn struck_at_a_terraces_face_it_stops_at_the_foot_of_it() {
        // The face is at y = 10, out of reach over the lane's ground.
        let [(from, low), (to, high)] = struck(Vec2::new(-17.5, 8.5), Vec2::Y, 0.0, 0.5, 3.5).unwrap();
        assert_eq!((from, low, high), (0.5, 0.0, 0.0));
        assert!(to < 1.5 && to > 1.5 - LANE_STEP * 1.5, "struck to {to} m along, the face is 1.5 m out");
    }

    #[test]
    fn struck_out_over_a_drop_it_stops_at_the_edge() {
        // The south terrace's north edge is at y = -9, with the square far below it.
        let [(from, low), (to, high)] = struck(Vec2::new(2.5, -11.0), Vec2::Y, 1.5, 0.5, 3.5).unwrap();
        assert_eq!((from, low, high), (0.5, 1.5, 1.5));
        assert!(to < 2.0 && to > 1.0, "struck to {to} m along, the edge is 2 m out");
        // Struck from just past the edge, nothing on the lane is in reach.
        assert_eq!(struck(Vec2::new(2.5, -11.0), Vec2::Y, 1.5, 2.2, 3.5), None);
    }

    #[test]
    fn struck_up_a_ramp_it_lies_on_the_slope_as_far_as_it_reaches() {
        // The terrace's west stair rises north from y = 6.25, two thirds of a meter a meter.
        let [(from, low), (to, high)] = struck(Vec2::new(-9.5, 5.0), Vec2::Y, 0.0, 0.5, 3.5).unwrap();
        assert!((from - 1.25).abs() <= LANE_STEP, "struck from {from} m along, the ramp's foot is 1.25 m out");
        assert!(((high - low) / (to - from) - 2.0 / 3.0).abs() < 0.05, "struck at a slope of {}", (high - low) / (to - from));
        assert!(high <= REACH_UP && high > REACH_UP - 0.2, "struck up to {high} m, it reaches {REACH_UP} m");
    }

    #[test]
    fn struck_over_a_crate_it_meets_the_crate_top() {
        // The lane runs onto the lowest of the stack, a crate high, across most of the lane.
        let [(from, low), (to, high)] = struck(Vec2::new(15.5, 0.0), Vec2::Y, 0.0, 0.5, 2.0).unwrap();
        assert_eq!((low, high, to), (CRATE, CRATE, 2.0));
        assert!(from > 0.6 && from < 1.0, "struck the crate from {from} m along, its edge is 1 m out");
    }

    #[test]
    fn struck_off_a_high_place_it_strikes_only_air() {
        // West off the watchtower's top, far over the ground.
        assert_eq!(struck(Vec2::new(-11.5, -9.0), Vec2::NEG_X, 6.0, 1.0, 3.0), None);
    }

    #[test]
    fn the_longest_straight_piece_of_a_run_is_kept() {
        let mut best = None;
        // Flat, then a long even rise, then a short flat top.
        let heights = [0.0, 0.0, 0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.6];
        straight(&heights, 3, &mut best);
        assert_eq!(best, Some((5, 11, 0.0, 0.6)));
        // Two even halves: the nearer one.
        let mut best = None;
        straight(&[0.0, 0.0, 0.0, 0.5, 0.5, 0.5], 0, &mut best);
        assert_eq!(best.map(|(a, b, ..)| (a, b)), Some((0, 2)));
    }
}
