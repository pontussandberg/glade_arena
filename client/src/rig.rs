//! Fighters with moving parts (see `arena::fighter_rig`), animated smoothly but deliberately:
//! eased keyframes in distinct beats (draw, hold, a fast committed strike), a walk with a light
//! bob and sway, and every joint on a firm, nearly critically damped spring so motion is smoothed
//! without wobbling (cloth that moves on its own is `cloth.rs`).
//! Every fighter turns to face where it walks, swings or dashes (`Facing`), acts out its attack
//! and strikes a pose while dashing; each class has its own `Moves`.
//!
//! An attack is timed to the server's: the draw and the strike fill its windup, and the
//! strike is `Moves::contact` of the way through as it goes off (the spear leaving the hand, the
//! blade's tip just reaching the target). The rest of the strike and the follow-through are the
//! backswing, which walking cuts short (`CUT_RATE`). Nothing on screen says when the attack has
//! gone off but the fighter itself. A quicker attack (`follow_up`) plays all of it faster.
//!
//! The javelinist always carries the javelin cocked over the shoulder, ready to throw. The
//! windup: twist the throwing shoulder far back, lean back and sink into a wide stance, lead arm
//! pointing at the target, while drawing the javelin further back; then lunge forward through
//! the throw, the arm whipping over the top, hold the follow-through for a moment and settle
//! back. Head and javelin stay on the target throughout. The hand is empty until a new javelin
//! is drawn, halfway through the cooldown. A thrown Q is a quick flick of the arm through the
//! same throw.
//!
//! The revenant drags its great blade behind it, the point trailing just above the ground. Its
//! attack is a heavy overhead blow: the windup heaves the blade up from behind and over its
//! head, leaning back with the lead hand thrown out at the target; the strike brings it down
//! over the top in one great arc as the lead leg steps long into it and the body crunches in
//! behind it, the blade smearing out longer, and on into the ground, where it leaves a crack
//! (`swish.rs`). It lands as the blade comes level, out in front. A hit empowers the blade
//! (Follow Through): it grows half as big again while that's armed and through the empowered
//! attack (`EMPOWERED_BLADE`), which is the same blow, faster, smearing out further still to
//! reach its longer lane (`EMPOWERED_STRETCH`). A strike started before the last one has settled chains onto it: the blade comes
//! straight back into the draw instead of sinking to the carry in between. A dash is a lunge,
//! the blade driven out ahead, held all the way through it; a dash taken mid-windup leaves the
//! windup be, the fighter gliding along in its stance, facing the aim.
//!
//! The frost mage carries its staff upright at its side. The windup raises it high toward the
//! target, the off hand reaching out to gather the cold; the cast thrusts the crystal forward at
//! the target. A nova slams the staff down into the ground in a deep crouch, the off hand flung
//! back.
//!
//! Transforms are only written when they change, so a fighter standing still costs nothing.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use arena_shared::classes::{AbilityKind, AttackKind};
use arena_shared::config::TICK_HZ;
use arena_shared::protocol::*;
use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use crate::Me;
use crate::cloth::WardrobeAssets;
use crate::feedback::AttackClock;
use crate::arena::{self, RIG_HAND, RIG_HIP, RIG_NECK, RIG_SHOULDER};
use crate::render::shown;

pub struct RigPlugin;

impl Plugin for RigPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((crate::cloth::ClothPlugin, crate::swish::SwishPlugin));
        app.add_systems(Startup, load_rigs);
        app.add_systems(Update, (add_rigs, see_throws, turn_fighters, pose_rigs, show_auras, gleam_blades).chain().in_set(Posing));
    }
}

/// The spawn ticks of another fighter's latest auto-attack and Q throws, noted as their spears
/// appear. Spears fly on our own clock (where they really are), but the thrower is drawn a round
/// trip or so in the past: its body only gets to a throw after the spear is already flying. What
/// shows a throw (the arm, the hand going empty, the cast bar) goes by this
/// instead, so the javelin isn't drawn in the hand and in the air at once.
#[derive(Component, Default)]
pub(crate) struct SeenThrows {
    attack: Option<u32>,
    ability: Option<u32>,
}

impl SeenThrows {
    /// The attack winding up to show: none once its spear is flying.
    pub(crate) fn windup(seen: Option<&Self>, attack: &AttackState) -> Option<Swing> {
        let thrown = seen.and_then(|seen| seen.attack);
        attack.winding_up().filter(|windup| thrown.is_none_or(|thrown| windup.started_at > thrown))
    }
}

fn see_throws(
    shots: Query<&Projectile, Added<Projectile>>,
    mut throwers: Query<(&PlayerId, &mut SeenThrows), Without<Me>>,
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

/// How a rigged class moves. Keyframes are (carrying, fully drawn back, released, dashing),
/// radians. Limbs: positive swings forward. Twist: negative turns the weapon (right, +Z)
/// shoulder back. Lean: positive leans back (along the facing, whatever the twist).
pub(crate) struct Moves {
    weapon_arm: [f32; 4],
    /// How much of the body's twist the weapon arm undoes (0 to 1), so it swings along the
    /// facing rather than across the twisted chest: an overhand throw draws straight back behind
    /// the shoulder and whips straight through, instead of round behind the head.
    weapon_arm_untwist: f32,
    /// The weapon arm goes right round with each strike (a windmill: up behind, over the top,
    /// down ahead and on round back to the carry), rather than back the way it came.
    pub(crate) windmill: bool,
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
    /// How big the fighter is drawn (1 = the joints in `arena`).
    scale: f32,
    /// How much longer the weapon is drawn for a moment as a strike lands (a fraction of its
    /// length), the hand pushed out with it: a fighting game's smear, so the blade's tip reaches
    /// as far as the strike does.
    pub(crate) stretch: f32,
    /// Where in the windup (0..1) the draw back ends, and how many ticks of it the strike at its
    /// end takes (at most the rest of it): a hold at full draw in between, if there's time.
    pub(crate) draw_end: f32,
    pub(crate) strike_ticks: f32,
    /// A heavy strike: gathering speed all the way into its end, where it stops dead, rather
    /// than snapping out and slowing into place.
    pub(crate) heavy: bool,
    /// After the strike is done: how long (ticks) its end is held, and when it's back to
    /// carrying (`FOLLOW_THROUGH` for most).
    pub(crate) follow_through: (f32, f32),
    /// How stiff its joints' springs are (natural frequency, rad/s; `JOINT_STIFFNESS` for most):
    /// stiffer is more solid, with less lag and give.
    pub(crate) stiffness: f32,
    /// How damped its joints' springs are (1: none overshoot; `JOINT_DAMPING` for most).
    pub(crate) damping: f32,
    /// How much the head turns and leans with the body (`HEAD_FOLLOW` for most; 1: wholly).
    head_follow: f32,
    /// The weapon arm's swing turned about the upright (radians, in the facing's frame; positive
    /// turns it in across the body): a blow in one straight plane, aimed where it lands.
    swing_yaw: f32,
    /// How far through the strike (0 to 1, above 0) it is when the attack goes off: the rest of
    /// it plays in the backswing, where walking cuts it short. A thrust: the tip just reaching
    /// the target's body. A throw: the spear leaving the hand. Tuned by eye.
    contact: f32,
}

/// How the weapon sits in the hand.
pub(crate) enum Held {
    /// Kept pointing (a little above) the aim, whatever the arm and body do: a javelin ready to
    /// throw.
    OnTarget,
    /// Kept standing up, whatever the arm and body do, tipped this far (radians, keyframes like
    /// the joints') toward the facing: a staff.
    Upright([f32; 4]),
    /// Gripped in the fist, the blade coming out of its thumb side, square to the forearm, so it
    /// goes wherever the arm takes it: bent at the wrist (`wrist`, keyframes like the joints':
    /// positive bends the blade toward the forearm's line, negative cocks it back toward the
    /// shoulder) and turned with the forearm about its own length (`roll`: 0, the blade out
    /// ahead of a hanging arm; a half turn, out behind it). A heavy blade swung with the arm.
    Gripped { wrist: [f32; 4], roll: [f32; 4] },
}

const JAVELINIST: Moves = Moves {
    weapon_arm: [-2.9, -2.8, 1.9, -2.1],
    weapon_arm_untwist: 1.0,
    windmill: false,
    lead_arm: [0.35, 1.5, -0.9, -0.8],
    lead_leg: [0.12, 0.55, 0.4, 0.6],
    back_leg: [-0.1, -0.5, -0.75, -0.6],
    // Drawn: the shoulder turned back only so far that the javelin, held on target, stays clear
    // of the head. Released: the shoulder comes round, the javelin in front of the chest.
    twist: [0.0, -0.8, 0.55, 0.0],
    lean: [0.0, 0.28, -0.38, -0.45],
    head_dip: [0.0, 0.2, -0.15, 0.0],
    held: Held::OnTarget,
    throws: true,
    scale: 1.16,
    stretch: 0.0,
    draw_end: DRAW_END,
    strike_ticks: STRIKE_TICKS,
    heavy: false,
    follow_through: FOLLOW_THROUGH,
    stiffness: JOINT_STIFFNESS,
    damping: JOINT_DAMPING,
    head_follow: HEAD_FOLLOW,
    swing_yaw: 0.0,
    contact: 1.0,
};

const REVENANT: Moves = Moves {
    // A windmill: the arm swings back and up behind (the blade lifted off the ground behind it)
    // to over the head, then on over the top, down ahead into the ground, and round past the leg
    // back to the drag. The dash (the fourth of each) is a lunge, held all the way through it:
    // the blade driven out ahead, the weapon shoulder leading.
    weapon_arm: [-0.35, -3.35, -6.03, 1.45],
    // The arm swings in its own plane, whatever the body's twist: a straight blow.
    weapon_arm_untwist: 1.0,
    windmill: true,
    // The free arm hangs loose at the side throughout, drifting back past the hip as the
    // glaive goes up, and swung further back as it comes down.
    lead_arm: [-0.2, -0.45, -0.9, -0.9],
    // Legs hang from the leaning body: bowed into the blow, the lead leg reaches far forward
    // and the back one, straight under the hips, braces back.
    lead_leg: [0.25, 0.25, 1.85, 0.7],
    back_leg: [-0.2, -0.6, -0.45, -0.75],
    // Side on, the weapon shoulder back; wound further round, then turning into the blow.
    twist: [-0.8, -0.5, 0.8, 0.5],
    lean: [-0.1, 0.2, -0.7, -0.5],
    head_dip: [0.1, 0.2, -0.45, -0.15],
    // Dragged: the thumb (and blade) back, the blade trailing low behind. Drawn: straight up
    // over the head, in line with the arm (so the grip turns over on the way up without the
    // blade swinging about). Struck: in one plane over the top, the wrist coming through, the
    // blade flat on the ground ahead. Unwinding: the grip turning back over out to the right
    // as the arm comes round to the drag.
    held: Held::Gripped { wrist: [0.85, FRAC_PI_2, -0.45, 1.3], roll: [-PI, 0.0, 0.0, 0.0] },
    throws: false,
    scale: 1.13,
    stretch: 0.15,
    // Heaved up overhead quickly, held there a good moment, and a heavy blow coming over slow
    // and gathering speed into the ground; solid, the joints firm.
    draw_end: 0.35,
    strike_ticks: 3.0,
    heavy: true,
    // Planted in the ground a good while (unless walking off cuts it short), then hauled back
    // round to the drag, about as the next attack is ready.
    follow_through: (9.0, 21.0),
    stiffness: 60.0,
    // Critically damped: firm, nothing overshoots.
    damping: 1.0,
    // Head and shoulders go into the blow together.
    head_follow: 0.75,
    // The blow's plane turned in from the weapon shoulder to the middle of its lane, where the
    // blade lands.
    swing_yaw: 0.13,
    // As the whole blade comes down flat on the ground along the lane, the hand almost on it.
    contact: 0.97,
};

const FROST_MAGE: Moves = Moves {
    weapon_arm: [0.3, 1.9, 1.25, 0.75],
    weapon_arm_untwist: 0.0,
    windmill: false,
    lead_arm: [-0.1, 1.25, 0.8, -1.0],
    lead_leg: [0.1, 0.3, 0.5, 0.55],
    back_leg: [-0.1, -0.3, -0.5, -0.5],
    twist: [0.0, -0.4, 0.35, 0.15],
    lean: [0.0, 0.12, -0.25, -0.35],
    head_dip: [0.0, 0.1, -0.15, -0.2],
    held: Held::Upright([0.08, -0.25, 1.0, -0.05]),
    throws: false,
    scale: 1.05,
    stretch: 0.0,
    draw_end: DRAW_END,
    strike_ticks: STRIKE_TICKS,
    heavy: false,
    follow_through: FOLLOW_THROUGH,
    stiffness: JOINT_STIFFNESS,
    damping: JOINT_DAMPING,
    head_follow: HEAD_FOLLOW,
    swing_yaw: 0.0,
    contact: 0.9,
};

/// The moves for a class's look (every class has one: `arena::FIGHTER_LOOKS`).
pub(crate) fn moves(class_key: &str) -> &'static Moves {
    match class_key {
        "javelinist" => &JAVELINIST,
        "revenant" => &REVENANT,
        "frost_mage" => &FROST_MAGE,
        _ => unreachable!("no moves for class {class_key:?}"),
    }
}

/// Where in the windup (0..1) the draw back ends, and how many ticks of the windup the strike at
/// its end takes (at most the rest of it), unless its moves say otherwise (`Moves::draw_end`,
/// `Moves::strike_ticks`): draw, a hold at full draw if there's time, then a fast, committed
/// strike, however long the windup. Ticks here and below are at normal attack speed: a quicker
/// attack plays them faster.
const DRAW_END: f32 = 0.5;
const STRIKE_TICKS: f32 = 2.5;
/// How long (ticks) a strike's smear takes to snap back once the strike is done
/// (`Moves::stretch`), and how much further down the arm the hand is pushed with it (a fraction
/// of the stretch).
const STRETCH_TICKS: f32 = 3.0;
const SMEAR_REACH: f32 = 0.4;
/// A thrown weapon: ticks after a throw until a new one is in the hand.
const REARM_TICKS: f32 = 6.0;
/// After the strike is done: how long (ticks) the follow-through is held, and when it's back to
/// carrying.
const FOLLOW_THROUGH: (f32, f32) = (1.5, 7.0);
/// An empowered weapon (`follow_up`): how much bigger it grows, how quickly (per second,
/// exponential), and how much further its strike smears out, so it reaches its lane's far end
/// (`FollowUp::reach`).
pub(crate) const EMPOWERED_BLADE: f32 = 1.4;
const EMPOWER_RATE: f32 = 12.0;
pub(crate) const EMPOWERED_STRETCH: f32 = 0.05;
/// How much a gripped blade grown to twice its size is tipped up (radians, in proportion).
const BIG_BLADE_LIFT: f32 = 0.09;
/// How quickly walking off cuts the backswing short (per second, exponential): the
/// follow-through blends out into the walk.
const CUT_RATE: f32 = 30.0;
/// A melee windup started during the last strike's follow-through: how long (a fraction of the
/// windup) what's left of the strike is still held before the blade comes back into the draw.
const CHAIN_HOLD: f32 = 0.15;
/// A thrown Q's flick: ticks after the throw until the arm starts and finishes coming back.
const FLICK: (f32, f32) = (0.5, 5.0);
/// A nova's slam: ticks after the cast until it starts and finishes rising out of it.
const SLAM: (f32, f32) = (4.0, 12.0);
/// How quickly a fighter turns toward where it wants to face, and eases in and out of its walk
/// (per second, exponential).
const TURN_RATE: f32 = 40.0;
const WALK_RATE: f32 = 10.0;
/// How much the head turns and leans with the body (0: it stays square on the aim), so it moves
/// with the shoulders through a strike instead of hanging still while the body turns under it.
const HEAD_FOLLOW: f32 = 0.3;
/// How brightly the eyes (and other glowing parts) glow.
const EYE_GLOW: f32 = 6.0;

/// How see-through a weapon's aura is, and how fast (radians per second) and how much it swells.
const AURA_ALPHA: f32 = 0.42;
const AURA_PULSE: (f32, f32) = (9.0, 0.15);
/// A blade's gleam: how long between gleams (seconds, at least and at most), how long one lasts,
/// how big it gets (at its least and most, of the glint's size), and how far it turns (radians):
/// quickest coming in and going out, settling still as it shines its brightest.
const GLEAM_EVERY: (f32, f32) = (1.6, 2.4);
const GLEAM_SECONDS: f32 = 0.7;
const GLEAM_SIZE: (f32, f32) = (0.25, 0.75);
const GLEAM_SPIN: f32 = 2.4;
/// Where in a gleam (0..1) it reaches its height and leaves it, and how much it squishes while
/// it's there (a fraction of its size).
const GLEAM_HOLD: (f32, f32) = (0.35, 0.65);
const GLEAM_SQUISH: f32 = 0.3;

/// When a blade gleams next, and the gleam under way: from when, where on the blade (in its own
/// space), how bright; `seed` picks the next.
#[derive(Default)]
struct Gleaming {
    next: f32,
    from: f32,
    at: Vec3,
    peak: f32,
    seed: u32,
}

/// How fast the glint twinkles (radians per second) and spins (radians per second).
const GLINT_TWINKLE: f32 = 7.0;
const GLINT_SPIN: f32 = 2.5;

/// Rig meshes for each class, and its glow (crystals: the same for everyone of a class), and the
/// cold white everyone's eyes burn with. Every other part is drawn in the fighter's own material
/// (colors in the vertices), so a hit flashes all of it.
#[derive(Resource)]
struct RigAssets {
    meshes: HashMap<ClassId, RigHandles>,
    eye_glow: Handle<StandardMaterial>,
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
    held_aura: Option<(Handle<Mesh>, Handle<StandardMaterial>)>,
    held_glint: Option<(Handle<Mesh>, Handle<StandardMaterial>, Vec3)>,
    wardrobe: Option<WardrobeAssets>,
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
    /// The held weapon's aura (`RigMeshes::held_aura`), if it has one.
    aura: Option<Entity>,
    /// And the glint at its point.
    glint: Option<Entity>,
    /// A gleam running along a gripped blade's edge now and then (`gleam_blades`), and when.
    gleam: Option<Entity>,
    gleaming: Gleaming,
    moves: &'static Moves,
    /// Walk cycles so far, and how much the fighter is walking (0..1, eased).
    stride: f32,
    walking: f32,
    /// The backswing cut short by walking off: the attack's `released_at`, and how far it's
    /// blended out (0..1).
    cut: Option<(u32, f32)>,
    /// How far its weapon is grown, empowered by its attack's `follow_up` (0..1, eased).
    empowered: f32,
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
pub(crate) struct Joints {
    weapon_arm: Spring,
    lead_arm: Spring,
    back_leg: Spring,
    lead_leg: Spring,
    twist: Spring,
    lean: Spring,
    head_dip: Spring,
    /// The held weapon's tip (`Held::Upright`) or wrist (`Held::Gripped`): an
    /// angle, which may come right round, so it takes the nearest way to its target.
    held_tip: Spring,
    /// The forearm's roll (`Held::Gripped`).
    roll: Spring,
}

impl Joints {
    /// Every angle a step of `dt` (seconds) closer to `to` on its spring, as stiff as `moves`
    /// say; a windmilling weapon arm (`Moves::windmill`), like the held weapon's tip, the nearest
    /// way round.
    pub(crate) fn follow(&mut self, to: &Angles, dt: f32, moves: &Moves) -> Angles {
        let spring = |joint: &mut Spring, target: f32| joint.follow(target, dt, moves.stiffness, moves.damping);
        self.held_tip.at += TAU * ((to.tip - self.held_tip.at) / TAU).round();
        if moves.windmill {
            self.weapon_arm.at += TAU * ((to.weapon_arm - self.weapon_arm.at) / TAU).round();
        }
        Angles {
            twist: spring(&mut self.twist, to.twist),
            lean: spring(&mut self.lean, to.lean),
            weapon_arm: spring(&mut self.weapon_arm, to.weapon_arm),
            lead_arm: spring(&mut self.lead_arm, to.lead_arm),
            back_leg: spring(&mut self.back_leg, to.back_leg),
            lead_leg: spring(&mut self.lead_leg, to.lead_leg),
            head_dip: spring(&mut self.head_dip, to.head_dip),
            tip: spring(&mut self.held_tip, to.tip),
            roll: spring(&mut self.roll, to.roll),
        }
    }
}

/// How far into each of its poses a rig is (0 to 1 each, layered in this order): drawn back,
/// struck (released), and its Q pose.
#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct Phase {
    pub(crate) draw: f32,
    pub(crate) throw: f32,
    pub(crate) dashing: f32,
}

impl Phase {
    /// A joint's angle from its keyframes (carrying, drawn back, released, Q).
    fn pose(&self, [carry, drawn, released, dash]: [f32; 4]) -> f32 {
        carry.lerp(drawn, self.draw).lerp(released, self.throw).lerp(dash, self.dashing)
    }

    /// How far it's braced in any of them (0: carrying, free to walk).
    fn braced(&self) -> f32 {
        self.draw.max(self.throw).max(self.dashing)
    }
}

/// The angles a rig is posed by (radians, as in `Moves`).
#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct Angles {
    pub(crate) twist: f32,
    pub(crate) lean: f32,
    pub(crate) weapon_arm: f32,
    pub(crate) lead_arm: f32,
    pub(crate) back_leg: f32,
    pub(crate) lead_leg: f32,
    pub(crate) head_dip: f32,
    /// The held weapon's tip, or its wrist (`Held`).
    pub(crate) tip: f32,
    /// The forearm's roll (`Held::Gripped`).
    pub(crate) roll: f32,
}

/// The angles `moves` pose a rig by at `phase`, its limbs swinging by `step` as it walks (-1 to
/// 1: where in its stride, and how much it's walking).
pub(crate) fn targets(moves: &Moves, phase: Phase, step: f32) -> Angles {
    let pose = |keys| phase.pose(keys);
    let legs = step * LEG_SWING * (1.0 - phase.braced());
    let (tip, roll) = match moves.held {
        Held::OnTarget => (0.0, 0.0),
        // After a strike that took it right round (over the top), it settles back to the carry
        // the same way round, not back over the top.
        Held::Upright([carry, drawn, released, dash]) => {
            let carry = if phase.draw > 0.0 { carry } else { carry + TAU * ((released - carry) / TAU).round() };
            (pose([carry, drawn, released, dash]), 0.0)
        }
        Held::Gripped { wrist, roll } => (pose(wrist), pose(roll)),
    };
    // A windmill's arm settles back to the carry on round the way it was going.
    let [carry, drawn, released, dash] = moves.weapon_arm;
    let carry = if moves.windmill && phase.draw == 0.0 { carry + TAU * ((released - carry) / TAU).round() } else { carry };
    Angles {
        twist: pose(moves.twist),
        lean: pose(moves.lean),
        weapon_arm: pose([carry, drawn, released, dash]) - step * ARM_SWING * 0.3,
        lead_arm: pose(moves.lead_arm) + step * ARM_SWING,
        back_leg: pose(moves.back_leg) - legs,
        lead_leg: pose(moves.lead_leg) + legs,
        head_dip: pose(moves.head_dip),
        tip,
        roll,
    }
}

/// A rig's pose, put together from its angles: the body (turned to its look, twisted, leaning,
/// swaying, its hips dropped by a wide stance and bobbing as it walks: only its height is set
/// here), each part's rotation on it, and the held weapon's place, turn and stretch on its arm.
pub(crate) struct Posed {
    pub(crate) body: Transform,
    pub(crate) head: Quat,
    /// Weapon arm, lead arm.
    pub(crate) arms: [Quat; 2],
    /// Back leg, lead leg.
    pub(crate) legs: [Quat; 2],
    pub(crate) held: Transform,
    /// The held weapon in the pose the joints are heading for (`aimed`), on the body as placed
    /// (only its height): see `HeldAt`.
    pub(crate) held_aimed: Transform,
}

/// Puts a rig together from its `angles` (and from `aimed`, the ones its joints are heading
/// for), facing `look`, `walk` of the way into its walk at `stride` (radians) through its
/// stride, its weapon stretched by `smear` (`Moves::stretch` already in it) and `blade` times
/// its size.
#[allow(clippy::too_many_arguments)]
pub(crate) fn assemble(moves: &Moves, angles: &Angles, aimed: &Angles, look: Vec2, stride: f32, walk: f32, smear: f32, blade: f32) -> Posed {
    let sway = stride.sin() * SWAY * walk;
    // Leaning along the facing, then twisting: leaning back about the twisted body's own axis
    // would tip it sideways, across the line of the strike.
    let leaning = |twist: f32, lean: f32, sway: f32| Quat::from_rotation_z(lean) * Quat::from_rotation_y(twist) * Quat::from_rotation_x(sway);
    let arm_turn = |twist: f32, swing: f32| Quat::from_rotation_y(moves.swing_yaw - moves.weapon_arm_untwist * twist) * Quat::from_rotation_z(swing);
    let facing_turn = Quat::from_rotation_y(look.to_angle());
    let lean = leaning(angles.twist, angles.lean, sway);
    // The legs swing along the facing, however the body twists above them (the hips turn less
    // than the shoulders): a lunge stays a lunge, not a split.
    let leg_turn = |leg: f32| Quat::from_rotation_y(-angles.twist) * Quat::from_rotation_z(leg);
    // Standing on the lower foot, however the body leans and the legs spread (the legs are
    // straight, as long as the hips are high, and hang from the leaning body).
    let foot = |hip: Vec3, leg: f32| (lean * (hip + leg_turn(leg) * Vec3::NEG_Y * RIG_HIP.y)).y;
    let lowest = foot(RIG_HIP, angles.back_leg).min(foot(RIG_HIP.with_z(-RIG_HIP.z), angles.lead_leg));
    let body = Transform {
        translation: Vec3::Y * (-lowest + (2.0 * stride).cos().abs() * BOB * walk) * moves.scale,
        rotation: facing_turn * lean,
        scale: Vec3::splat(moves.scale),
    };
    let unlean = lean.inverse();
    let weapon_arm = arm_turn(angles.twist, angles.weapon_arm);
    // The pose the joints are heading for (not where their springs have got to): the body's
    // lean and the weapon arm, and where the hand is (in the facing's frame, unscaled).
    let (aimed_lean, aimed_arm) = (leaning(aimed.twist, aimed.lean, 0.0), arm_turn(aimed.twist, aimed.weapon_arm));
    // A gripped blade as its arm holds it: a bigger one tipped up a little, so it still lands
    // along the ground rather than into it.
    let lift = (blade - 1.0) * BIG_BLADE_LIFT;
    let gripped = |angles: &Angles| Quat::from_rotation_y(angles.roll) * Quat::from_rotation_z(-(FRAC_PI_2 + angles.tip - lift));
    let held_in = |weapon_arm: Quat, unlean: Quat, angles: &Angles| match moves.held {
        Held::OnTarget => weapon_arm.inverse() * unlean * Quat::from_rotation_z(-(FRAC_PI_2 - 0.12)),
        Held::Upright(_) => weapon_arm.inverse() * unlean * Quat::from_rotation_z(-angles.tip),
        // In the arm's plane: it swings with the arm (`Moves::swing_yaw` aims it).
        Held::Gripped { .. } => gripped(angles),
    };
    // The held weapon's world pose in the pose the joints are heading for, rebuilt from the joints
    // it hangs from (it's a child of the weapon arm at the hand, see `add_rigs`).
    let aimed_body = Transform { rotation: facing_turn * aimed_lean, ..body };
    let held_aimed = aimed_body
        * Transform::from_translation(RIG_SHOULDER).with_rotation(aimed_arm)
        * Transform::from_translation(RIG_HAND).with_rotation(held_in(aimed_arm, aimed_lean.inverse(), aimed));
    Posed {
        body,
        head: unlean.slerp(Quat::IDENTITY, moves.head_follow) * Quat::from_rotation_z(-angles.head_dip),
        arms: [weapon_arm, Quat::from_rotation_z(angles.lead_arm)],
        legs: [leg_turn(angles.back_leg), leg_turn(angles.lead_leg)],
        held: Transform {
            translation: RIG_HAND * (1.0 + SMEAR_REACH * smear),
            rotation: held_in(weapon_arm, unlean, angles),
            scale: Vec3::new(1.0, 1.0 + smear, 1.0) * blade,
        },
        held_aimed,
    }
}

/// The attack as the joints' targets see it at `now`: as far ahead of its own clock as the
/// joints lag behind their targets on their springs (`Moves::stiffness`), so they're where the
/// attack is when it is, the strike reaching its contact pose as the attack goes off, not a tick
/// or two after. A windup that will have gone off by then has (`windup` and `released` as for
/// `strike_phase`).
pub(crate) fn ahead_of_springs(class: ClassId, moves: &Moves, windup: Option<Swing>, released: Option<f32>, now: f32) -> (Option<Swing>, Option<f32>, f32) {
    let lead = 2.0 * moves.damping / moves.stiffness * TICK_HZ as f32;
    let ahead = now + lead;
    match windup {
        Some(swing) if ahead >= swing.releases_at(class) as f32 => (None, Some((ahead - swing.releases_at(class) as f32) * swing.speed(class)), ahead),
        _ => (windup, released.map(|since| since + lead), ahead),
    }
}

/// Where in its attack a fighter of `class` posed by `moves` is at `now` (a fractional tick):
/// (draw, throw, smear) for its `windup` in progress (if any), or the attack that went off
/// `released` ticks ago (at normal attack speed). Draw: drawing back (0..1), held there, then
/// striking during the windup, up to the contact pose as the attack goes off; then the rest of
/// the strike, and the follow-through easing back to carrying. Smear: how far the weapon is
/// stretched out (0..1), longest as the strike is done.
pub(crate) fn strike_phase(class: ClassId, moves: &Moves, attack: &AttackState, windup: Option<Swing>, released: Option<f32>, now: f32) -> (f32, f32, f32) {
    let def = class.def();
    let strike_ticks = |swing: &Swing| (moves.strike_ticks / swing.speed(class)).min(swing.windup_ticks(class) as f32 * (1.0 - moves.draw_end));
    // A melee windup picks up from how much of the last strike's follow-through was left when
    // it started (1: still fully held; 0: settled back to carrying): the blade comes from
    // there straight into the draw, never sinking further toward carrying.
    let follow_left = |windup: &Swing| match (&def.attack.kind, attack.released_at) {
        (AttackKind::Melee { .. }, Some(at)) if windup.started_at >= at => {
            1.0 - ease(FOLLOW_THROUGH.0, FOLLOW_THROUGH.1, (windup.started_at - at) as f32)
        }
        _ => 0.0,
    };
    match (windup, released) {
        (Some(windup), _) => {
            let progress = windup.windup_progress(now, class);
            let windup_ticks = windup.windup_ticks(class) as f32;
            let striking = strike_ticks(&windup);
            let (strike, _) = strike_at(striking - (1.0 - progress) * windup_ticks, striking, moves.contact, moves.heavy);
            // What's left of the strike eases out over the draw, and how much the pose
            // carries only ever shrinks, from what it was when the windup began.
            let follow = follow_left(&windup);
            let held = follow * (1.0 - ease(CHAIN_HOLD, moves.draw_end, progress));
            let carrying = (1.0 - follow) * (1.0 - ease(0.0, moves.draw_end, progress));
            let draw = if held < 1.0 { 1.0 - carrying / (1.0 - held) } else { 1.0 };
            (draw, strike.max(held), strike.powi(3))
        }
        (None, Some(since)) => {
            let striking = attack.swing.map_or(moves.strike_ticks, |swing| strike_ticks(&swing));
            let (strike, finish) = strike_at(striking + since, striking, moves.contact, moves.heavy);
            let (hold, settle) = moves.follow_through;
            let follow = 1.0 - ease(finish + hold, finish + settle, since);
            let smear = strike.powi(3) * (1.0 - ease(finish, finish + STRETCH_TICKS, since));
            (0.0, strike.min(follow), smear)
        }
        (None, None) => (0.0, 0.0, 0.0),
    }
}

fn load_rigs(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
) {
    let rigs = ClassId::all()
        .map(|c| {
            let rig = arena::fighter_rig(&c.def().id);
            let mut add = |mesh| meshes.add(mesh);
            let handles = RigHandles {
                head: add(rig.head),
                arm: add(rig.arm),
                leg: add(rig.leg),
                held: add(rig.held),
                eyes: add(rig.eyes),
                glow: materials.add(arena::glow(rig.glow, EYE_GLOW)),
                held_glow: rig.held_glow.map(|mesh| meshes.add(mesh)),
                body_glow: rig.body_glow.map(|mesh| meshes.add(mesh)),
                held_aura: rig.held_aura.map(|mesh| (meshes.add(mesh), materials.add(arena::translucent(rig.glow, AURA_ALPHA, EYE_GLOW)))),
                held_glint: rig.held_glint.map(|(mesh, at)| (meshes.add(mesh), materials.add(arena::glow(Color::WHITE, EYE_GLOW)), at)),
                wardrobe: rig.wardrobe.map(|wardrobe| WardrobeAssets::load(wardrobe, &mut meshes, &mut bindposes, &mut materials)),
            };
            (c, handles)
        })
        .collect();
    commands.insert_resource(RigAssets { meshes: rigs, eye_glow: materials.add(arena::glow(arena::palette::SOUL, EYE_GLOW)) });
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
            aura: handles.held_aura.as_ref().map(|(mesh, material)| part(mesh, material, Vec3::ZERO)),
            glint: handles.held_glint.as_ref().map(|(mesh, material, at)| part(mesh, material, *at)),
            gleam: handles.held_glint.as_ref().filter(|_| matches!(moves(&class.def().id).held, Held::Gripped { .. })).map(|(mesh, material, _)| part(mesh, material, Vec3::ZERO)),
            gleaming: Gleaming { seed: player.index().index(), ..default() },
            moves: moves(&class.def().id),
            stride: 0.0,
            walking: 0.0,
            cut: None,
            empowered: 0.0,
            joints: Joints::default(),
        };
        let eyes = part(&handles.eyes, &assets.eye_glow, Vec3::ZERO);
        let held_glow = handles.held_glow.as_ref().map(|mesh| part(mesh, &handles.glow, Vec3::ZERO));
        let body_glow = handles.body_glow.as_ref().map(|mesh| part(mesh, &handles.glow, Vec3::ZERO));
        commands.entity(rig.head).add_child(eyes);
        commands.entity(rig.arms[0]).add_child(rig.held);
        if let Some(glow) = held_glow {
            commands.entity(rig.held).add_child(glow);
        }
        for shown_armed in rig.aura.into_iter().chain(rig.glint).chain(rig.gleam) {
            commands.entity(shown_armed).insert((Visibility::Hidden, NotShadowCaster, NotShadowReceiver));
            commands.entity(rig.held).add_child(shown_armed);
        }
        if let Some(glow) = body_glow {
            commands.entity(player).add_child(glow);
        }
        commands.entity(player).add_children(&[rig.head, rig.arms[0], rig.arms[1], rig.legs[0], rig.legs[1]]);
        if let Some(wardrobe) = &handles.wardrobe {
            crate::cloth::dress(&mut commands, player, rig.legs, own, wardrobe);
        }
        if matches!(rig.moves.held, Held::Gripped { .. }) {
            commands.entity(player).insert(crate::swish::Blade { held: rig.held, length: arena::BLADE_LENGTH, shown: 0, quickened: false, striking: None, dragged: None, corner: arena::glaive_edge(1.0), rasp: 0 });
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
        let wants = match (attack.winding_up(), ability.dash) {
            (Some(windup), _) => windup.dir,
            // Struck mid-dash: still facing the strike.
            (None, Some(_)) if attack_in_dash(attack, ability) => facing.look,
            (None, Some(dash)) => dash.dir,
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

/// A dash with an attack in it (dashing mid-windup, or a windup going off mid-dash): it keeps
/// the attack's look, facing the aim, its pose undisturbed, gliding without a stride. Only a
/// dash on its own strikes the dash pose.
fn attack_in_dash(attack: &AttackState, ability: &AbilityState) -> bool {
    ability.dash.is_some_and(|dash| attack.winding_up().is_some() || attack.released_at.is_some_and(|at| at >= dash.started_at))
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

/// The strike (0..1) `ticks` after it started, as one committed motion: `contact` of the way
/// through after `strike_ticks`, when the attack goes off, and the rest after that. Snapping
/// out and slowing into place, or (`heavy`) gathering speed all the way into its end. Also how
/// many ticks after it went off the strike is done.
fn strike_at(ticks: f32, strike_ticks: f32, contact: f32, heavy: bool) -> (f32, f32) {
    // Where on the ease the contact falls.
    let at_contact = if heavy { contact.cbrt() } else { 1.0 - (1.0 - contact).cbrt() };
    let whole = strike_ticks / at_contact;
    let strike = if heavy { ease_in(0.0, whole, ticks) } else { ease_out(0.0, whole, ticks) };
    (strike, whole - strike_ticks)
}

/// 0 below `from`, 1 above `to`: slow at first, faster and faster. A heavy blow.
fn ease_in(from: f32, to: f32, x: f32) -> f32 {
    ((x - from) / (to - from)).clamp(0.0, 1.0).powi(3)
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
        Has<Me>,
    )>,
    mut parts: Query<(&mut Transform, &mut Visibility), Without<Rig>>,
) {
    let ease_walk = rate(&time, WALK_RATE);
    let dt = time.delta_secs().min(0.1);
    for (mut rig, facing, mut body, mut held_at, class, attack, ability, seen, is_me) in &mut rigs {
        let rig = &mut *rig;
        let gliding = attack_in_dash(attack, ability);
        if !gliding {
            rig.stride += facing.moved.length() * STRIDES_PER_METER;
        }
        let is_walking = if facing.moved.length() > 0.001 && !gliding { 1.0 } else { 0.0 };
        let walking = rig.walking + (is_walking - rig.walking) * ease_walk;
        rig.walking = if (walking - is_walking).abs() < 1e-3 { is_walking } else { walking };
        let stride = rig.stride.fract() * TAU;
        let step = stride.sin() * rig.walking;

        // Where in the strike we are: drawing (0..1), holding, then striking during the windup,
        // up to its contact pose as the attack goes off; then the rest of the strike, and the
        // follow-through easing back to carrying, unless walking cuts it short. A thrown Q
        // flicks through it.
        let def = class.def();
        let now = clock.as_ref().map_or(0.0, |clock| clock.now(is_me));
        let present = clock.as_ref().map_or(0.0, |clock| clock.now(true));
        // A throw whose spear we've seen counts from when it really happened, on our own clock.
        // One the state already shows a moment before our clock gets to it (news from the server
        // a frame early) has just happened: not yet to come, which would drop the pose back to
        // carrying for a frame.
        let since_throw = |tick: Option<u32>, seen: Option<u32>| match seen.filter(|&s| tick.is_none_or(|t| t <= s)) {
            Some(seen) => Some((present - seen as f32).max(0.0)),
            None => tick.map(|t| (now - t as f32).max(0.0)),
        };
        let windup = SeenThrows::windup(seen, attack);
        // The last attack's speed scales its backswing like its windup, and a hit that armed its
        // `follow_up` speeds it up (normal once it's done).
        let speed = attack.swing.map_or(1.0, |swing| swing.backswing_speed(*class));
        // Ticks since the last attack went off, at normal attack speed.
        let released = since_throw(attack.released_at, seen.and_then(|seen| seen.attack)).map(|since| since * speed);
        let moves = rig.moves;
        // Empowered (`follow_up` armed, or the empowered attack under way): the weapon grows, and
        // the empowered strike smears out further, to its longer lane.
        let empowering = attack.swing.is_some_and(|swing| swing.quick);
        let empowered = if empowering || attack.quick_at(now) { 1.0 } else { 0.0 };
        rig.empowered += (empowered - rig.empowered) * rate(&time, EMPOWER_RATE);
        let blade = 1.0 + (EMPOWERED_BLADE - 1.0) * rig.empowered;
        let stretch = moves.stretch + if empowering { EMPOWERED_STRETCH } else { 0.0 };
        let (posed_windup, posed_released, ahead) = ahead_of_springs(*class, moves, windup, released, now);
        let (draw, mut throw, mut smear) = strike_phase(*class, moves, attack, posed_windup, posed_released, ahead);
        // Walking off once the attack has gone off cuts its backswing short, for good: the
        // follow-through blends out quickly into the walk.
        let cut = match (windup, attack.released_at) {
            (None, Some(at)) if released.is_some() => {
                let cut = rig.cut.filter(|(of, _)| *of == at).map_or(0.0, |(_, cut)| cut);
                let cut = if cut > 0.0 || is_walking > 0.0 { cut + (1.0 - cut) * rate(&time, CUT_RATE) } else { 0.0 };
                rig.cut = Some((at, cut));
                cut
            }
            _ => {
                rig.cut = None;
                0.0
            }
        };
        throw *= 1.0 - cut;
        smear *= 1.0 - cut;
        let flicked = since_throw(ability.used_at(*class), seen.and_then(|seen| seen.ability));
        let mut dashing = if ability.dash.is_some() && !gliding { 1.0 } else { 0.0 };
        match (&def.ability.kind, flicked) {
            (AbilityKind::Projectile { .. }, Some(since)) => throw = throw.max(1.0 - ease(FLICK.0, FLICK.1, since)),
            // A nova's slam is its Q pose, like a dash's.
            (AbilityKind::Nova { .. }, Some(since)) => dashing = 1.0 - ease(SLAM.0, SLAM.1, since),
            _ => {}
        }
        let phase = Phase { draw, throw, dashing };
        // A thrown weapon: the hand is empty for a moment after a throw, until a new one is
        // drawn (whatever the arm does meanwhile).
        let empty_handed = moves.throws
            && matches!(def.attack.kind, AttackKind::Projectile { .. })
            && released.is_some_and(|s| s < REARM_TICKS);

        // Every angle chases its pose on a spring, and the rig is put together from them.
        let angles = rig.joints.follow(&targets(moves, phase, step), dt, moves);
        let aimed = targets(moves, phase, 0.0);
        let walk = rig.walking * (1.0 - phase.braced());
        let posed = assemble(moves, &angles, &aimed, facing.look, stride, walk, stretch * smear, blade);
        let placed = Transform { translation: body.translation.with_y(posed.body.translation.y), ..posed.body };
        body.set_if_neq(placed);
        // For a fighter that throws its weapon: where it is in the pose the joints are heading for
        // (child transforms aren't propagated until after this frame's shots are placed).
        if moves.throws {
            let at = Transform::from_translation(placed.translation.with_y(0.0)) * posed.held_aimed;
            held_at.set_if_neq(HeldAt(at));
        }
        let rotations = [(rig.head, posed.head), (rig.arms[0], posed.arms[0]), (rig.arms[1], posed.arms[1]), (rig.legs[0], posed.legs[0]), (rig.legs[1], posed.legs[1])];
        for (part, rotation) in rotations {
            if let Ok((mut transform, _)) = parts.get_mut(part)
                && transform.rotation != rotation
            {
                transform.rotation = rotation;
            }
        }
        if let Ok((mut transform, mut visibility)) = parts.get_mut(rig.held) {
            visibility.set_if_neq(shown(!empty_handed));
            transform.set_if_neq(posed.held);
        }
    }
}

/// Every couple of seconds a gleam catches somewhere along a gripped blade's edge, a star of
/// light swelling to a random brightness and going out again, each blade on its own time.
fn gleam_blades(time: Res<Time>, mut rigs: Query<&mut Rig>, mut parts: Query<(&mut Visibility, &mut Transform)>) {
    let now = time.elapsed_secs();
    for mut rig in &mut rigs {
        let Some(gleam) = rig.gleam else { continue };
        let g = &mut rig.gleaming;
        let mut random = || {
            g.seed = g.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (g.seed >> 8) as f32 / (1 << 24) as f32
        };
        if g.next == 0.0 {
            g.next = now + random() * GLEAM_EVERY.1;
        }
        if now >= g.next {
            let (along, peak, wait) = (random(), random(), random());
            g.from = now;
            g.at = arena::glaive_edge(0.1 + 0.85 * along);
            g.peak = GLEAM_SIZE.0 + (GLEAM_SIZE.1 - GLEAM_SIZE.0) * peak;
            g.next = now + GLEAM_EVERY.0 + (GLEAM_EVERY.1 - GLEAM_EVERY.0) * wait;
        }
        let t = (now - g.from) / GLEAM_SECONDS;
        let Ok((mut visibility, mut transform)) = parts.get_mut(gleam) else { continue };
        let on = (0.0..1.0).contains(&t);
        visibility.set_if_neq(shown(on));
        if on {
            // Growing steadily, lingering at its height with a squish (squashed flat and wide, and
            // springing back), shrinking steadily away; turning quickest at either end and settling
            // still at its height.
            let (rise, fall) = GLEAM_HOLD;
            let size = g.peak * (t / rise).min(1.0) * ((1.0 - t) / (1.0 - fall)).min(1.0);
            let held = ((t - rise) / (fall - rise)).clamp(0.0, 1.0);
            let squish = GLEAM_SQUISH * (std::f32::consts::PI * held).sin();
            let turn = GLEAM_SPIN * 4.0 * (t - 0.5).powi(3);
            let scale = Vec3::new(size * (1.0 + squish), size * (1.0 - squish), size * (1.0 + squish));
            *transform = Transform::from_translation(g.at).with_rotation(Quat::from_rotation_y(turn)).with_scale(scale);
        }
    }
}

/// A weapon's aura and glint show while the attack's `follow_up` is armed or quickening the
/// windup in progress: the aura swelling and shrinking a little, the glint spinning and flaring.
/// A blade's trail brightens with them (`swish::Blade::quickened`).
fn show_auras(
    time: Res<Time>,
    clock: Option<AttackClock>,
    mut fighters: Query<(&Rig, &AttackState, Has<Me>, Option<&mut crate::swish::Blade>)>,
    mut parts: Query<(&mut Visibility, &mut Transform)>,
) {
    let t = time.elapsed_secs();
    let pulse = 1.0 + AURA_PULSE.1 * (t * AURA_PULSE.0).sin();
    // Mostly small, flaring sharply once a twinkle.
    let flare = 0.4 + 1.0 * (0.5 + 0.5 * (t * GLINT_TWINKLE).sin()).powi(6);
    for (rig, attack, is_me, blade) in &mut fighters {
        // Hidden without a synced clock (e.g. after a disconnect): no telling when the window ends.
        let on = clock.as_ref().is_some_and(|clock| attack.quick_at(clock.now(is_me)) || attack.swing.is_some_and(|w| w.quick));
        if let Some(mut blade) = blade
            && blade.quickened != on
        {
            blade.quickened = on;
        }
        if let Some((mut visibility, mut transform)) = rig.aura.and_then(|aura| parts.get_mut(aura).ok()) {
            visibility.set_if_neq(shown(on));
            if on {
                transform.scale = Vec3::new(pulse, 1.0, pulse);
            }
        }
        if let Some((mut visibility, mut transform)) = rig.glint.and_then(|glint| parts.get_mut(glint).ok()) {
            visibility.set_if_neq(shown(on));
            if on {
                transform.scale = Vec3::splat(flare);
                transform.rotation = Quat::from_rotation_y(t * GLINT_SPIN);
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
