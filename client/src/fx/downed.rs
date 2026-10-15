//! Where a downed fighter lies: spirit light pooled on the ground round them, veins of it creeping
//! out as an ally picks them up (as far as the rescue has got), and wisps of it rising off them,
//! so they're found from afar.

use super::*;

use crate::swish::noise;

/// A downed fighter's mark: its veins (`veins`, scaled to the rescue) and when it last let a wisp
/// rise.
#[derive(Component)]
pub(super) struct DownMark {
    of: Entity,
    veins: Entity,
    wisp_at: f32,
}

/// How often a wisp rises off the downed (seconds).
const WISP_EVERY: f32 = 0.12;

/// Marks every downed fighter where they lie, the veins growing with their rescue; the mark goes
/// when they're back up.
pub(super) fn mark_downed(
    mut commands: Commands,
    time: Res<Time>,
    art: Res<FxArt>,
    downed: Query<(Entity, &Pos, &Downed)>,
    mut marks: Query<(Entity, &mut DownMark, &mut Transform)>,
    mut veins: Query<&mut Transform, Without<DownMark>>,
) {
    let now = time.elapsed_secs();
    for (mark, mut down, mut transform) in &mut marks {
        let Ok((_, pos, downed)) = downed.get(down.of) else {
            commands.entity(mark).despawn();
            continue;
        };
        let at = arena::to_world(pos.0, 0.0);
        transform.translation = at;
        if let Ok(mut veins) = veins.get_mut(down.veins) {
            let r = REVIVE_RADIUS * (0.15 + 0.85 * downed.rescue);
            veins.scale = Vec3::new(r, 1.0, r);
        }
        if now >= down.wisp_at {
            down.wisp_at = now + WISP_EVERY;
            let k = (now * 1000.0) as u32;
            let (angle, out) = (noise(k, 1) * std::f32::consts::TAU, 0.5 * noise(k, 2));
            let from = at + Vec3::new(angle.cos() * out, 0.2, angle.sin() * out);
            let rise = Motion::Thrown { velocity: Vec3::Y * (1.2 + noise(k, 3)), gravity: -0.3, tumble: Vec3::ZERO };
            put(&mut commands, &art, &art.mote, Part::new(now, [0.15, 0.9, 1.6], Ink::Spirit, from).size(0.06 + 0.05 * noise(k, 4)).motion(rise).flicker(0.3).shrink(0.2));
        }
    }
    for (fighter, pos, _) in &downed {
        if marks.iter().any(|(_, mark, _)| mark.of == fighter) {
            continue;
        }
        let pool = Transform::from_xyz(0.0, 0.03, 0.0).with_scale(Vec3::new(REVIVE_RADIUS, 1.0, REVIVE_RADIUS));
        let pool = commands.spawn((Mesh3d(art.splotch[0].clone()), MeshMaterial3d(art.ink(Ink::SpiritMist, 0.0).clone()), pool, NotShadowCaster, NotShadowReceiver)).id();
        let veins = commands
            .spawn((Mesh3d(art.frost[1].clone()), MeshMaterial3d(art.ink(Ink::Spirit, 0.0).clone()), Transform::from_xyz(0.0, 0.04, 0.0), NotShadowCaster, NotShadowReceiver))
            .id();
        commands
            .spawn((DownMark { of: fighter, veins, wisp_at: 0.0 }, Transform::from_translation(arena::to_world(pos.0, 0.0)), Visibility::Inherited))
            .add_children(&[pool, veins]);
    }
}
