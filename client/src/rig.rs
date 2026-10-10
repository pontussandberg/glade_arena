//! Fighters with moving parts (see `glade::fighter_rig`), animated smoothly but deliberately:
//! eased keyframes in distinct beats (draw, hold, a fast committed strike), a walk with a light
//! bob and sway, and every joint on a firm, nearly critically damped spring so motion is smoothed
//! without wobbling (a tail, if the class has one, swings on a looser one).
//! Every fighter turns to face where it walks, swings or dashes (`Facing`), acts out its attack
//! windup in step with the cast bar and strikes a pose while dashing; each class has its own
//! `Moves`.
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
//! The frost mage carries its staff upright at its side. The windup raises it high toward the
//! target, the off hand reaching out to gather the cold; the cast thrusts the crystal forward at
//! the target. A nova slams the staff down into the ground in a deep crouch, the off hand flung
//! back.
//!
//! Transforms are only written when they change, so a fighter standing still costs nothing.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use arena_shared::classes::{AbilityKind, AttackKind};
use arena_shared::protocol::*;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use lightyear::prelude::*;

use crate::feedback::AttackClock;
use crate::glade::{self, RIG_HAND, RIG_HIP, RIG_NECK, RIG_SHOULDER, RIG_TAIL};
use crate::render::shown;

pub struct RigPlugin;

impl Plugin for RigPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_rigs);
        app.add_systems(Update, (add_rigs, see_throws, turn_fighters, pose_rigs).chain().in_set(Posing));
    }
}

/// The spawn ticks of another fighter's latest auto-attack and Q throws, noted as their spears
/// appear. Spears fly on our own clock (where they really are), but the thrower is drawn a round
/// trip or so in the past: its body only gets to a throw after the spear is already flying. What
/// shows a throw (the arm, the hand going empty, the cast bar, the telegraph) goes by this
/// instead, so the javelin isn't drawn in the hand and in the air at once.
#[derive(Component, Default)]
pub(crate) struct SeenThrows {
    attack: Option<u32>,
    ability: Option<u32>,
}

impl SeenThrows {
    /// The windup to show: none once its spear is flying.
    pub(crate) fn windup(seen: Option<&Self>, attack: &AttackState) -> Option<Windup> {
        let thrown = seen.and_then(|seen| seen.attack);
        attack.windup.filter(|windup| thrown.is_none_or(|thrown| windup.started_at > thrown))
    }
}

fn see_throws(
    shots: Query<&Projectile, Added<Projectile>>,
    mut throwers: Query<(&PlayerId, &mut SeenThrows), Without<Predicted>>,
) {
    for shot in &shots {
        let Some((_, mut seen)) = throwers.iter_mut().find(|(id, _)| id.0 == shot.owner) else { continue };
        let latest = if shot.ability { &mut seen.ability } else { &mut seen.attack };
        *latest = (*latest).max(Some(shot.spawn_tick));
    }
}

/// Where fighters get turned and posed; what depends on a fighter's transform runs after it.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct Posing;

/// Walk cycles per meter walked, how far limbs swing (radians), how high the body bobs (meters)
/// and how far it sways side to side (radians).
const STRIDES_PER_METER: f32 = 0.4;
const LEG_SWING: f32 = 0.6;
const ARM_SWING: f32 = 0.4;
const BOB: f32 = 0.025;
const SWAY: f32 = 0.02;
/// The joints' springs: how stiff (natural frequency, rad/s) and how damped (1 = no overshoot).
/// Firm and nearly critically damped: motion is smoothed, not wobbly, and a strike still lands
/// on its tick.
const JOINT_STIFFNESS: f32 = 32.0;
const JOINT_DAMPING: f32 = 0.95;
/// The tail: looser, and swung by walking and turning.
const TAIL_STIFFNESS: f32 = 11.0;
const TAIL_DAMPING: f32 = 0.7;
const TAIL_WALK: f32 = 0.35;
const TAIL_TURN: f32 = 0.06;

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
    // (The fourth keyframe of each is the class's Q pose: a dash, or a nova's slam.)
    /// Throws its weapon with the auto-attack: the hand is empty until a new one is drawn.
    throws: bool,
    /// How big the fighter is drawn (1 = the joints in `glade`).
    scale: f32,
}

/// How the weapon sits in the hand.
enum Held {
    /// Kept pointing (a little above) the aim, whatever the arm and body do: a javelin ready to
    /// throw.
    OnTarget,
    /// Fixed in the fist, along the arm and this far (radians) forward of it: a sword.
    InHand(f32),
    /// Kept standing up, whatever the arm and body do, tipped this far (radians, keyframes like
    /// the joints') toward the facing: a staff.
    Upright([f32; 4]),
}

const JAVELINIST: Moves = Moves {
    weapon_arm: [-2.9, -2.8, 1.9, -2.1],
    lead_arm: [0.35, 1.5, -0.9, -0.8],
    lead_leg: [0.12, 0.55, 0.4, 0.6],
    back_leg: [-0.1, -0.5, -0.75, -0.6],
    // Released: the shoulder comes round just far enough to put the javelin square in front of
    // the chest, on the line it flies (see `HeldAt`), not across it.
    twist: [0.0, -1.0, 0.3, 0.0],
    lean: [0.0, 0.28, -0.38, -0.45],
    head_dip: [0.0, 0.2, -0.15, 0.0],
    held: Held::OnTarget,
    throws: true,
    scale: 1.12,
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
    throws: false,
    scale: 1.0,
};

const FROST_MAGE: Moves = Moves {
    weapon_arm: [0.3, 1.9, 1.25, 0.75],
    lead_arm: [-0.1, 1.25, 0.8, -1.0],
    lead_leg: [0.1, 0.3, 0.5, 0.55],
    back_leg: [-0.1, -0.3, -0.5, -0.5],
    twist: [0.0, -0.4, 0.35, 0.15],
    lean: [0.0, 0.12, -0.25, -0.35],
    head_dip: [0.0, 0.1, -0.15, -0.2],
    held: Held::Upright([0.08, -0.25, 1.0, -0.05]),
    throws: false,
    scale: 1.05,
};

/// The moves for a class's look (every class has one: `glade::FIGHTER_LOOKS`).
fn moves(class_key: &str) -> &'static Moves {
    match class_key {
        "javelinist" => &JAVELINIST,
        "revenant" => &REVENANT,
        "frost_mage" => &FROST_MAGE,
        _ => unreachable!("no moves for class {class_key:?}"),
    }
}

/// Where in the windup (0..1) the draw back ends and the strike starts: draw, a clear hold at
/// full draw, then a fast, committed strike.
const DRAW_END: f32 = 0.5;
const STRIKE_START: f32 = 0.78;
/// After the throw: how long (ticks) the follow-through is held, and when it's back to carrying.
const FOLLOW_THROUGH: (f32, f32) = (3.0, 14.0);
/// A thrown Q's flick: ticks after the throw until the arm starts and finishes coming back.
const FLICK: (f32, f32) = (1.0, 10.0);
/// A nova's slam: ticks after the cast until it starts and finishes rising out of it.
const SLAM: (f32, f32) = (8.0, 24.0);
/// How quickly a fighter turns toward where it wants to face, and eases in and out of its walk
/// (per second, exponential).
const TURN_RATE: f32 = 28.0;
const WALK_RATE: f32 = 10.0;
/// How brightly the eyes (and other glowing parts) glow.
const EYE_GLOW: f32 = 6.0;

/// Rig meshes for each class, and its glow (eyes, crystals: the same for everyone of a class).
/// Every other part is drawn in the fighter's own material (colors in the vertices), so a hit
/// flashes all of it.
#[derive(Resource)]
struct RigAssets {
    meshes: HashMap<ClassId, RigHandles>,
}

struct RigHandles {
    head: Handle<Mesh>,
    arm: Handle<Mesh>,
    leg: Handle<Mesh>,
    held: Handle<Mesh>,
    eyes: Handle<Mesh>,
    glow: Handle<StandardMaterial>,
    held_glow: Option<Handle<Mesh>>,
    body_glow: Option<Handle<Mesh>>,
    tail: Option<Handle<Mesh>>,
}

/// Which way a fighter faces, where it was last frame, and how far it moved and turned
/// (radians) since.
#[derive(Component)]
struct Facing {
    look: Vec2,
    last_pos: Vec2,
    moved: Vec2,
    turned: f32,
}

/// Where a fighter's held weapon is in the world in the pose it's heading for this frame (its
/// joints' targets, not where their springs have got to): its grip, the weapon pointing along
/// +Y. A thrown javelin leaves the hand from here (`render::fly_shots`): where the throw sends
/// it, out in front, rather than from an arm the springs still hold halfway through the swing.
#[derive(Component, Default, PartialEq)]
pub(crate) struct HeldAt(pub Transform);

/// A fighter's moving parts (child entities), its moves and walk state.
#[derive(Component)]
struct Rig {
    head: Entity,
    /// Right (weapon) arm and left (lead) arm.
    arms: [Entity; 2],
    /// Right (back) leg and left (lead) leg.
    legs: [Entity; 2],
    held: Entity,
    tail: Option<Entity>,
    moves: &'static Moves,
    /// Walk cycles so far, and how much the fighter is walking (0..1, eased).
    stride: f32,
    walking: f32,
    joints: Joints,
}

/// A value on a damped spring: chases its target with a little lag and overshoot, then settles.
#[derive(Default)]
struct Spring {
    at: f32,
    speed: f32,
}

impl Spring {
    fn follow(&mut self, target: f32, dt: f32, stiffness: f32, damping: f32) -> f32 {
        // Semi-implicit Euler, in small enough steps to stay stable at low frame rates.
        let steps = (dt / 0.008).ceil().max(1.0);
        let h = dt / steps;
        for _ in 0..steps as u32 {
            let pull = stiffness * stiffness * (target - self.at) - 2.0 * damping * stiffness * self.speed;
            self.speed += pull * h;
            self.at += self.speed * h;
        }
        // Settled: stop, so a still fighter stops writing transforms.
        if (target - self.at).abs() < 1e-4 && self.speed.abs() < 1e-3 {
            *self = Spring { at: target, speed: 0.0 };
        }
        self.at
    }
}

/// Every posed angle, each on its spring.
#[derive(Default)]
struct Joints {
    weapon_arm: Spring,
    lead_arm: Spring,
    back_leg: Spring,
    lead_leg: Spring,
    twist: Spring,
    lean: Spring,
    head_dip: Spring,
    tail_swing: Spring,
    tail_sway: Spring,
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
                glow: materials.add(glade::glow(rig.glow, EYE_GLOW)),
                held_glow: rig.held_glow.map(|mesh| meshes.add(mesh)),
                body_glow: rig.body_glow.map(|mesh| meshes.add(mesh)),
                tail: rig.tail.map(|tail| meshes.add(tail)),
            };
            (c, handles)
        })
        .collect();
    commands.insert_resource(RigAssets { meshes: rigs });
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
            tail: handles.tail.as_ref().map(|tail| part(tail, own, RIG_TAIL)),
            moves: moves(&class.def().id),
            stride: 0.0,
            walking: 0.0,
            joints: Joints::default(),
        };
        let eyes = part(&handles.eyes, &handles.glow, Vec3::ZERO);
        let held_glow = handles.held_glow.as_ref().map(|mesh| part(mesh, &handles.glow, Vec3::ZERO));
        let body_glow = handles.body_glow.as_ref().map(|mesh| part(mesh, &handles.glow, Vec3::ZERO));
        commands.entity(rig.head).add_child(eyes);
        commands.entity(rig.arms[0]).add_child(rig.held);
        if let Some(glow) = held_glow {
            commands.entity(rig.held).add_child(glow);
        }
        if let Some(glow) = body_glow {
            commands.entity(player).add_child(glow);
        }
        commands.entity(player).add_children(&[rig.head, rig.arms[0], rig.arms[1], rig.legs[0], rig.legs[1]]);
        if let Some(tail) = rig.tail {
            commands.entity(player).add_child(tail);
        }
        commands.entity(player).insert((
            rig,
            Facing { look: Vec2::X, last_pos: pos.0, moved: Vec2::ZERO, turned: 0.0 },
            HeldAt::default(),
            SeenThrows::default(),
        ));
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
        let turned = facing.look.angle_to(look);
        if facing.look != look || facing.last_pos != pos.0 || facing.moved != moved || facing.turned != turned {
            *facing = Facing { look, last_pos: pos.0, moved, turned };
        }
    }
}

/// 0 below `from`, 1 above `to`, smooth in between.
fn ease(from: f32, to: f32, x: f32) -> f32 {
    let t = ((x - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// 0 below `from`, 1 above `to`: fast at first, then slowing into place. A committed strike.
fn ease_out(from: f32, to: f32, x: f32) -> f32 {
    let t = 1.0 - ((x - from) / (to - from)).clamp(0.0, 1.0);
    1.0 - t * t * t
}

/// Poses each rig: facing (from `Facing`), a walk with bob and sway, the windup's progress
/// driving its class's strike, and its dash pose while dashing, every joint on its spring. The
/// parts are posed in the body's (twisted, leaning) space; the head (and a weapon held on target)
/// undo that to stay on the aim.
fn pose_rigs(
    time: Res<Time>,
    // Not there until we're synced with the server; the lobby's fighter is posed before that
    // (and it never attacks, so it doesn't need the clock).
    clock: Option<AttackClock>,
    mut rigs: Query<(
        &mut Rig,
        &Facing,
        &mut Transform,
        &mut HeldAt,
        &ClassId,
        &AttackState,
        &AbilityState,
        Option<&SeenThrows>,
        Has<Predicted>,
    )>,
    mut parts: Query<(&mut Transform, &mut Visibility), Without<Rig>>,
) {
    let ease_walk = rate(&time, WALK_RATE);
    let dt = time.delta_secs().min(0.1);
    for (mut rig, facing, mut body, mut held_at, class, attack, ability, seen, is_me) in &mut rigs {
        let rig = &mut *rig;
        rig.stride += facing.moved.length() * STRIDES_PER_METER;
        let is_walking = if facing.moved.length() > 0.001 { 1.0 } else { 0.0 };
        let walking = rig.walking + (is_walking - rig.walking) * ease_walk;
        rig.walking = if (walking - is_walking).abs() < 1e-3 { is_walking } else { walking };
        let phase = rig.stride.fract() * TAU;
        let step = phase.sin() * rig.walking;

        // Where in the strike we are: drawing (0..1), holding, then striking (0..1) during the
        // windup, and the follow-through easing back to carrying after it; a thrown Q
        // flicks through it.
        let def = class.def();
        let now = clock.as_ref().map_or(0.0, |clock| clock.now(is_me));
        let present = clock.as_ref().map_or(0.0, |clock| clock.now(true));
        // A throw whose spear we've seen counts from when it really happened, on our own clock.
        let since_throw = |tick: Option<u32>, seen: Option<u32>| match seen.filter(|&s| tick.is_none_or(|t| t <= s)) {
            Some(seen) => Some(present - seen as f32).filter(|s| *s >= 0.0),
            None => tick.map(|t| now - t as f32).filter(|s| *s >= 0.0),
        };
        let released = since_throw(attack.released_at, seen.and_then(|seen| seen.attack));
        let windup = SeenThrows::windup(seen, attack);
        let (draw, mut throw) = match (windup, released) {
            (Some(windup), _) => {
                let progress = windup.progress(now, *class);
                (ease(0.0, DRAW_END, progress), ease_out(STRIKE_START, 1.0, progress))
            }
            (None, Some(since)) => (0.0, 1.0 - ease(FOLLOW_THROUGH.0, FOLLOW_THROUGH.1, since)),
            (None, None) => (0.0, 0.0),
        };
        let flicked = since_throw(ability.used_at(*class), seen.and_then(|seen| seen.ability));
        let mut dashing = if ability.dash.is_some() { 1.0 } else { 0.0 };
        match (&def.ability.kind, flicked) {
            (AbilityKind::Projectile { .. }, Some(since)) => throw = throw.max(1.0 - ease(FLICK.0, FLICK.1, since)),
            // A nova's slam is its Q pose, like a dash's.
            (AbilityKind::Nova { .. }, Some(since)) => dashing = 1.0 - ease(SLAM.0, SLAM.1, since),
            _ => {}
        }
        let moves = rig.moves;
        let pose = |[carry, drawn, released, dash]: [f32; 4]| carry.lerp(drawn, draw).lerp(released, throw).lerp(dash, dashing);
        let braced = draw.max(throw).max(dashing);
        // A thrown weapon: the hand is empty until a new one is drawn, halfway through the cooldown.
        let rearm = (def.attack.cooldown_ticks - def.attack.windup_ticks) as f32 / 2.0;
        let empty_handed = rig.moves.throws
            && matches!(def.attack.kind, AttackKind::Projectile { .. })
            && windup.is_none()
            && released.is_some_and(|s| s < rearm);

        // Every angle chases its pose on a spring.
        let spring = |joint: &mut Spring, target: f32| joint.follow(target, dt, JOINT_STIFFNESS, JOINT_DAMPING);
        let joints = &mut rig.joints;
        let (twist_to, lean_to, weapon_arm_to) = (pose(moves.twist), pose(moves.lean), pose(moves.weapon_arm));
        let twist = spring(&mut joints.twist, twist_to);
        let lean = spring(&mut joints.lean, lean_to);
        let legs = [
            spring(&mut joints.back_leg, pose(moves.back_leg) - step * LEG_SWING * (1.0 - braced)),
            spring(&mut joints.lead_leg, pose(moves.lead_leg) + step * LEG_SWING * (1.0 - braced)),
        ];
        let weapon_arm = spring(&mut joints.weapon_arm, weapon_arm_to - step * ARM_SWING * 0.3);
        let lead_arm = spring(&mut joints.lead_arm, pose(moves.lead_arm) + step * ARM_SWING);
        let head_dip = spring(&mut joints.head_dip, pose(moves.head_dip));

        // The body: facing, twisted and leaning into the strike, swaying with each step; the hips
        // drop with a wide stance (the legs are straight, so the feet stay down) and bob as it
        // walks.
        let walk = rig.walking * (1.0 - braced);
        let sway = phase.sin() * SWAY * walk;
        let leaning = |twist: f32, lean: f32, sway: f32| {
            Quat::from_rotation_y(twist) * Quat::from_rotation_z(lean) * Quat::from_rotation_x(sway)
        };
        let facing_turn = Quat::from_rotation_y(facing.look.to_angle());
        let lean = leaning(twist, lean, sway);
        let spread = legs[0].abs().max(legs[1].abs());
        let mut posed = *body;
        posed.rotation = facing_turn * lean;
        posed.scale = Vec3::splat(moves.scale);
        posed.translation.y = (-RIG_HIP.y * (1.0 - spread.cos()) + (2.0 * phase).cos().abs() * BOB * walk) * moves.scale;
        body.set_if_neq(posed);

        // The tail swings back as it walks and out to the side as it turns, loosely.
        let turning = facing.turned / dt.max(1e-3);
        let tail = rig.tail.map(|tail| {
            let swing = rig.joints.tail_swing.follow(-TAIL_WALK * rig.walking, dt, TAIL_STIFFNESS, TAIL_DAMPING);
            let sway = rig.joints.tail_sway.follow(-TAIL_TURN * turning, dt, TAIL_STIFFNESS, TAIL_DAMPING);
            (tail, Quat::from_rotation_z(swing) * Quat::from_rotation_x(sway))
        });

        let unlean = lean.inverse();
        let weapon_arm = Quat::from_rotation_z(weapon_arm);
        let held_in = |weapon_arm: Quat, unlean: Quat| match moves.held {
            Held::OnTarget => weapon_arm.inverse() * unlean * Quat::from_rotation_z(-(FRAC_PI_2 - 0.12)),
            Held::InHand(forward) => Quat::from_rotation_z(PI + forward),
            Held::Upright(tip) => weapon_arm.inverse() * unlean * Quat::from_rotation_z(-pose(tip)),
        };
        let held = held_in(weapon_arm, unlean);
        let rotations = [
            (rig.head, unlean * Quat::from_rotation_z(-head_dip)),
            (rig.arms[0], weapon_arm),
            (rig.arms[1], Quat::from_rotation_z(lead_arm)),
            (rig.legs[0], Quat::from_rotation_z(legs[0])),
            (rig.legs[1], Quat::from_rotation_z(legs[1])),
            (rig.held, held),
        ];
        // For a fighter that throws its weapon: the held weapon's world pose in the pose the joints
        // are heading for (see `HeldAt`), rebuilt from the joints it hangs from (it's a child of
        // the weapon arm at the hand, see `add_rigs`): child transforms aren't propagated until
        // after this frame's shots are placed.
        if moves.throws {
            let aimed_lean = leaning(twist_to, lean_to, 0.0);
            let aimed_arm = Quat::from_rotation_z(weapon_arm_to);
            let aimed_body = Transform { rotation: facing_turn * aimed_lean, ..posed };
            let hand = Transform::from_translation(RIG_SHOULDER).with_rotation(aimed_arm)
                * Transform::from_translation(RIG_HAND).with_rotation(held_in(aimed_arm, aimed_lean.inverse()));
            held_at.set_if_neq(HeldAt(aimed_body * hand));
        }
        for (part, rotation) in rotations.into_iter().chain(tail) {
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
