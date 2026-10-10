//! Pickups lying in the arena, and what taking one shows: a glowing green cross (a heal) or a
//! blue winged boot (a haste, like a sprint's) floating over a glow on the ground, bobbing and turning, gone while
//! they're waiting to come back. A heal bursts around whoever takes it, even at full health (a
//! ring spreading on the ground, sparks spiraling up); a hasted fighter trails blue wind streaks while it runs, like a
//! sprint. Who takes a pickup is the server's call (`Pickup`, `Hasted`); a heal's number is in
//! `feedback`, the timers on the minimap.

use std::f32::consts::FRAC_PI_2;

use arena_shared::protocol::*;
use bevy::prelude::*;
use lightyear::prelude::*;

use crate::feedback::AttackClock;
use crate::arena::{self, palette, to_world};
use crate::render::shown;

pub struct PickupsPlugin;

impl Plugin for PickupsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_looks);
        app.add_systems(
            Update,
            (add_pickup_visuals, float_pickups, (burst_on_take, grow_bursts).chain(), (trail_haste, fade_streaks).chain()),
        );
    }
}

/// How high a pickup floats (meters), how far it bobs and how fast (per second), and how fast
/// it turns (radians per second).
const FLOAT_HEIGHT: f32 = 0.85;
const BOB: f32 = 0.12;
const BOB_RATE: f32 = 2.2;
const TURN_RATE: f32 = 1.4;
/// How brightly pickups glow.
const GLOW: f32 = 3.0;

/// A heal's burst: how long it lasts (seconds), how wide its ring spreads (meters), and its
/// sparks: how many, how far out from the fighter they start, how high they rise and how far
/// round they turn (radians) on the way.
const BURST_SECONDS: f32 = 0.9;
const BURST_RING: (f32, f32) = (0.5, 2.0);
const SPARKS: usize = 10;
const SPARK_RADIUS: f32 = 0.5;
const SPARK_RISE: f32 = 1.7;
const SPARK_TURN: f32 = 1.6;

/// A haste's wind streaks: how often a hasted fighter on the move sheds one (seconds), how long
/// one lasts, how long it starts (meters), and the band around the fighter they appear in
/// (sideways from its middle; heights).
const STREAK_EVERY: f32 = 0.035;
const STREAK_SECONDS: f32 = 0.3;
const STREAK_LENGTH: f32 = 0.9;
const STREAK_SIDE: f32 = 0.38;
const STREAK_HEIGHTS: (f32, f32) = (0.2, 1.6);

/// The pickups' meshes and materials, and their effects'.
#[derive(Resource)]
struct PickupLooks {
    bar: Handle<Mesh>,
    ground: Handle<Mesh>,
    heal: (Handle<StandardMaterial>, Handle<StandardMaterial>),
    haste: (Handle<StandardMaterial>, Handle<StandardMaterial>),
    /// The winged boot: its shaft, its foot, a feather of its wings (one meter long along +X,
    /// scaled to its length), and the wings' pale glow.
    boot_shaft: Handle<Mesh>,
    boot_foot: Handle<Mesh>,
    feather: Handle<Mesh>,
    wing: Handle<StandardMaterial>,
    /// A heal burst's ring and sparks.
    ring: Handle<Mesh>,
    spark: Handle<Mesh>,
    /// A wind streak: a thin bar one meter long along +X, scaled to its length.
    streak: Handle<Mesh>,
    streak_material: Handle<StandardMaterial>,
}

impl PickupLooks {
    /// A kind's glowing body and its glow on the ground.
    fn of(&self, kind: PickupKind) -> &(Handle<StandardMaterial>, Handle<StandardMaterial>) {
        match kind {
            PickupKind::Heal => &self.heal,
            PickupKind::Haste => &self.haste,
        }
    }
}

/// A pickup's color.
pub(crate) fn color(kind: PickupKind) -> Color {
    match kind {
        PickupKind::Heal => palette::HEAL,
        PickupKind::Haste => palette::HASTE,
    }
}

/// A pickup's icon, as text (the minimap's marker).
pub(crate) fn icon(kind: PickupKind) -> &'static str {
    match kind {
        PickupKind::Heal => "+",
        PickupKind::Haste => ">>",
    }
}

fn load_looks(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let mut kind = |kind| {
        let c = color(kind);
        (materials.add(arena::glow(c, GLOW)), materials.add(arena::translucent(c, 0.3, 1.5)))
    };
    let (heal, haste) = (kind(PickupKind::Heal), kind(PickupKind::Haste));
    commands.insert_resource(PickupLooks {
        bar: meshes.add(Cuboid::new(0.14, 0.5, 0.14)),
        ground: meshes.add(Circle::new(0.6).mesh().resolution(24).build()),
        heal,
        haste,
        boot_shaft: meshes.add(Cuboid::new(0.16, 0.32, 0.17)),
        boot_foot: meshes.add(Cuboid::new(0.36, 0.12, 0.17)),
        feather: meshes.add(Cuboid::new(1.0, 0.05, 0.025)),
        wing: materials.add(arena::glow(palette::ICE, GLOW)),
        ring: meshes.add(Annulus::new(0.8, 1.0).mesh().resolution(40).build()),
        spark: meshes.add(Cuboid::new(0.08, 0.08, 0.08)),
        streak: meshes.add(Cuboid::new(1.0, 0.03, 0.03)),
        streak_material: materials.add(arena::translucent(palette::HASTE, 0.7, 2.5)),
    });
}

/// The floating part of a pickup (a child), which bobs and turns.
#[derive(Component)]
struct Floating(Entity);

/// Gives each pickup its look once it arrives: a cross for a heal (two crossed bars), a winged
/// boot for a haste (toe forward, a fan of pale feathers off each side of its heel), over a glow
/// on the ground.
fn add_pickup_visuals(mut commands: Commands, looks: Res<PickupLooks>, new: Query<(Entity, &Pickup), Added<Pickup>>) {
    for (entity, pickup) in &new {
        let (body, glow) = looks.of(pickup.kind).clone();
        let bar = |at: Vec3, degrees: f32| {
            (Mesh3d(looks.bar.clone()), MeshMaterial3d(body.clone()), Transform::from_translation(at).with_rotation(Quat::from_rotation_z(degrees.to_radians())))
        };
        let floating = commands.spawn((Transform::from_xyz(0.0, FLOAT_HEIGHT, 0.0), Visibility::Inherited)).id();
        match pickup.kind {
            PickupKind::Heal => {
                commands.entity(floating).with_children(|cross| {
                    cross.spawn(bar(Vec3::ZERO, 0.0));
                    cross.spawn(bar(Vec3::ZERO, 90.0));
                });
            }
            PickupKind::Haste => {
                let part = |mesh: &Handle<Mesh>, material: &Handle<StandardMaterial>, transform: Transform| {
                    (Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), transform)
                };
                commands.entity(floating).with_children(|boot| {
                    boot.spawn(part(&looks.boot_shaft, &body, Transform::from_xyz(-0.06, 0.1, 0.0)));
                    boot.spawn(part(&looks.boot_foot, &body, Transform::from_xyz(0.04, -0.11, 0.0)));
                    // Feathers fanning up and back from the top of the heel, longest highest.
                    for side in [-1.0, 1.0] {
                        for (degrees, length) in [(25.0f32, 0.22), (45.0, 0.3), (65.0, 0.36)] {
                            let up_back = Vec2::from_angle((180.0 - degrees).to_radians());
                            let root = Vec3::new(-0.1, 0.18, side * 0.1);
                            let center = root + (up_back * length / 2.0).extend(0.0);
                            let feather = Transform::from_translation(center)
                                .with_rotation(Quat::from_rotation_z(up_back.to_angle()))
                                .with_scale(Vec3::new(length, 1.0, 1.0));
                            boot.spawn(part(&looks.feather, &looks.wing, feather));
                        }
                    }
                });
            }
        }
        let ground = commands
            .spawn((
                Mesh3d(looks.ground.clone()),
                MeshMaterial3d(glow),
                Transform::from_xyz(0.0, 0.03, 0.0).with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
            ))
            .id();
        commands
            .entity(entity)
            .insert((Transform::from_translation(to_world(pickup.at, 0.0)), Visibility::Hidden, Floating(floating)))
            .add_children(&[floating, ground]);
    }
}

/// Bobs and turns the pickups that are lying there; hides the taken ones.
fn float_pickups(
    time: Res<Time>,
    mut pickups: Query<(&Pickup, &Floating, &mut Visibility)>,
    mut floating: Query<&mut Transform>,
) {
    let t = time.elapsed_secs();
    for (pickup, float, mut visibility) in &mut pickups {
        let there = pickup.back_at.is_none();
        visibility.set_if_neq(shown(there));
        if !there {
            continue;
        }
        if let Ok(mut transform) = floating.get_mut(float.0) {
            transform.translation.y = FLOAT_HEIGHT + BOB * (t * BOB_RATE).sin();
            transform.rotation = Quat::from_rotation_y(t * TURN_RATE);
        }
    }
}

/// When a pickup was last seen taken (its `back_at` then), so each taking bursts once.
#[derive(Component)]
struct SeenTaken(Option<u32>);

/// A heal's burst around `player`, from `born` (seconds): its ring and sparks are its children.
#[derive(Component)]
struct HealBurst {
    player: Entity,
    born: f32,
    ring: Entity,
    sparks: Vec<Entity>,
}

/// Bursts around whoever takes a heal, the moment we learn it's taken: tied to the pickup, not
/// to health, so it shows at full health too. A taking already there when we first see the
/// pickup is only noted.
fn burst_on_take(
    mut commands: Commands,
    time: Res<Time>,
    looks: Res<PickupLooks>,
    mut pickups: Query<(Entity, &Pickup, Option<&mut SeenTaken>), Changed<Pickup>>,
    players: Query<(Entity, &PlayerId)>,
) {
    for (entity, pickup, seen) in &mut pickups {
        let Some(mut seen) = seen else {
            commands.entity(entity).insert(SeenTaken(pickup.back_at));
            continue;
        };
        if std::mem::replace(&mut seen.0, pickup.back_at) == pickup.back_at || pickup.back_at.is_none() {
            continue;
        }
        let taker = pickup.taken_by.and_then(|by| players.iter().find(|(_, id)| id.0 == by));
        let (Some((player, _)), PickupKind::Heal) = (taker, pickup.kind) else { continue };
        let (glow, faint) = looks.heal.clone();
        let flat = Quat::from_rotation_x(-FRAC_PI_2);
        let ring = commands
            .spawn((Mesh3d(looks.ring.clone()), MeshMaterial3d(faint), Transform::from_xyz(0.0, 0.05, 0.0).with_rotation(flat)))
            .id();
        let sparks: Vec<Entity> = (0..SPARKS)
            .map(|_| commands.spawn((Mesh3d(looks.spark.clone()), MeshMaterial3d(glow.clone()), Transform::default())).id())
            .collect();
        commands
            .spawn((HealBurst { player, born: time.elapsed_secs(), ring, sparks: sparks.clone() }, Transform::default(), Visibility::Inherited))
            .add_child(ring)
            .add_children(&sparks);
    }
}

/// Bursts follow their fighter: the ring spreads out and thins away, the sparks spiral up
/// around it, shrinking as they go; then it's gone.
fn grow_bursts(
    mut commands: Commands,
    time: Res<Time>,
    players: Query<&Pos>,
    bursts: Query<(Entity, &HealBurst)>,
    mut parts: Query<&mut Transform>,
) {
    for (entity, burst) in &bursts {
        let t = (time.elapsed_secs() - burst.born) / BURST_SECONDS;
        let (Ok(pos), true) = (players.get(burst.player), t < 1.0) else {
            commands.entity(entity).despawn();
            continue;
        };
        if let Ok(mut root) = parts.get_mut(entity) {
            root.translation = to_world(pos.0, 0.0);
        }
        // Eased out: quick at first, slowing as it fades.
        let out = 1.0 - (1.0 - t) * (1.0 - t);
        let fade = (1.0 - t).min(1.0);
        if let Ok(mut ring) = parts.get_mut(burst.ring) {
            let width = BURST_RING.0 + (BURST_RING.1 - BURST_RING.0) * out;
            ring.scale = Vec3::new(width, width, 1.0) * fade.sqrt();
        }
        for (i, spark) in burst.sparks.iter().enumerate() {
            let Ok(mut transform) = parts.get_mut(*spark) else { continue };
            let start = i as f32 / SPARKS as f32;
            let angle = start * std::f32::consts::TAU + out * SPARK_TURN;
            let height = 0.2 + 0.5 * ((i * 7) % SPARKS) as f32 / SPARKS as f32 + SPARK_RISE * out;
            transform.translation = Vec3::new(angle.cos() * SPARK_RADIUS, height, angle.sin() * SPARK_RADIUS);
            transform.rotation = Quat::from_rotation_y(angle);
            transform.scale = Vec3::splat(fade);
        }
    }
}

/// Where a hasted fighter was last frame, and how long until it sheds its next streak.
#[derive(Component)]
struct HasteTrail {
    last: Vec2,
    next_in: f32,
}

/// A wind streak, from `born` (seconds): it stays where it was shed, so it trails behind.
#[derive(Component)]
struct Streak {
    born: f32,
    length: f32,
}

/// While a fighter is hasted (on the timeline it's drawn on) and moving, it sheds golden wind
/// streaks along the way it runs, around its body at varying heights: a sprint's rush of air.
fn trail_haste(
    mut commands: Commands,
    time: Res<Time>,
    clock: AttackClock,
    looks: Res<PickupLooks>,
    mut fighters: Query<(Entity, &Pos, &Hasted, Option<&Health>, Has<Predicted>, Option<&mut HasteTrail>), With<Mesh3d>>,
    mut seed: Local<u32>,
) {
    let mut random = || {
        *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (*seed >> 8) as f32 / (1u32 << 24) as f32
    };
    for (fighter, pos, hasted, health, is_me, trail) in &mut fighters {
        let on = hasted.0.covers(clock.now(is_me)) && health.is_none_or(Health::alive);
        let Some(mut trail) = trail else {
            if on {
                commands.entity(fighter).insert(HasteTrail { last: pos.0, next_in: 0.0 });
            }
            continue;
        };
        if !on {
            commands.entity(fighter).remove::<HasteTrail>();
            continue;
        }
        let moved = pos.0 - std::mem::replace(&mut trail.last, pos.0);
        trail.next_in -= time.delta_secs();
        let Some(dir) = moved.try_normalize() else { continue };
        while trail.next_in <= 0.0 {
            trail.next_in += STREAK_EVERY;
            let side = Vec2::new(-dir.y, dir.x) * STREAK_SIDE * (random() * 2.0 - 1.0);
            let height = STREAK_HEIGHTS.0 + (STREAK_HEIGHTS.1 - STREAK_HEIGHTS.0) * random();
            let length = STREAK_LENGTH * (0.6 + 0.6 * random());
            let at = to_world(pos.0 - dir * 0.2 + side, height);
            commands.spawn((
                Streak { born: time.elapsed_secs(), length },
                Mesh3d(looks.streak.clone()),
                MeshMaterial3d(looks.streak_material.clone()),
                // Its front at `at`, trailing back the way the fighter came.
                Transform::from_translation(at)
                    .with_rotation(Quat::from_rotation_y(dir.to_angle()))
                    .with_scale(Vec3::new(length, 1.0, 1.0)),
            ));
        }
    }
}

/// Streaks shorten and thin away over their life, then go.
fn fade_streaks(mut commands: Commands, time: Res<Time>, mut streaks: Query<(Entity, &Streak, &mut Transform)>) {
    for (entity, streak, mut transform) in &mut streaks {
        let t = (time.elapsed_secs() - streak.born) / STREAK_SECONDS;
        if t >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }
        let left = 1.0 - t;
        transform.scale = Vec3::new(streak.length * left, left, left);
    }
}
