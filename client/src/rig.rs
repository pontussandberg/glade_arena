//! Fighters with moving parts (see `glade::fighter_rig`), animated OSRS-style: poses that step
//! from frame to frame instead of gliding. Every fighter turns to face where it walks, swings or
//! dashes (`Facing`), swings its limbs as it walks, acts out its attack windup in step with the
//! cast bar and strikes a pose while dashing; each class has its own `Moves`.
//!
//! The javelinist always carries the javelin cocked over the shoulder, ready to throw. The
//! windup: twist the throwing shoulder far back, lean back and sink into a wide stance, lead arm
//! pointing at the target, while drawing the javelin further back; then lunge forward through
//! the throw, the arm whipping over the top, hold the follow-through for a moment and settle
//! back. Head and javelin stay on the target throughout. The hand is empty until a new javelin
//! is drawn, halfway through the cooldown. A thrown Q is a quick flick of the arm through the
//! same throw.
//!
//! The revenant holds its sword low and ready. The windup raises it up and back over the
//! shoulder, turning away; the strike chops it down through the target, stepping into it. A
//! dash is a forward lunge with the blade swept back.
//!
//! Transforms are only written when they change, so a fighter standing still costs nothing.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use arena_shared::classes::{AbilityKind, AttackKind};
use arena_shared::protocol::*;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use lightyear::prelude::*;

use crate::feedback::AttackClock;
use crate::glade::{self, palette, RIG_HAND, RIG_HIP, RIG_NECK, RIG_SHOULDER};
use crate::render::shown;

pub struct RigPlugin;

impl Plugin for RigPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_rigs);
        app.add_systems(Update, (add_rigs, turn_fighters, pose_rigs).chain().in_set(Posing));
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

/// How a rigged class moves. Keyframes are (carrying, fully drawn back, released, dashing),
/// radians. Limbs: positive swings forward. Twist: negative turns the weapon (right, +Z)
/// shoulder back. Lean: positive leans back.
struct Moves {
    weapon_arm: [f32; 4],
    lead_arm: [f32; 4],
    lead_leg: [f32; 4],
    back_leg: [f32; 4],
    twist: [f32; 4],
    lean: [f32; 4],
    head_dip: [f32; 4],
    held: Held,
}

/// How the weapon sits in the hand.
enum Held {
    /// Kept pointing (a little above) the aim, whatever the arm and body do: a javelin ready to
    /// throw.
    OnTarget,
    /// Fixed in the fist, along the arm and this far (radians) forward of it: a sword.
    InHand(f32),
}

const JAVELINIST: Moves = Moves {
    weapon_arm: [-2.1, -2.8, 1.9, -2.1],
    lead_arm: [0.35, 1.5, -0.9, -0.8],
    lead_leg: [0.12, 0.55, 0.4, 0.6],
    back_leg: [-0.1, -0.5, -0.75, -0.6],
    twist: [0.0, -1.0, 0.55, 0.0],
    lean: [0.0, 0.28, -0.38, -0.45],
    head_dip: [0.0, 0.2, -0.15, 0.0],
    held: Held::OnTarget,
};

const REVENANT: Moves = Moves {
    weapon_arm: [0.65, -2.7, 1.0, -1.5],
    lead_arm: [-0.2, 0.7, -0.6, -1.1],
    lead_leg: [0.1, 0.45, 0.6, 0.65],
    back_leg: [-0.1, -0.4, -0.65, -0.7],
    twist: [0.0, -0.75, 0.5, 0.0],
    lean: [0.0, 0.2, -0.4, -0.55],
    head_dip: [0.0, 0.15, -0.25, 0.0],
    held: Held::InHand(0.45),
};

/// The moves for a class's look (every class has one: `glade::FIGHTER_LOOKS`).
fn moves(class_key: &str) -> &'static Moves {
    match class_key {
        "javelinist" => &JAVELINIST,
        "revenant" => &REVENANT,
        _ => unreachable!("no moves for class {class_key:?}"),
    }
}

/// After the throw: how long (ticks) the follow-through is held, and when it's back to carrying.
const FOLLOW_THROUGH: (f32, f32) = (3.0, 14.0);
/// A thrown Q's flick: ticks after the throw until the arm starts and finishes coming back.
const FLICK: (f32, f32) = (1.0, 10.0);
/// How quickly a fighter turns toward where it wants to face, and eases in and out of its walk
/// (per second, exponential).
const TURN_RATE: f32 = 16.0;
const WALK_RATE: f32 = 10.0;
/// How brightly the eyes glow.
const EYE_GLOW: f32 = 6.0;

/// Rig meshes for each class, and the eyes' glow (the same for everyone). Every other part is
/// drawn in the fighter's own material (colors in the vertices), so a hit flashes all of it.
#[derive(Resource)]
struct RigAssets {
    meshes: HashMap<ClassId, RigHandles>,
    glow: Handle<StandardMaterial>,
}

struct RigHandles {
    head: Handle<Mesh>,
    arm: Handle<Mesh>,
    leg: Handle<Mesh>,
    held: Handle<Mesh>,
    eyes: Handle<Mesh>,
}

/// Which way a fighter faces, where it was last frame and how far it moved since.
#[derive(Component)]
struct Facing {
    look: Vec2,
    last_pos: Vec2,
    moved: Vec2,
}

/// A fighter's moving parts (child entities), its moves and walk state.
#[derive(Component)]
struct Rig {
    head: Entity,
    /// Right (weapon) arm and left (lead) arm.
    arms: [Entity; 2],
    /// Right (back) leg and left (lead) leg.
    legs: [Entity; 2],
    held: Entity,
    moves: &'static Moves,
    /// Walk cycles so far, and how much the fighter is walking (0..1, eased).
    stride: f32,
    walking: f32,
}

fn load_rigs(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let rigs = ClassId::all()
        .map(|c| {
            let rig = glade::fighter_rig(&c.def().id);
            let mut add = |mesh| meshes.add(mesh);
            let handles = RigHandles {
                head: add(rig.head),
                arm: add(rig.arm),
                leg: add(rig.leg),
                held: add(rig.held),
                eyes: add(rig.eyes),
            };
            (c, handles)
        })
        .collect();
    let glow = materials.add(glade::glow(palette::WISP, EYE_GLOW));
    commands.insert_resource(RigAssets { meshes: rigs, glow });
}

fn mirrored(right: Vec3) -> Vec3 {
    Vec3::new(right.x, right.y, -right.z)
}

/// Gives a fighter its moving parts and facing once it has a body.
fn add_rigs(
    mut commands: Commands,
    assets: Res<RigAssets>,
    new: Query<(Entity, &ClassId, &Pos, &MeshMaterial3d<StandardMaterial>), Added<Mesh3d>>,
) {
    for (player, class, pos, body) in &new {
        let Some(handles) = assets.meshes.get(class) else { continue };
        let mut part = |mesh: &Handle<Mesh>, material: &Handle<StandardMaterial>, at: Vec3| {
            let transform = Transform::from_translation(at);
            commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), transform)).id()
        };
        let own = &body.0;
        let rig = Rig {
            head: part(&handles.head, own, RIG_NECK),
            arms: [part(&handles.arm, own, RIG_SHOULDER), part(&handles.arm, own, mirrored(RIG_SHOULDER))],
            legs: [part(&handles.leg, own, RIG_HIP), part(&handles.leg, own, mirrored(RIG_HIP))],
            held: part(&handles.held, own, RIG_HAND),
            moves: moves(&class.def().id),
            stride: 0.0,
            walking: 0.0,
        };
        let eyes = part(&handles.eyes, &assets.glow, Vec3::ZERO);
        commands.entity(rig.head).add_child(eyes);
        commands.entity(rig.arms[0]).add_child(rig.held);
        commands.entity(player).add_children(&[rig.head, rig.arms[0], rig.arms[1], rig.legs[0], rig.legs[1]]);
        commands.entity(player).insert((rig, Facing { look: Vec2::X, last_pos: pos.0, moved: Vec2::ZERO }));
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

/// Poses each rig: facing (from `Facing`), limbs swinging as it walks, the windup's (stepped)
/// progress driving its class's strike, and its dash pose while dashing. The parts are posed in
/// the body's (twisted, leaning) space; the head (and a weapon held on target) undo that to stay
/// on the aim.
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

        // Where in the strike we are: drawing (0..1) then releasing (0..1) during the windup,
        // and the follow-through easing back to carrying after it; a thrown Q flicks through it.
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
        let dashing = if ability.dash.is_some() { 1.0 } else { 0.0 };
        let moves = rig.moves;
        let pose = |[carry, drawn, released, dash]: [f32; 4]| carry.lerp(drawn, draw).lerp(released, throw).lerp(dash, dashing);
        let braced = draw.max(throw).max(dashing);
        // A thrown weapon: the hand is empty until a new one is drawn, halfway through the cooldown.
        let rearm = (def.attack.cooldown_ticks - def.attack.windup_ticks) as f32 / 2.0;
        let empty_handed = matches!(def.attack.kind, AttackKind::Projectile { .. })
            && attack.windup.is_none()
            && since(attack.released_at(*class)).is_some_and(|s| s < rearm);

        let lean = Quat::from_rotation_y(pose(moves.twist)) * Quat::from_rotation_z(pose(moves.lean));
        let legs = [
            pose(moves.back_leg) - step * LEG_SWING * (1.0 - braced),
            pose(moves.lead_leg) + step * LEG_SWING * (1.0 - braced),
        ];
        // A wide stance lowers the hips (the legs are straight), so the feet stay on the ground.
        let spread = legs[0].abs().max(legs[1].abs());
        let mut posed = *body;
        posed.rotation = Quat::from_rotation_y(facing.look.to_angle()) * lean;
        posed.translation.y = -RIG_HIP.y * (1.0 - spread.cos());
        body.set_if_neq(posed);

        let unlean = lean.inverse();
        let weapon_arm = Quat::from_rotation_z(pose(moves.weapon_arm) - step * ARM_SWING * 0.3);
        let held = match moves.held {
            Held::OnTarget => weapon_arm.inverse() * unlean * Quat::from_rotation_z(-(FRAC_PI_2 - 0.12)),
            Held::InHand(forward) => Quat::from_rotation_z(PI + forward),
        };
        let rotations = [
            (rig.head, unlean * Quat::from_rotation_z(-pose(moves.head_dip))),
            (rig.arms[0], weapon_arm),
            (rig.arms[1], Quat::from_rotation_z(pose(moves.lead_arm) + step * ARM_SWING)),
            (rig.legs[0], Quat::from_rotation_z(legs[0])),
            (rig.legs[1], Quat::from_rotation_z(legs[1])),
            (rig.held, held),
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

#[cfg(test)]
mod tests {
    use arena_shared::protocol::ClassId;

    #[test]
    fn every_class_has_moves() {
        for class in ClassId::all() {
            super::moves(&class.def().id);
        }
    }
}
