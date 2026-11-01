//! Ravens wheeling over a fortress's keep (on a map whose theme builds `Theme::battlements`): a
//! few black birds on slow circles at their own heights, beating their wings for a while, then
//! gliding.

use super::*;

/// A raven: the middle of the circle it wheels on, how wide, how fast (radians a second; the
/// sign is which way round), and where on it it was at the start.
#[derive(Component)]
pub(super) struct Raven {
    center: Vec3,
    radius: f32,
    speed: f32,
    phase: f32,
}

/// One of a raven's wings: which side of it (1 its right, -1 its left).
#[derive(Component)]
pub(super) struct Wing(f32);

const FEATHERS: Color = Color::srgb_u8(0x10, 0x10, 0x14);
const RAVENS: usize = 9;
/// How much bigger than life: small enough to read as birds, big enough to be seen from the
/// ground, high over the keep.
const SCALE: f32 = 1.6;

/// The ravens over the map's keep (its biggest tower), if it has towers.
pub(super) fn spawn_ravens(commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<PlainMaterial>, m: &Map, rng: &mut Lcg) {
    let Some(keep) = m.towers.iter().max_by(|a, b| (a.size().x * a.size().y).total_cmp(&(b.size().x * b.size().y))) else { return };
    let over = to_world(keep.center(), keep.top);
    let material = materials.add(matte(FEATHERS));
    // Its body along -z (forward), head and beak in front, a wedge of tail behind; each wing a
    // thin slab out from its root at the shoulder.
    let body = meshes.add(Cuboid::new(0.11, 0.1, 0.38));
    let head = meshes.add(Cuboid::new(0.08, 0.08, 0.1));
    let tail = meshes.add(Cuboid::new(0.14, 0.02, 0.16));
    let wing = |side: f32| Cuboid::new(0.5, 0.02, 0.2).mesh().build().translated_by(Vec3::X * side * 0.25);
    let wings = [(1.0, meshes.add(wing(1.0))), (-1.0, meshes.add(wing(-1.0)))];
    for _ in 0..RAVENS {
        let raven = Raven {
            center: over + Vec3::Y * rng.range(4.0, 12.0) + Vec3::new(rng.range(-3.0, 3.0), 0.0, rng.range(-3.0, 3.0)),
            radius: rng.range(5.0, 15.0),
            speed: rng.range(0.22, 0.42) * if rng.next() < 0.7 { 1.0 } else { -1.0 },
            phase: rng.range(0.0, std::f32::consts::TAU),
        };
        commands
            .spawn((ArenaScene, raven, Transform::from_scale(Vec3::splat(SCALE)), Visibility::default()))
            .with_children(|bird| {
                bird.spawn((Mesh3d(body.clone()), MeshMaterial3d(material.clone())));
                bird.spawn((Mesh3d(head.clone()), MeshMaterial3d(material.clone()), Transform::from_xyz(0.0, 0.03, -0.23)));
                bird.spawn((Mesh3d(tail.clone()), MeshMaterial3d(material.clone()), Transform::from_xyz(0.0, 0.0, 0.25)));
                for (side, wing) in &wings {
                    bird.spawn((Wing(*side), Mesh3d(wing.clone()), MeshMaterial3d(material.clone()), Transform::from_xyz(*side * 0.04, 0.02, -0.02)));
                }
            });
    }
}

/// Round they go, rising and falling a little, banked into the turn; wings beating for a spell,
/// then held out in a glide.
pub(super) fn fly_ravens(time: Res<Time>, mut ravens: Query<(&Raven, &mut Transform, &Children)>, mut wings: Query<(&Wing, &mut Transform), Without<Raven>>) {
    let t = time.elapsed_secs();
    for (raven, mut at, children) in &mut ravens {
        let a = raven.phase + t * raven.speed;
        let round = Vec3::new(a.cos(), 0.0, a.sin());
        at.translation = raven.center + round * raven.radius + Vec3::Y * (t * 0.4 + raven.phase).sin() * 0.8;
        let ahead = Vec3::new(-a.sin(), 0.0, a.cos()) * raven.speed.signum();
        // Banked: its wing toward the middle down.
        let bank = Quat::from_rotation_z(-0.35 * raven.speed.signum());
        at.rotation = Transform::IDENTITY.looking_to(ahead, Vec3::Y).rotation * bank;
        let beating = (t * 0.3 + raven.phase * 2.0).sin() > -0.1;
        let lift = if beating { (t * 8.5 + raven.phase * 3.0).sin() * 0.65 } else { 0.1 };
        for child in children {
            if let Ok((wing, mut transform)) = wings.get_mut(*child) {
                transform.rotation = Quat::from_rotation_z(wing.0 * lift);
            }
        }
    }
}
