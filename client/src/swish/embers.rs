//! Embers flaking off an empowered blade's edge and drifting up.

use super::*;

/// Embers flaking off a quickened blade's edge (`Blade::quickened`): how many a second, how fast
/// they rise (m/s, least and most) and drift aside, how long each lasts (seconds, least and
/// most), how big (meters, least and most), how drawn out along their flight, and how bright.
pub(super) const EMBERS_PER_SECOND: f32 = 22.0;
pub(super) const EMBER_RISE: (f32, f32) = (0.35, 0.8);
pub(super) const EMBER_DRIFT: f32 = 0.25;
pub(super) const EMBER_LIFE: (f32, f32) = (0.35, 0.75);
pub(super) const EMBER_SIZE: (f32, f32) = (0.018, 0.035);
pub(super) const EMBER_STRETCH: f32 = 3.0;
pub(super) const EMBER_GLOW: f32 = 7.0;

/// A quickened blade sheds sharp embers off its edge, one after another, from anywhere along it.
pub(super) fn shed_embers(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<SwishAssets>,
    mut fighters: Query<(&mut Blade, &InheritedVisibility)>,
    blades: Query<&GlobalTransform>,
) {
    let now = time.elapsed_secs();
    for (mut blade, visible) in &mut fighters {
        if !blade.quickened || !visible.get() {
            blade.embers.0 = now;
            continue;
        }
        let Ok(held) = blades.get(blade.held) else { continue };
        while blade.embers.0 <= now {
            let shed = blade.embers.1;
            blade.embers = (blade.embers.0 + 1.0 / EMBERS_PER_SECOND, shed.wrapping_add(1));
            let r = |k: u32| noise(shed.wrapping_mul(0x51ED_270B) ^ blade.held.index().index(), k);
            let from = held.transform_point(arena::glaive_edge(0.05 + 0.95 * r(0)));
            let aside = Vec3::new(r(1) - 0.5, 0.0, r(2) - 0.5) * 2.0 * EMBER_DRIFT;
            let velocity = Vec3::Y * (EMBER_RISE.0 + (EMBER_RISE.1 - EMBER_RISE.0) * r(3)) + aside;
            let size = EMBER_SIZE.0 + (EMBER_SIZE.1 - EMBER_SIZE.0) * r(4);
            let transform = Transform::from_translation(from).with_scale(Vec3::ZERO);
            commands.spawn((
                Mesh3d(assets.wisp.clone()),
                MeshMaterial3d(assets.ember_glow.clone()),
                transform,
                GlobalTransform::from(transform),
                Ember { started: now, velocity, size, life: EMBER_LIFE.0 + (EMBER_LIFE.1 - EMBER_LIFE.0) * r(5) },
                NotShadowCaster,
                NotShadowReceiver,
            ));
        }
    }
}

/// An ember flaking off a quickened blade, from `started`.
#[derive(Component)]
pub(super) struct Ember {
    pub(super) started: f32,
    pub(super) velocity: Vec3,
    pub(super) size: f32,
    pub(super) life: f32,
}

/// Embers rise, slowing, flare and wink out.
pub(super) fn drift_embers(mut commands: Commands, time: Res<Time>, mut embers: Query<(Entity, &mut Ember, &mut Transform)>) {
    let (now, dt) = (time.elapsed_secs(), time.delta_secs().min(0.05));
    for (entity, mut ember, mut transform) in &mut embers {
        let t = (now - ember.started) / ember.life;
        if t >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }
        transform.translation += ember.velocity * dt;
        ember.velocity *= 1.0 - 2.0 * dt;
        // A sharp sliver along its flight, flaring at once and dwindling to nothing.
        let size = ember.size * (t / 0.1).min(1.0) * (1.0 - t);
        transform.rotation = Quat::from_rotation_arc(Vec3::Y, ember.velocity.normalize_or(Vec3::Y));
        transform.scale = Vec3::new(size, size * EMBER_STRETCH, size);
    }
}
