//! The mark a strike leaves in the air: one thin line of wind along the blade where it landed,
//! lingering a moment as it fades.

use super::*;

use arena_shared::config::TICK_HZ;
use bevy::light::{NotShadowCaster, NotShadowReceiver};

use crate::arena::{FlatMesh, palette, to_world};

/// The mark a strike leaves: how long it lasts (seconds; it runs from where the blade starts to
/// its tip, `Blade::from`), and how many steps it fades in (one shared material each).
const MARK_LIFE: f32 = 0.32;
/// How high over the ground a mark lies along its strike's lane (meters): over the tear.
const MARK_HEIGHT: f32 = 0.05;
pub(super) const MARK_FADES: usize = 8;
/// The air's color, and how see-through the mark is at its brightest.
pub(super) const AIR: Color = palette::SILVER;
pub(super) const MARK_ALPHA: f32 = 0.85;

/// The mark: one thin, sharp line of wind along +X, 1 long, in two crossed planes (flat and
/// upright, so it shows from any angle): tapering to a needle point at its far end and fading out
/// at its near one, a bright core down its middle. Both faces.
pub(super) fn mark_mesh() -> Mesh {
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
    b.build()
}

/// A strike's mark, from `started` (seconds): drawn `from` the blade's base to its tip as the
/// strike landed.
#[derive(Component)]
pub(super) struct Mark {
    pub(super) started: f32,
    pub(super) from: Vec3,
    pub(super) to: Vec3,
}

impl Mark {
    /// Where it is `t` (0..1) of the way through: drawn on out to the tip in an instant, then
    /// lingering, thinning as it fades.
    pub(super) fn transform(&self, t: f32) -> Transform {
        let drawn = (t / 0.12).min(1.0);
        let thin = 1.0 - 0.6 * t;
        let along = self.to - self.from;
        Transform::from_translation(self.from)
            .with_rotation(Quat::from_rotation_arc(Vec3::X, along.normalize_or(Vec3::X)))
            .with_scale(Vec3::new(along.length() * (0.4 + 0.6 * drawn), thin, thin))
    }
}

/// A mark for each new swing, lying on the ground where the blade comes down: along the middle
/// of the strike's lane (`AttackDef::lane`, longer empowered), wherever the blade itself happens
/// to be as the hit arrives. Without a lane, along the blade where it is (after it's posed and
/// placed). `LastSwing` comes from the server; each is drawn once. An attack that cuts twice
/// (`AttackDef::twice`) marks its second cut too, along the other hand's blade (`OffBlade`) as
/// the server lands it.
pub(super) fn show_marks(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<SwishAssets>,
    mut swings: Query<(Entity, &LastSwing, &mut Blade, &Pos, &ClassId), Changed<LastSwing>>,
    mut offhands: Query<&mut OffBlade>,
    blades: Query<&GlobalTransform>,
) {
    let now = time.elapsed_secs();
    let mut draw = |mark: Mark| {
        let transform = mark.transform(0.0);
        commands.spawn((
            Mesh3d(assets.mark.clone()),
            MeshMaterial3d(assets.fades[0].clone()),
            transform,
            GlobalTransform::from(transform),
            mark,
            NotShadowCaster,
            NotShadowReceiver,
        ));
    };
    for (entity, swing, mut blade, pos, class) in &mut swings {
        if swing.tick <= blade.shown {
            continue;
        }
        blade.shown = swing.tick;
        // The ground breaks over its hitbox at once, as the hit is dealt (`strike_ground`): it
        // shows where the strike hit, cut short or not.
        blade.striking = true;
        if let Some(gap) = class.def().attack.twice
            && let Ok(mut off) = offhands.get_mut(entity)
        {
            off.cut_at = Some(now + gap as f32 / TICK_HZ as f32);
        }
        let mark = match class.def().attack.lane_grown(swing.empowered, swing.grown) {
            Some((near, far, _)) => {
                let dir = swing.dir.normalize_or(Vec2::X);
                Mark { started: now, from: to_world(pos.0 + dir * near, MARK_HEIGHT), to: to_world(pos.0 + dir * far, MARK_HEIGHT) }
            }
            None => {
                let Ok(held) = blades.get(blade.held) else { continue };
                Mark { started: now, from: blade.at(held, blade.from), to: blade.at(held, 1.0) }
            }
        };
        draw(mark);
    }
    for mut off in &mut offhands {
        if off.cut_at.is_some_and(|at| now >= at) {
            off.cut_at = None;
            let Ok(held) = blades.get(off.held) else { continue };
            draw(Mark { started: now, from: off.at(held, 0.0), to: off.at(held, 1.0) });
        }
    }
}

/// Marks draw on, linger, thin and fade, and are gone.
pub(super) fn drive_marks(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<SwishAssets>,
    mut marks: Query<(Entity, &Mark, &mut Transform, &mut MeshMaterial3d<StandardMaterial>)>,
) {
    let now = time.elapsed_secs();
    for (entity, mark, mut transform, mut material) in &mut marks {
        let t = (now - mark.started) / MARK_LIFE;
        if t >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }
        *transform = mark.transform(t);
        let fade = faded(&assets.fades, t);
        if material.0 != *fade {
            material.0 = fade.clone();
        }
    }
}
