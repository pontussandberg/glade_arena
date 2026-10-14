//! A blade dragged with its point on the ground: the faint line it scuffs, and the rasp of
//! spectral sparks and dark motes skipping off where it scrapes.

use super::*;

use bevy::light::{NotShadowCaster, NotShadowReceiver};

use crate::arena::FlatMesh;

/// A point dragging on the ground: how low it has to be (meters), how far it goes between scuffs,
/// how wide a scuff is, how long they last (seconds) and how see-through at most.
const DRAG_HEIGHT: f32 = 0.06;
const SCUFF_STEP: f32 = 0.1;
const SCUFF_WIDTH: f32 = 0.035;
const SCUFF_LIFE: f32 = 1.6;
pub(super) const SCUFF_ALPHA: f32 = 0.5;
/// The rasp of spectral sparks a dragging blade gives off: the odds a scuff throws any (one, now
/// and then two), how hard (m/s up, and back), how thin (meters) and how long they are drawn
/// (times as long as thin), how long they last (seconds), and the odds one is a dark mote of
/// the void instead of light.
const SPARK_ODDS: f32 = 0.55;
const SPARK_THROW: (f32, f32) = (1.6, 0.9);
const SPARK_SIZE: (f32, f32) = (0.004, 0.009);
const SPARK_STRETCH: f32 = 4.5;
const SPARK_LIFE: f32 = 0.45;
const SPARK_DARK: f32 = 0.35;

/// A scuff: flat, along +X from 0 to 1 (a little past, to meet the next), 1 across, solid down
/// its middle and clear at its sides.
pub(super) fn scuff_mesh() -> Mesh {
    let mut b = FlatMesh::default();
    let (start, end) = (Vec3::X * -0.15, Vec3::X * 1.15);
    for side in [0.5, -0.5] {
        let edge = Vec3::Z * side;
        b.tri_faded([start, start + edge, end], [1.0, 0.0, 1.0]);
        b.tri_faded([end, start + edge, end + edge], [1.0, 0.0, 0.0]);
    }
    b.build()
}

/// A scuff in the ground, from `started` (seconds).
#[derive(Component)]
pub(super) struct Scuff {
    pub(super) started: f32,
}

/// A blade whose point drags along the ground scuffs it, a short stroke each `SCUFF_STEP` it goes
/// (not across a jump: a waygate, a respawn).
pub(super) fn drag_marks(mut commands: Commands, time: Res<Time>, assets: Res<SwishAssets>, mut fighters: Query<&mut Blade>, blades: Query<&GlobalTransform>) {
    let now = time.elapsed_secs();
    for mut blade in &mut fighters {
        let Ok(held) = blades.get(blade.held) else { continue };
        if blade.lowest(held) > DRAG_HEIGHT {
            blade.dragged = None;
            continue;
        }
        let at = blade.at(held, 1.0).with_y(0.012);
        let Some(last) = blade.dragged else {
            blade.dragged = Some(at);
            continue;
        };
        let stroke = at - last;
        if stroke.length() > arena_shared::config::JUMP {
            blade.dragged = Some(at);
        } else if stroke.length() >= SCUFF_STEP {
            let transform = Transform::from_translation(last)
                .with_rotation(Quat::from_rotation_arc(Vec3::X, stroke.normalize()))
                .with_scale(Vec3::new(stroke.length(), 1.0, SCUFF_WIDTH));
            commands.spawn((
                Mesh3d(assets.scuff.clone()),
                MeshMaterial3d(assets.scuff_fades[0].clone()),
                transform,
                GlobalTransform::from(transform),
                Scuff { started: now },
                NotShadowCaster,
                NotShadowReceiver,
            ));
            // The rasp: now and then a spark or two of spectral light thrown off where it scrapes,
            // hopping up and back the way it came.
            blade.rasp = blade.rasp.wrapping_add(1);
            let seed = blade.rasp.wrapping_mul(2_654_435_761) ^ (now * 1000.0) as u32;
            let r = |k: u32| noise(seed, k);
            if r(0) < SPARK_ODDS {
                let back = -stroke.normalize();
                for k in 0..1 + (r(1) < 0.45) as u32 {
                    let side = Vec3::new(back.z, 0.0, -back.x) * (r(2 + k) - 0.5) * 1.6;
                    let velocity = (back * (0.4 + 0.6 * r(4 + k)) + side) * SPARK_THROW.1 + Vec3::Y * SPARK_THROW.0 * (0.6 + 0.6 * r(6 + k));
                    let size = SPARK_SIZE.0 + (SPARK_SIZE.1 - SPARK_SIZE.0) * r(8 + k);
                    let transform = Transform::from_translation(at + Vec3::Y * 0.02).with_scale(Vec3::splat(size));
                    let material = if r(12 + k) < SPARK_DARK { &assets.void_mote } else { &assets.wisp_glow };
                    commands.spawn((
                        Mesh3d(assets.wisp.clone()),
                        MeshMaterial3d(material.clone()),
                        transform,
                        GlobalTransform::from(transform),
                        Spark { started: now, velocity, size, life: SPARK_LIFE * (0.6 + 0.6 * r(10 + k)) },
                        NotShadowCaster,
                        NotShadowReceiver,
                    ));
                }
            }
            blade.dragged = Some(at);
        }
    }
}

/// A spark of spectral light thrown off by a dragging blade, from `started`.
#[derive(Component)]
pub(super) struct Spark {
    pub(super) started: f32,
    pub(super) velocity: Vec3,
    pub(super) size: f32,
    pub(super) life: f32,
}

/// Sparks hop, fall, and wink out.
pub(super) fn fly_sparks(mut commands: Commands, time: Res<Time>, mut sparks: Query<(Entity, &mut Spark, &mut Transform)>) {
    let (now, dt) = (time.elapsed_secs(), time.delta_secs().min(0.05));
    for (entity, mut spark, mut transform) in &mut sparks {
        let t = (now - spark.started) / spark.life;
        if t >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }
        spark.velocity.y -= GRAVITY * 0.5 * dt;
        transform.translation += spark.velocity * dt;
        transform.translation.y = transform.translation.y.max(0.01);
        // A thin streak along its flight, winking out at the end.
        let size = spark.size * (1.0 - t * t);
        transform.rotation = Quat::from_rotation_arc(Vec3::Y, spark.velocity.normalize_or(Vec3::Y));
        transform.scale = Vec3::new(size, size * SPARK_STRETCH, size);
    }
}

/// Scuffs fade, and are gone.
pub(super) fn fade_scuffs(mut commands: Commands, time: Res<Time>, assets: Res<SwishAssets>, mut scuffs: Query<(Entity, &Scuff, &mut MeshMaterial3d<StandardMaterial>)>) {
    let now = time.elapsed_secs();
    for (entity, scuff, mut material) in &mut scuffs {
        let t = (now - scuff.started) / SCUFF_LIFE;
        if t >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }
        let faded = faded(&assets.scuff_fades, t);
        if material.0 != *faded {
            material.0 = faded.clone();
        }
    }
}
