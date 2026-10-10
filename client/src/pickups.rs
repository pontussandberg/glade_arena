//! Pickups lying in the arena, and what taking one shows: a glowing green cross (a heal) or
//! golden chevrons (a haste) floating over a glow on the ground, bobbing and turning, gone while
//! they're waiting to come back; and a golden ring under a hasted fighter while it lasts. Who
//! takes a pickup is the server's call (`Pickup`, `Hasted`); a heal's number is in `feedback`,
//! the timers on the minimap.

use std::f32::consts::FRAC_PI_2;

use arena_shared::protocol::*;
use bevy::prelude::*;
use lightyear::prelude::*;

use crate::feedback::AttackClock;
use crate::glade::{self, palette, to_world};
use crate::render::{WorldAligned, shown};

pub struct PickupsPlugin;

impl Plugin for PickupsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_looks);
        app.add_systems(Update, (add_pickup_visuals, float_pickups, add_haste_rings, show_haste));
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

/// The pickups' meshes and materials, and the haste ring's.
#[derive(Resource)]
struct PickupLooks {
    bar: Handle<Mesh>,
    ground: Handle<Mesh>,
    ring: Handle<Mesh>,
    heal: (Handle<StandardMaterial>, Handle<StandardMaterial>),
    haste: (Handle<StandardMaterial>, Handle<StandardMaterial>),
    ring_material: Handle<StandardMaterial>,
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
        (materials.add(glade::glow(c, GLOW)), materials.add(glade::translucent(c, 0.3, 1.5)))
    };
    let (heal, haste) = (kind(PickupKind::Heal), kind(PickupKind::Haste));
    commands.insert_resource(PickupLooks {
        bar: meshes.add(Cuboid::new(0.14, 0.5, 0.14)),
        ground: meshes.add(Circle::new(0.6).mesh().resolution(24).build()),
        ring: meshes.add(Annulus::new(0.55, 0.7).mesh().resolution(32).build()),
        heal,
        haste,
        ring_material: materials.add(glade::translucent(palette::HASTE, 0.55, 2.0)),
    });
}

/// The floating part of a pickup (a child), which bobs and turns.
#[derive(Component)]
struct Floating(Entity);

/// Gives each pickup its look once it arrives: a cross for a heal (two crossed bars), a double
/// chevron for a haste (bars bent into two ">"), over a glow on the ground.
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
                commands.entity(floating).with_children(|chevrons| {
                    for x in [-0.12, 0.14] {
                        chevrons.spawn(bar(Vec3::new(x, 0.09, 0.0), -45.0));
                        chevrons.spawn(bar(Vec3::new(x, -0.09, 0.0), 45.0));
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

/// A fighter's haste ring (a child), shown while it's hasted.
#[derive(Component)]
struct HasteRing(Entity);

/// Each fighter gets a haste ring (hidden) once it has a body.
fn add_haste_rings(mut commands: Commands, looks: Res<PickupLooks>, new: Query<Entity, (With<PlayerId>, Added<Mesh3d>)>) {
    for player in &new {
        let ring = commands
            .spawn((
                Mesh3d(looks.ring.clone()),
                MeshMaterial3d(looks.ring_material.clone()),
                WorldAligned(Quat::from_rotation_x(-FRAC_PI_2), 0.05),
                Visibility::Hidden,
            ))
            .id();
        commands.entity(player).insert(HasteRing(ring)).add_child(ring);
    }
}

/// The ring shows while the haste covers "now" on the timeline the fighter is drawn on.
fn show_haste(
    clock: AttackClock,
    fighters: Query<(&Hasted, &HasteRing, Option<&Health>, Has<Predicted>)>,
    mut rings: Query<&mut Visibility>,
) {
    for (hasted, ring, health, is_me) in &fighters {
        let on = hasted.0.covers(clock.now(is_me)) && health.is_none_or(Health::alive);
        if let Ok(mut visibility) = rings.get_mut(ring.0) {
            visibility.set_if_neq(shown(on));
        }
    }
}
