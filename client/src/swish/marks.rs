//! The mark a strike leaves: one thin line of wind along its lane, lying on what it comes down on
//! (in pieces, one on each surface), or along the blade in the air for a strike with no lane;
//! lingering a moment as it fades.

use super::*;

use arena_shared::config::TICK_HZ;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::{NotShadowCaster, NotShadowReceiver};

use crate::arena::{FlatMesh, palette};
use crate::plain::PlainMaterial;

/// The mark a strike leaves: how long it lasts (seconds; it runs from where the blade starts to
/// its tip, `Blade::from`), and how many steps it fades in (one shared material each).
const MARK_LIFE: f32 = 0.32;
/// How high over what it lies on a mark lies along its strike's lane (meters): over the tear.
const MARK_HEIGHT: f32 = 0.05;
pub(super) const MARK_FADES: usize = 8;
/// The air's color, and how see-through the mark is at its brightest.
pub(super) const AIR: Color = palette::SILVER;
pub(super) const MARK_ALPHA: f32 = 0.85;

/// The mark's shape: one thin, sharp line of wind along +X, 1 long, in two crossed planes (flat
/// and upright, so it shows from any angle): tapering to a needle point at its far end and fading
/// out at its near one, a bright core down its middle. Both faces.
pub(super) fn mark_shape() -> FlatMesh {
    const STEPS: usize = 12;
    const HALF_WIDTH: f32 = 0.025;
    let mut b = FlatMesh::default();
    // Widest a little short of the tip, a needle point at it; clear at the near end.
    let width = |t: f32| HALF_WIDTH * (t / 0.17).min(1.0) * ((1.0 - t) / 0.23).min(1.0).powf(0.6);
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

/// How long (meters) the sides of the triangles a mark lying along a lane is cut into are at
/// most, to follow what it lies on.
const MARK_EDGE: f32 = 0.05;

/// Where a strike's mark lies along its lane: over the surfaces `under` it, `near` to `far` out
/// along it, `beside` its middle (to its right).
pub(crate) struct LaneMark {
    pub(super) under: Underfoot,
    pub(super) near: f32,
    pub(super) far: f32,
    pub(super) beside: f32,
}

/// Where a strike's mark is drawn.
pub(super) enum Lies {
    /// Along its lane, on what's under it.
    Laid(LaneMark, Laid),
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
    /// Shown `t` (0..1) of the way through: drawn on out to the tip in an instant, then
    /// lingering, thinning as it fades. Along its lane it's laid again on what's under it (and
    /// stands where it's laid); in the air, it's where it stands.
    pub(super) fn show(&mut self, meshes: &mut Assets<Mesh>, t: f32) -> Transform {
        let drawn = 0.4 + 0.6 * (t / 0.12).min(1.0);
        let thin = 1.0 - 0.6 * t;
        match &mut self.lies {
            Lies::Laid(lane, laid) => {
                // Thinning a step at a time, with its fade (`faded`), so it's laid again only that
                // often, not every frame: a crowd strikes all the time.
                let thin = 1.0 - 0.6 * (t * MARK_FADES as f32).floor() / MARK_FADES as f32;
                let from = Vec3::new(lane.near, 0.0, lane.beside);
                laid.lay(meshes, &mut lane.under, 0, from, Vec3::new((lane.far - lane.near) * drawn, thin, thin));
                Transform::IDENTITY
            }
            Lies::Air(from, to) => {
                let along = *to - *from;
                Transform::from_translation(*from).with_rotation(Quat::from_rotation_arc(Vec3::X, along.normalize_or(Vec3::X))).with_scale(Vec3::new(along.length() * drawn, thin, thin))
            }
        }
    }
}

/// A mark for each new swing, lying along the middle of the strike's lane (`AttackDef::lane`,
/// longer empowered) on what the blade comes down on there (`Underfoot`), wherever the blade
/// itself happens to be as the hit arrives: in pieces where the lane runs over steps and edges,
/// fallen on down onto whatever's below a drop. Without a lane, along the blade where it
/// is (after it's posed and placed). `LastSwing` comes from the server; each is drawn once. An
/// attack that cuts twice (`AttackDef::twice`) marks its second cut too, beside the first along
/// the same lane (or along the other hand's blade, `OffBlade`) as the server lands it.
#[allow(clippy::too_many_arguments)]
pub(super) fn show_marks(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<SwishAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    map: Res<crate::rooms::CurrentMap>,
    mut swings: Query<(Entity, &LastSwing, &mut Blade, &Pos, &ClassId, (Option<&Motion>, Option<&crate::render::PredictedWalk>)), Changed<LastSwing>>,
    mut offhands: Query<&mut OffBlade>,
    blades: Query<&GlobalTransform>,
) {
    let now = time.elapsed_secs();
    let mut draw = |lies: Lies, meshes: &mut Assets<Mesh>| {
        let mesh = match &lies {
            Lies::Laid(_, laid) => laid.mesh.clone(),
            Lies::Air(..) => assets.mark.clone(),
        };
        let mut mark = Mark { started: now, lies };
        let transform = mark.show(meshes, 0.0);
        commands.spawn((Mesh3d(mesh), MeshMaterial3d(assets.fades[0].clone()), transform, GlobalTransform::from(transform), mark, NoFrustumCulling, NotShadowCaster, NotShadowReceiver));
    };
    let laid = |meshes: &mut Assets<Mesh>, lane: LaneMark| Lies::Laid(lane, Laid::new(meshes, vec![mark_shape()], MARK_HEIGHT, MARK_EDGE));
    for (entity, swing, mut blade, pos, class, (motion, predicted)) in &mut swings {
        if swing.tick <= blade.shown {
            continue;
        }
        blade.shown = swing.tick;
        let dir = swing.dir.normalize_or(Vec2::X);
        let feet = crate::render::shown_height(map.map(), motion, predicted, pos.0);
        let lane = class.def().attack.lane_grown(swing.empowered, swing.grown);
        let under = Underfoot::new(map.map(), pos.0, dir, feet);
        // The ground breaks over its hitbox at once, as the hit is dealt (`strike_ground`), on
        // what's under it there.
        (blade.striking, blade.struck) = (true, lane.map(|_| under.clone()));
        let lies = match lane {
            Some((near, far, _)) => laid(&mut meshes, LaneMark { under: under.clone(), near, far, beside: 0.0 }),
            None => {
                let Ok(held) = blades.get(blade.held) else { continue };
                Lies::Air(blade.at(held, blade.from), blade.at(held, 1.0))
            }
        };
        // The second cut's mark, a beat later, beside the first's along the same lane (the
        // other hand cuts the other way across it), or along that blade where it is then.
        if let Some(gap) = class.def().attack.twice
            && let Ok(mut off) = offhands.get_mut(entity)
        {
            let at = now + gap as f32 / TICK_HZ as f32;
            off.cut = Some((at, lane.map(|(near, far, width)| LaneMark { under, near, far, beside: -width * SECOND_CUT_BESIDE })));
        }
        draw(lies, &mut meshes);
    }
    for mut off in &mut offhands {
        if off.cut.as_ref().is_some_and(|(at, _)| now >= *at)
            && let Some((_, lane)) = off.cut.take()
        {
            let lies = match lane {
                Some(lane) => laid(&mut meshes, lane),
                None => {
                    let Ok(held) = blades.get(off.held) else { continue };
                    Lies::Air(off.at(held, 0.0), off.at(held, 1.0))
                }
            };
            draw(lies, &mut meshes);
        }
    }
}

/// How far to the side of the lane's middle (a share of its width) a second cut's mark lies.
const SECOND_CUT_BESIDE: f32 = 0.3;

/// Marks draw on, linger, thin and fade, and are gone.
pub(super) fn drive_marks(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<SwishAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut marks: Query<(Entity, &mut Mark, &mut Transform, &mut MeshMaterial3d<PlainMaterial>)>,
) {
    let now = time.elapsed_secs();
    for (entity, mut mark, mut transform, mut material) in &mut marks {
        let t = (now - mark.started) / MARK_LIFE;
        if t >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }
        let shown = mark.show(&mut meshes, t);
        transform.set_if_neq(shown);
        let fade = faded(&assets.fades, t);
        if material.0 != *fade {
            material.0 = fade.clone();
        }
    }
}
