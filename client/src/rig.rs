//! Fighters with moving parts (see `glade::fighter_rig`), animated OSRS-style: poses that step
//! from frame to frame instead of gliding. Every fighter turns to face where it walks, swings or
//! dashes (`Facing`); rigged ones also swing their limbs as they walk and act out the attack
//! windup in step with the cast bar, and unrigged ones lean into a dash.
//!
//! The javelinist (the one rigged class) always carries the javelin cocked over the shoulder,
//! ready to throw. The windup: twist the throwing shoulder far back, lean back and sink into a
//! wide stance, lead arm pointing at the target, while drawing the javelin further back; then
//! lunge forward through the throw, the arm whipping over the top, hold the follow-through for
//! a moment and settle back. Head and javelin stay on the target throughout. The hand is empty
//! until a new javelin is drawn, halfway through the cooldown. A thrown Q is a quick flick of
//! the arm through the same throw.
//!
//! Transforms are only written when they change, so a fighter standing still costs nothing.

use std::f32::consts::{FRAC_PI_2, TAU};

use arena_shared::classes::AbilityKind;
use arena_shared::protocol::*;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use lightyear::prelude::*;

use crate::feedback::AttackClock;
use crate::glade::{self, RIG_HAND, RIG_HIP, RIG_NECK, RIG_SHOULDER};
use crate::render::{Look, Visuals, shown};

pub struct RigPlugin;

impl Plugin for RigPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_rigs);
        app.add_systems(Update, (add_rigs, turn_fighters, (pose_rigs, lean_unrigged)).chain().in_set(Posing));
    }
}

/// Where fighters get turned and posed; what depends on a fighter's transform runs after it.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct Posing;

/// Poses per windup and per walk cycle: the stepped, keyframe-y OSRS look.
const WINDUP_FRAMES: f32 = 8.0;
const WALK_FRAMES: f32 = 8.0;
/// Walk cycles per meter walked, and how far limbs swing (radians).
const STRIDES_PER_METER: f32 = 0.55;
const LEG_SWING: f32 = 0.5;
const ARM_SWING: f32 = 0.4;

/// Keyframes (carrying, fully drawn, thrown), radians. Limbs: positive swings forward. Twist:
/// negative turns the throwing (right, +Z) shoulder back. Lean: positive leans back.
const THROW_ARM: [f32; 3] = [-2.1, -2.8, 1.9];
const LEAD_ARM: [f32; 3] = [0.35, 1.5, -0.9];
const LEAD_LEG: [f32; 3] = [0.12, 0.55, 0.4];
const BACK_LEG: [f32; 3] = [-0.1, -0.5, -0.75];
const TWIST: [f32; 3] = [0.0, -1.0, 0.55];
const LEAN: [f32; 3] = [0.0, 0.28, -0.38];
const HEAD_DIP: [f32; 3] = [0.0, 0.2, -0.15];
/// After the throw: how long (ticks) the follow-through is held, and when it's back to carrying.
const FOLLOW_THROUGH: (f32, f32) = (3.0, 14.0);
/// A thrown Q's flick: ticks after the throw until the arm starts and finishes coming back.
const FLICK: (f32, f32) = (1.0, 10.0);
/// How quickly a fighter turns toward where it wants to face, and eases in and out of its walk
/// (per second, exponential).
const TURN_RATE: f32 = 16.0;
const WALK_RATE: f32 = 10.0;
/// How far an unrigged fighter leans into a dash (radians).
const DASH_LEAN: f32 = 0.45;

/// Rig meshes for each class that has one, and the white material the trim shares (its colors
/// are in the vertices). Arms use the fighter's own (team-colored) material, eyes a glow in it.
#[derive(Resource)]
struct RigAssets {
    meshes: HashMap<ClassId, RigHandles>,
    trim: Handle<StandardMaterial>,
}

struct RigHandles {
    head: Handle<Mesh>,
    arm: Handle<Mesh>,
    leg: Handle<Mesh>,
    held: Handle<Mesh>,
    eyes: Handle<Mesh>,
}

/// Which way a fighter faces, where it was last frame and how far it moved since (every
/// fighter).
#[derive(Component)]
struct Facing {
    look: Vec2,
    last_pos: Vec2,
    moved: Vec2,
}

/// A rigged fighter's moving parts (child entities) and walk state.
#[derive(Component)]
struct Rig {
    head: Entity,
    /// Right (throwing) arm and left (lead) arm.
    arms: [Entity; 2],
    /// Right (back) leg and left (lead) leg.
    legs: [Entity; 2],
    held: Entity,
    /// Walk cycles so far, and how much the fighter is walking (0..1, eased).
    stride: f32,
    walking: f32,
}

fn load_rigs(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let rigs = ClassId::all()
        .filter_map(|c| {
            let rig = glade::fighter_rig(&c.def().id)?;
            let mut add = |mesh| meshes.add(mesh);
            Some((
                c,
                RigHandles {
                    head: add(rig.head),
                    arm: add(rig.arm),
                    leg: add(rig.leg),
                    held: add(rig.held),
                    eyes: add(rig.eyes),
                },
            ))
        })
        .collect();
    commands.insert_resource(RigAssets { meshes: rigs, trim: materials.add(glade::matte(Color::WHITE)) });
}

fn mirrored(right: Vec3) -> Vec3 {
    Vec3::new(right.x, right.y, -right.z)
}

/// Gives a fighter its facing once it has a body, and its moving parts if its class has them.
fn add_rigs(
    mut commands: Commands,
    assets: Res<RigAssets>,
    mut visuals: ResMut<Visuals>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    new: Query<(Entity, &PlayerId, &ClassId, &Pos, &MeshMaterial3d<StandardMaterial>, Has<Predicted>), Added<Mesh3d>>,
) {
    for (player, id, class, pos, body, is_me) in &new {
        commands.entity(player).insert(Facing { look: Vec2::X, last_pos: pos.0, moved: Vec2::ZERO });
        let Some(handles) = assets.meshes.get(class) else { continue };
        let glow = visuals.material(&mut materials, id.0, is_me, Look::Eyes);
        let mut part = |mesh: &Handle<Mesh>, material: &Handle<StandardMaterial>, at: Vec3| {
            let transform = Transform::from_translation(at);
            commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), transform)).id()
        };
        let trim = &assets.trim;
        let rig = Rig {
            head: part(&handles.head, trim, RIG_NECK),
            arms: [part(&handles.arm, &body.0, RIG_SHOULDER), part(&handles.arm, &body.0, mirrored(RIG_SHOULDER))],
            legs: [part(&handles.leg, trim, RIG_HIP), part(&handles.leg, trim, mirrored(RIG_HIP))],
            held: part(&handles.held, trim, RIG_HAND),
            stride: 0.0,
            walking: 0.0,
        };
        let eyes = part(&handles.eyes, &glow, Vec3::ZERO);
        commands.entity(rig.head).add_child(eyes);
        commands.entity(rig.arms[0]).add_child(rig.held);
        commands.entity(player).add_children(&[rig.head, rig.arms[0], rig.arms[1], rig.legs[0], rig.legs[1]]);
        commands.entity(player).insert(rig);
    }
}

/// `1 - e^(-per_second * dt)`: how far to ease toward a target this frame.
fn rate(time: &Time, per_second: f32) -> f32 {
    1.0 - (-per_second * time.delta_secs()).exp()
}

/// Turns every fighter toward where it wants to face: its dash, its locked aim while winding up,
/// or the way it walks (if it moved this frame). Snaps once there, so it settles.
fn turn_fighters(time: Res<Time>, mut fighters: Query<(&mut Facing, &Pos, &AttackState, &AbilityState)>) {
    let turn = rate(&time, TURN_RATE);
    for (mut facing, pos, attack, ability) in &mut fighters {
        let moved = pos.0 - facing.last_pos;
        let wants = match (ability.dash, attack.windup) {
            (Some(dash), _) => dash.dir,
            (None, Some(windup)) => windup.dir,
            (None, None) if moved.length() > 0.01 => moved.normalize(),
            (None, None) => facing.look,
        };
        let look = facing.look.lerp(wants, turn).try_normalize().unwrap_or(wants);
        let look = if look.dot(wants) > 0.9999 { wants } else { look };
        let changed = Facing { look, last_pos: pos.0, moved };
        if facing.look != changed.look || facing.last_pos != changed.last_pos || facing.moved != changed.moved {
            *facing = changed;
        }
    }
}

/// 0 below `from`, 1 above `to`, smooth in between.
fn ease(from: f32, to: f32, x: f32) -> f32 {
    let t = ((x - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// `x` (0..1) held to the start of its frame, out of `frames`.
fn stepped(x: f32, frames: f32) -> f32 {
    (x * frames).floor() / frames
}

/// Poses each rig: facing (from `Facing`), limbs swinging as it walks, and the windup's (stepped)
/// progress driving the throw. The parts are posed in the body's (twisted, leaning) space; the
/// head and javelin undo that to stay on the aim.
fn pose_rigs(
    time: Res<Time>,
    clock: AttackClock,
    mut rigs: Query<(&mut Rig, &Facing, &mut Transform, &ClassId, &AttackState, &AbilityState, Has<Predicted>)>,
    mut parts: Query<(&mut Transform, &mut Visibility), Without<Rig>>,
) {
    let ease_walk = rate(&time, WALK_RATE);
    for (mut rig, facing, mut body, class, attack, ability, is_me) in &mut rigs {
        rig.stride += facing.moved.length() * STRIDES_PER_METER;
        let is_walking = if facing.moved.length() > 0.001 { 1.0 } else { 0.0 };
        let walking = rig.walking + (is_walking - rig.walking) * ease_walk;
        rig.walking = if (walking - is_walking).abs() < 1e-3 { is_walking } else { walking };
        let step = (stepped(rig.stride.fract(), WALK_FRAMES) * TAU).sin() * rig.walking;

        // Where in the throw we are: drawing (0..1) then throwing (0..1) during the windup, and
        // the follow-through easing back to carrying after it; a thrown Q flicks through it.
        let def = class.def();
        let now = clock.now(is_me);
        let since = |tick: Option<u32>| tick.map(|t| now - t as f32).filter(|s| *s >= 0.0);
        let (draw, mut throw) = match (attack.windup, since(attack.released_at(*class))) {
            (Some(windup), _) => {
                let progress = stepped(windup.progress(now, *class), WINDUP_FRAMES);
                (ease(0.0, 0.6, progress), ease(0.7, 1.0, progress))
            }
            (None, Some(since)) => (0.0, 1.0 - ease(FOLLOW_THROUGH.0, FOLLOW_THROUGH.1, since.floor())),
            (None, None) => (0.0, 0.0),
        };
        if let (AbilityKind::Projectile { .. }, Some(since)) = (&def.ability.kind, since(ability.used_at(*class))) {
            throw = throw.max(1.0 - ease(FLICK.0, FLICK.1, since.floor()));
        }
        let pose = |[carry, drawn, thrown]: [f32; 3]| carry.lerp(drawn, draw).lerp(thrown, throw);
        let braced = draw.max(throw);
        // The hand is empty until a new javelin is drawn, halfway through the cooldown.
        let rearm = (def.attack.cooldown_ticks - def.attack.windup_ticks) as f32 / 2.0;
        let empty_handed = attack.windup.is_none() && since(attack.released_at(*class)).is_some_and(|s| s < rearm);

        let lean = Quat::from_rotation_y(pose(TWIST)) * Quat::from_rotation_z(pose(LEAN));
        let legs = [pose(BACK_LEG) - step * LEG_SWING * (1.0 - braced), pose(LEAD_LEG) + step * LEG_SWING * (1.0 - braced)];
        // A wide stance lowers the hips (the legs are straight), so the feet stay on the ground.
        let spread = legs[0].abs().max(legs[1].abs());
        let mut posed = *body;
        posed.rotation = Quat::from_rotation_y(facing.look.to_angle()) * lean;
        posed.translation.y = -RIG_HIP.y * (1.0 - spread.cos());
        body.set_if_neq(posed);

        let unlean = lean.inverse();
        let throw_arm = Quat::from_rotation_z(pose(THROW_ARM) - step * ARM_SWING * 0.3);
        // Always pointing a little above the aim, whatever the arm and body are doing.
        let javelin = throw_arm.inverse() * unlean * Quat::from_rotation_z(-(FRAC_PI_2 - 0.12));
        let rotations = [
            (rig.head, unlean * Quat::from_rotation_z(-pose(HEAD_DIP))),
            (rig.arms[0], throw_arm),
            (rig.arms[1], Quat::from_rotation_z(pose(LEAD_ARM) + step * ARM_SWING)),
            (rig.legs[0], Quat::from_rotation_z(legs[0])),
            (rig.legs[1], Quat::from_rotation_z(legs[1])),
            (rig.held, javelin),
        ];
        for (part, rotation) in rotations {
            if let Ok((mut transform, mut visibility)) = parts.get_mut(part) {
                if transform.rotation != rotation {
                    transform.rotation = rotation;
                }
                if part == rig.held {
                    visibility.set_if_neq(shown(!empty_handed));
                }
            }
        }
    }
}

/// Fighters without moving parts: face their `Facing`, leaning into a dash.
fn lean_unrigged(mut fighters: Query<(&Facing, &mut Transform, &AbilityState), Without<Rig>>) {
    for (facing, mut body, ability) in &mut fighters {
        let lean = if ability.dash.is_some() { -DASH_LEAN } else { 0.0 };
        let rotation = Quat::from_rotation_y(facing.look.to_angle()) * Quat::from_rotation_z(lean);
        if body.rotation != rotation {
            body.rotation = rotation;
        }
    }
}
