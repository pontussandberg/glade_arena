//! Spectral motes rising off a drape's ends, more as it moves.

use super::*;

#[derive(Resource)]
pub(super) struct MoteMesh(pub(super) Handle<Mesh>);

/// A spark of spectral light given off by cloth: it drifts up, slowing, and shrinks away.
#[derive(Component)]
pub(super) struct Mote {
    pub(super) born: f32,
    pub(super) life: f32,
    pub(super) size: f32,
    pub(super) drift: Vec3,
}

pub(super) fn drift_motes(time: Res<Time>, mut commands: Commands, mut motes: Query<(Entity, &Mote, &mut Transform)>) {
    let (now, dt) = (time.elapsed_secs(), time.delta_secs());
    for (entity, mote, mut transform) in &mut motes {
        let age = (now - mote.born) / mote.life;
        if age >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }
        transform.translation += mote.drift * (1.0 - age * 0.7) * dt;
        let size = mote.size * (1.0 - age).sqrt() * (age * 8.0).min(1.0);
        transform.scale = Vec3::new(size, size * 1.8, size);
    }
}
