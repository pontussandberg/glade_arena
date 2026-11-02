//! The mark a strike leaves: one thin line of wind along its lane, lying on what it comes down on
//! (in pieces, one on each surface), or along the blade in the air for a strike with no lane;
//! lingering a moment as it fades.

use super::*;

use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::MeshTag;

use crate::arena::{FlatMesh, palette};
use crate::plain::PlainMaterial;

/// The mark a strike leaves: how long it lasts (seconds; it runs from where the blade starts to
/// its tip, `Blade::from`), and how many steps it fades in (one shared material each).
const MARK_LIFE: f32 = 0.32;
/// How high over what it lies on a mark lies along its strike's lane (meters): over the tear.
pub(super) const MARK_HEIGHT: f32 = 0.05;
pub(super) const MARK_FADES: usize = 8;
/// The air's color, and how see-through the mark is at its brightest.
pub(super) const AIR: Color = palette::SILVER;
pub(super) const MARK_ALPHA: f32 = 0.85;

/// How wide the mark is (meters, from its spine out to either edge) at its widest.
pub(super) const MARK_HALF_WIDTH: f32 = 0.025;

/// The mark's shape: one thin, sharp line of wind along +X, 1 long, in two crossed planes (flat
/// and upright, so it shows from any angle): tapering to a needle point at its far end and fading
/// out at its near one, a bright core down its middle (as `lying.wgsl` draws it along a lane).
pub(super) fn mark_shape() -> FlatMesh {
    const STEPS: usize = 12;
    let mut b = FlatMesh::default();
    // Widest a little short of the tip, a needle point at it; clear at the near end.
    let width = |t: f32| MARK_HALF_WIDTH * (t / 0.17).min(1.0) * ((1.0 - t) / 0.23).min(1.0).powf(0.6);
    let alpha = |t: f32| (t / 0.35).min(1.0).powf(1.5);
    for across in [Vec3::Z, Vec3::Y] {
        for i in 0..STEPS {
            let (t0, t1) = (i as f32 / STEPS as f32, (i + 1) as f32 / STEPS as f32);
            let (p0, p1) = (Vec3::X * t0, Vec3::X * t1);
            let (w0, w1) = (across * width(t0), across * width(t1));
            let (a0, a1) = (alpha(t0), alpha(t1));
            // Bright along its spine, clear at its edges.
            for side in [1.0, -1.0] {
                b.tri_faded_both([p0, p0 + w0 * side, p1], [a0, 0.0, a1]);
                b.tri_faded_both([p1, p0 + w0 * side, p1 + w1 * side], [a1, 0.0, 0.0]);
            }
        }
    }
    b
}

/// The grid a mark lying along a lane is drawn on (`lying_grid`), 1 long and as wide as the mark,
/// flat and upright: how many cells it has along it, to follow what it lies on the longest lane.
const MARK_CELLS: u32 = 64;

pub(super) fn lane_mark_grid() -> Mesh {
    lying_grid(Vec2::new(0.0, -MARK_HALF_WIDTH), Vec2::new(1.0, MARK_HALF_WIDTH), UVec2::new(MARK_CELLS, 1), true)
}

/// Where a strike's mark is drawn.
pub(super) enum Lies {
    /// Along its lane, from its frame (`Underfoot::frame`), this long: lying on what's under it
    /// (`lying`).
    Lane(Transform, f32),
    /// In the air, from the blade's base to its tip as the strike landed.
    Air(Vec3, Vec3),
}

/// A strike's mark, from `started` (seconds).
#[derive(Component)]
pub(super) struct Mark {
    pub(super) started: f32,
    pub(super) lies: Lies,
}

impl Mark {
    /// Where it stands `t` (0..1) of the way through: drawn on out to the tip in an instant, then
    /// lingering, thinning as it fades. Along its lane, stretched so from its frame (`lying_at`).
    pub(super) fn show(&self, t: f32) -> Transform {
        let drawn = 0.4 + 0.6 * (t / 0.12).min(1.0);
        let thin = 1.0 - 0.6 * t;
        match self.lies {
            Lies::Lane(frame, length) => lying_at(frame, Vec3::new(length * drawn, thin, thin)),
            Lies::Air(from, to) => {
                let along = to - from;
                Transform::from_translation(from).with_rotation(Quat::from_rotation_arc(Vec3::X, along.normalize_or(Vec3::X))).with_scale(Vec3::new(along.length() * drawn, thin, thin))
            }
        }
    }
}

/// A mark for each new swing, lying along the middle of the strike's lane (`AttackDef::lane`,
/// longer empowered) on what the blade comes down on there (`Underfoot`), wherever the blade
/// itself happens to be as the hit arrives: in pieces where the lane runs over steps and edges,
/// fallen on down onto whatever's below a drop. Without a lane, along the blade where it
/// is (after it's posed and placed). `LastSwing` comes from the server; each is drawn once.
#[allow(clippy::too_many_arguments)]
pub(super) fn show_marks(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<SwishAssets>,
    mut heights: ResMut<LyingHeights>,
    map: Res<crate::rooms::CurrentMap>,
    mut swings: Query<(&LastSwing, &mut Blade, &Pos, &ClassId, (Option<&Motion>, Option<&crate::render::PredictedWalk>)), Changed<LastSwing>>,
    blades: Query<&GlobalTransform>,
) {
    let now = time.elapsed_secs();
    for (swing, mut blade, pos, class, (motion, predicted)) in &mut swings {
        if swing.tick <= blade.shown {
            continue;
        }
        blade.shown = swing.tick;
        let dir = swing.dir.normalize_or(Vec2::X);
        let feet = crate::render::shown_height(map.map(), motion, predicted, pos.0);
        let lane = class.def().attack.lane_grown(swing.empowered, swing.grown);
        // Along its lane, as far aside of the striker as it ran (onto the crosshair's line).
        let under = Underfoot::new(map.map(), swing.lane_from(pos.0), dir, feet);
        // The ground breaks over its hitbox at once, as the hit is dealt (`strike_ground`), on
        // what's under it there.
        (blade.striking, blade.struck) = (true, lane.map(|_| under.clone()));
        match lane {
            // Laid on what's under it all along its lane (none if there's no room in the heights).
            Some((near, far, _)) => {
                let mut under = under;
                let reach = under.heights(Vec2::new(near, -MARK_HALF_WIDTH), Vec2::new(far, MARK_HALF_WIDTH));
                let Some(slot) = heights.keep(&reach, near, near) else { continue };
                let tag = lying_tag((&slot, 0, false), 1.0, 1.0);
                draw(&mut commands, now, Lies::Lane(under.frame(), far - near), assets.lane_mark.0.clone()).insert((MeshMaterial3d(assets.lane_mark.1.clone()), tag, slot));
            }
            None => {
                let Ok(held) = blades.get(blade.held) else { continue };
                draw(&mut commands, now, Lies::Air(blade.at(held, blade.from), blade.at(held, 1.0)), assets.mark.clone()).insert(MeshMaterial3d(assets.fades[0].clone()));
            }
        }
    }
}

/// A strike's mark, from `started` (seconds), where it `lies`, drawn as `mesh` (its material yet to
/// come).
fn draw<'a>(commands: &'a mut Commands, started: f32, lies: Lies, mesh: Handle<Mesh>) -> EntityCommands<'a> {
    let mark = Mark { started, lies };
    let transform = mark.show(0.0);
    commands.spawn((Mesh3d(mesh), transform, GlobalTransform::from(transform), mark, NoFrustumCulling, NotShadowCaster, NotShadowReceiver))
}

/// Marks draw on, linger, thin and fade, and are gone: in the air, in steps of one shared
/// material each; along a lane, by its tag (`lying_tag`).
pub(super) fn drive_marks(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<SwishAssets>,
    mut marks: Query<(Entity, &Mark, &mut Transform, Option<&mut MeshMaterial3d<PlainMaterial>>, Option<(&mut MeshTag, &HeightsSlot)>)>,
) {
    let now = time.elapsed_secs();
    for (entity, mark, mut transform, material, lying) in &mut marks {
        let t = (now - mark.started) / MARK_LIFE;
        if t >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }
        transform.set_if_neq(mark.show(t));
        if let Some((mut tag, slot)) = lying {
            tag.set_if_neq(lying_tag((slot, 0, false), 1.0, 1.0 - t));
        }
        if let Some(mut material) = material {
            let fade = faded(&assets.fades, t);
            if material.0 != *fade {
                material.0 = fade.clone();
            }
        }
    }
}
