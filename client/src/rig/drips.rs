//! A primed weapon's light dripping (Deadeye): while a fighter's next attack is primed, pale drops
//! of its aura's light gather on its held blade's edge and fall a little way, dwindling to nothing
//! long before the ground; and off the primed javelin as it flies, falling behind it.

use super::*;

use crate::swish::noise;

/// Drops dripping off a primed blade: how many a second, how hard they're pulled down (m/s²), how
/// long each lasts (seconds, least and most), how big (meters, least and most), how drawn out
/// along their fall, and how bright.
const DRIPS_PER_SECOND: f32 = 13.0;
const DRIP_PULL: f32 = 1.4;
const DRIP_LIFE: (f32, f32) = (0.35, 0.65);
const DRIP_SIZE: (f32, f32) = (0.012, 0.022);
/// How drawn out a drop is as it forms and as it ends (its length to its width), and how thin.
const DRIP_STRETCH: (f32, f32) = (3.0, 5.5);
const DRIP_THIN: f32 = 0.55;
/// A primed javelin in flight drips faster, so its drops don't fall meters apart.
const FLYING_DRIPS_PER_SECOND: f32 = 58.0;
const DRIP_GLOW: f32 = 5.0;

/// The drops' shape and light.
#[derive(Resource)]
pub(super) struct DripAssets {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
}

pub(super) fn load_drips(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(DripAssets {
        mesh: meshes.add(Sphere::new(1.0).mesh().ico(1).unwrap()),
        material: materials.add(arena::glow(arena::palette::DEADEYE, DRIP_GLOW)),
    });
}

/// A drop of light off a primed blade, from `started`, falling at `speed` (m/s).
#[derive(Component)]
pub(super) struct Drip {
    started: f32,
    speed: f32,
    size: f32,
    life: f32,
}

/// A fighter whose next attack is primed, its aura lit and a blade edge to drip from, sheds drops
/// one after another from anywhere along that edge.
pub(super) fn shed_drips(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<DripAssets>,
    mut fighters: Query<(&mut Rig, &AttackState, &InheritedVisibility)>,
    helds: Query<&GlobalTransform>,
) {
    let now = time.elapsed_secs();
    for (mut rig, attack, visible) in &mut fighters {
        let edge = rig.gleam.map(|(_, edge)| edge);
        let (Some(edge), Some(_), true, true) = (edge, rig.aura, attack.primed.is_some(), visible.get()) else {
            rig.drips.0 = now;
            continue;
        };
        let Ok(held) = helds.get(rig.held) else { continue };
        while rig.drips.0 <= now {
            let shed = rig.drips.1;
            rig.drips = (rig.drips.0 + 1.0 / DRIPS_PER_SECOND, shed.wrapping_add(1));
            let r = |k: u32| noise(shed.wrapping_mul(0x51ED_270B) ^ rig.held.index().index(), k);
            // Either edge of the blade (the left mirrors the right across Z).
            let side = if r(0) < 0.5 { 1.0 } else { -1.0 };
            let from = held.transform_point(edge(0.1 + 0.85 * r(1)) * Vec3::new(1.0, 1.0, side));
            spawn_drip(&mut commands, &assets, from, now, (r(2), r(3)));
        }
    }
}

/// A primed javelin in flight (`Projectile::crit`): it drips as it goes, from where it's drawn
/// (`render::add_visuals` gives it this), so its drops fall behind it along its flight. When
/// (seconds) the next falls, and how many have.
#[derive(Component, Default)]
pub(crate) struct ShotDrips(f32, u32);

/// A primed javelin in flight sheds drops off its blade's edges, one after another.
pub(super) fn drip_from_shots(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<DripAssets>,
    mut shots: Query<(Entity, &GlobalTransform, &InheritedVisibility, &mut ShotDrips)>,
) {
    let now = time.elapsed_secs();
    for (shot, at, visible, mut drips) in &mut shots {
        // Not yet let go of (hidden): nothing falls from the hand.
        if !visible.get() {
            drips.0 = now;
            continue;
        }
        while drips.0 <= now {
            let shed = drips.1;
            *drips = ShotDrips(drips.0 + 1.0 / FLYING_DRIPS_PER_SECOND, shed.wrapping_add(1));
            let r = |k: u32| noise(shed.wrapping_mul(0x51ED_270B) ^ shot.index().index(), k);
            let side = if r(0) < 0.5 { 1.0 } else { -1.0 };
            let from = at.transform_point(arena::shot_spear_edge(0.1 + 0.85 * r(1)) * Vec3::new(1.0, 1.0, side));
            spawn_drip(&mut commands, &assets, from, now, (r(2), r(3)));
        }
    }
}

/// A drop of light at `from`, starting to gather at `now`, its size and life by `luck` (0..1 each).
fn spawn_drip(commands: &mut Commands, assets: &DripAssets, from: Vec3, now: f32, luck: (f32, f32)) {
    let transform = Transform::from_translation(from).with_scale(Vec3::ZERO);
    commands.spawn((
        Mesh3d(assets.mesh.clone()),
        MeshMaterial3d(assets.material.clone()),
        transform,
        GlobalTransform::from(transform),
        Drip { started: now, speed: 0.0, size: DRIP_SIZE.0 + (DRIP_SIZE.1 - DRIP_SIZE.0) * luck.0, life: DRIP_LIFE.0 + (DRIP_LIFE.1 - DRIP_LIFE.0) * luck.1 },
        NotShadowCaster,
        NotShadowReceiver,
    ));
}

/// Drops swell as they gather, then fall, drawn out by their fall and dwindling to nothing.
pub(super) fn fall_drips(mut commands: Commands, time: Res<Time>, mut drips: Query<(Entity, &mut Drip, &mut Transform)>) {
    let (now, dt) = (time.elapsed_secs(), time.delta_secs().min(0.05));
    for (entity, mut drip, mut transform) in &mut drips {
        let t = (now - drip.started) / drip.life;
        if t >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }
        drip.speed += DRIP_PULL * dt;
        transform.translation.y -= drip.speed * dt;
        let size = drip.size * (t / 0.2).min(1.0) * (1.0 - t);
        let stretch = DRIP_STRETCH.0 + (DRIP_STRETCH.1 - DRIP_STRETCH.0) * t;
        transform.scale = Vec3::new(size * DRIP_THIN, size * stretch, size * DRIP_THIN);
    }
}
