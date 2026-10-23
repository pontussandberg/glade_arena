//! Fighters with moving parts (see `arena::fighter_rig`), animated smoothly but deliberately:
//! eased keyframes in distinct beats (draw, hold, a fast committed strike), a walk with a light
//! bob and sway, and every joint on a firm, nearly critically damped spring so motion is smoothed
//! without wobbling (cloth that moves on its own is `cloth/`).
//! Every fighter turns to face where it walks, swings or dashes (`Facing`) and acts out its
//! attack; each class has its own `Moves`.
//!
//! An attack is timed to the server's: the draw and the strike fill its windup, and the
//! strike is `Moves::contact` of the way through as it goes off (the spear leaving the hand, the
//! blade's tip just reaching the target). The rest of the strike and the follow-through are the
//! backswing, which walking cuts short (`CUT_RATE`). Nothing on screen says when the attack has
//! gone off but the fighter itself. A quicker attack (`follow_up`) plays all of it faster.
//!
//! The javelinist always carries the javelin cocked over the shoulder, ready to throw. The
//! windup: twist the throwing shoulder far back, lean well back and sink into a wide stance, lead arm
//! pointing at the target, while drawing the javelin further back; then lunge forward through
//! the throw, the arm whipping over the top, hold the follow-through for a moment and settle
//! back. Head and javelin stay on the target throughout. The hand is empty until a new javelin
//! is drawn, as the next attack is ready. A thrown Q is a quick flick of the arm through the
//! same throw.
//!
//! The revenant drags its great blade behind it, the point trailing just above the ground. Its
//! attack is a heavy overhead blow: the windup heaves the blade up from behind and over its
//! head, leaning back with the lead hand thrown out at the target; the strike brings it down
//! over the top in one great arc as the lead leg steps long into it and the body crunches in
//! behind it, the blade smearing out longer, and on into the ground, where it leaves a crack
//! (`swish/`). It lands as the blade comes level, out in front. A hit empowers the blade
//! (Follow Through): it grows half as big again while that's armed and through the empowered
//! attack (`EMPOWERED_BLADE`), which is the same blow, faster, smearing out further still to
//! reach its longer lane (`EMPOWERED_STRETCH`). A strike started before the last one has settled chains onto it: the blade comes
//! straight back into the draw instead of sinking to the carry in between. A dash is no move of
//! its own: the fighter glides along in its stance, turned to the dash, cutting a backswing
//! short; taken mid-windup, it doesn't turn at all, facing as it was until its next move.
//!
//! The frost mage carries its staff upright at its side. The windup raises it high toward the
//! target, the off hand reaching out to gather the cold; the cast thrusts the crystal forward at
//! the target. A nova slams the staff down into the ground in a deep crouch, the off hand flung
//! back.
//!
//! Transforms are only written when they change, so a fighter standing still costs nothing.

use std::f32::consts::TAU;

use arena_shared::abilities::AbilityKind;
use arena_shared::classes::AttackKind;
use arena_shared::config::TICK_HZ;
use arena_shared::protocol::*;
use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use crate::Me;
use crate::cloth::WardrobeAssets;
use crate::feedback::AttackClock;
use crate::arena::{self, RIG_HIP, RIG_KNEE, RIG_NECK, RIG_SHOULDER};
use crate::ease::*;

mod joints;
pub(crate) use joints::*;
mod moves;
pub(crate) use moves::*;
mod pose;
pub(crate) use pose::*;
mod throw;
pub(crate) use throw::*;
mod shine;
use shine::*;
mod drips;
use drips::*;
pub(crate) use drips::ShotDrips;

pub struct RigPlugin;

impl Plugin for RigPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((crate::cloth::ClothPlugin, crate::swish::SwishPlugin));
        app.add_systems(Startup, (load_rigs, load_drips));
        app.add_systems(PostUpdate, (shed_drips, drip_from_shots).after(TransformSystems::Propagate));
        app.add_systems(Update, fall_drips);
        app.add_systems(Update, (add_rigs, see_throws, turn_fighters, pose_rigs, show_auras, gleam_blades).chain().in_set(Posing));
    }
}

/// Where fighters get turned and posed; what depends on a fighter's transform runs after it.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct Posing;

/// How many feet a fighter has put down (one every half stride, as it walks): the footsteps
/// sound off it (`audio::footstep_sounds`).
#[derive(Component, Default, PartialEq, Eq)]
pub(crate) struct Footfalls(pub(crate) u32);

/// A fighter flinching from a blow (`feedback` sets it on each confirmed hit): squashed, then
/// springing back over `SQUASH_SECONDS`.
#[derive(Component)]
pub struct HitSquash {
    pub at: f32,
}

const SQUASH_SECONDS: f32 = 0.18;

/// Walk cycles per meter walked (the swing and bob: `pose`).
const STRIDES_PER_METER: f32 = 0.4;
/// How much bigger an elite monster is drawn than its kind.
const ELITE_SIZE: f32 = 1.25;

/// How big a fighter of `class` is drawn, as a share of its figure's size (an `elite` bigger).
pub(crate) fn drawn_size(class: ClassId, elite: bool) -> f32 {
    class.def().size() * if elite { ELITE_SIZE } else { 1.0 }
}
/// An empowered weapon (`follow_up`): how much bigger it grows, how quickly (per second,
/// exponential), and how much further its strike smears out, so it reaches its lane's far end
/// (`FollowUp::reach`).
pub(crate) const EMPOWERED_BLADE: f32 = 1.5;
const EMPOWER_RATE: f32 = 12.0;
pub(crate) const EMPOWERED_STRETCH: f32 = 0.05;
/// How quickly walking off cuts the backswing short (per second, exponential): the
/// follow-through blends out into the walk.
const CUT_RATE: f32 = 30.0;
/// A thrown Q's flick: ticks after the throw until the arm starts and finishes coming back.
const FLICK: (f32, f32) = (0.5, 5.0);
/// How far through its throw (0 to 1) a thrower's arm flicks for a Q: through the release, short
/// of the spent follow-through its throw ends in.
const FLICK_REACH: f32 = 0.55;
/// A nova's slam: ticks after the cast until it starts and finishes rising out of it.
const SLAM: (f32, f32) = (4.0, 12.0);
/// How quickly a fighter turns toward where it wants to face, and eases in and out of its walk
/// (per second, exponential).
const TURN_RATE: f32 = 40.0;
const WALK_RATE: f32 = 10.0;
/// How brightly the eyes (and other glowing parts) glow.
const EYE_GLOW: f32 = 6.0;
/// Standing still, a fighter breathes: how fast (radians per second), how far it rocks (radians)
/// and rises (meters, at size 1) with each breath.
const BREATH_RATE: f32 = 1.5;
const BREATH_LEAN: f32 = 0.014;
const BREATH_RISE: f32 = 0.005;
/// Dashing, a fighter throws itself into a lunge: how quickly it goes into it and comes out (per
/// second, exponential), how far it leans into it (radians), how far its legs split, and how far
/// its arms trail back.
const GLIDE_RATE: f32 = 14.0;
const DASH_LEAN: f32 = 0.35;
const DASH_STRIDE: f32 = 0.45;
const DASH_ARMS: f32 = 0.5;
/// Knocked off its feet, a fighter staggers: how far back it's thrown (radians) and how far its
/// arms fly up, at the top of the knock-up.
const STAGGER_LEAN: f32 = 0.5;
const STAGGER_ARMS: f32 = 0.7;

/// How brightly a weapon's aura glows (its cracks of light).
const AURA_GLOW: f32 = 9.0;


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
    forearm: Option<Handle<Mesh>>,
    lead_arm: Option<Handle<Mesh>>,
    leg: Handle<Mesh>,
    shin: Option<Handle<Mesh>>,
    held: Handle<Mesh>,
    eyes: Handle<Mesh>,
    glow: Handle<StandardMaterial>,
    held_glow: Option<Handle<Mesh>>,
    body_glow: Option<Handle<Mesh>>,
    held_aura: Option<(Handle<Mesh>, Handle<StandardMaterial>)>,
    held_glint: Option<(Handle<Mesh>, Handle<StandardMaterial>, arena::Edge)>,
    wardrobe: Option<WardrobeAssets>,
}

/// Which way a fighter faces, where it was last frame, and how far it moved and turned
/// (radians) since; the last dash it saw start (`Dash::started_at`), and whether it's holding
/// its look through it (`turn_fighters`): `INFINITY` while dashing, then since when it ended.
#[derive(Component)]
struct Facing {
    look: Vec2,
    last_pos: Vec2,
    moved: Vec2,
    turned: f32,
    dash_seen: Option<u32>,
    steady: Option<f32>,
}

/// A fighter's moving parts (child entities), its moves and walk state.
#[derive(Component)]
pub(crate) struct Rig {
    /// How big it's drawn, as a share of its figure's size (`ClassDef::size`, an elite bigger).
    size: f32,
    head: Entity,
    /// Right (weapon) arm and left (lead) arm, and their elbows (the held weapon hangs from the
    /// weapon arm's).
    arms: [Entity; 2],
    elbows: [Entity; 2],
    /// Right (back) leg and left (lead) leg, and their knees.
    legs: [Entity; 2],
    knees: [Entity; 2],
    held: Entity,
    /// The held weapon's aura (`RigMeshes::held_aura`), if it has one.
    aura: Option<Entity>,
    /// A gleam catching along the held blade's edge now and then (`gleam_blades`), where along it
    /// (`RigMeshes::held_glint`), and when.
    gleam: Option<(Entity, arena::Edge)>,
    gleaming: Gleaming,
    moves: &'static Moves,
    /// Walk cycles so far, and how much the fighter is walking (0..1, eased).
    stride: f32,
    walking: f32,
    /// The backswing cut short by walking off: the attack's `released_at`, and how far it's
    /// blended out (0..1).
    cut: Option<(u32, f32)>,
    /// The last attack's backswing speed, and when it went off: kept once the server clears its
    /// swing, so its follow-through and the hand's rearming don't jump back to normal speed.
    backswing: Option<(Option<u32>, f32)>,
    /// While its next attack is primed: when (seconds) it drips its next drop of light, and how
    /// many it has dripped (`shed_drips`).
    drips: (f32, u32),
    /// How far its weapon is grown, empowered by its attack's `follow_up` (0..1, eased).
    empowered: f32,
    /// How far into its dashing lunge it is (0..1, eased).
    glide: f32,
    joints: Joints,
}

fn load_rigs(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
) {
    let rigs = ClassId::all()
        .map(|c| {
            let rig = arena::fighter_rig(c.def().figure());
            // Smooth-shaded like the body (`render::setup_scene`): rounded forms, not chiseled
            // facets.
            let mut add = |mesh| meshes.add(mesh);
            let handles = RigHandles {
                head: add(rig.head),
                arm: add(rig.arm),
                forearm: rig.forearm.map(&mut add),
                lead_arm: rig.lead_arm.map(&mut add),
                leg: add(rig.leg),
                shin: rig.shin.map(&mut add),
                held: add(rig.held),
                eyes: meshes.add(rig.eyes),
                glow: materials.add(arena::glow(rig.glow, EYE_GLOW)),
                held_glow: rig.held_glow.map(|mesh| meshes.add(mesh)),
                body_glow: rig.body_glow.map(|mesh| meshes.add(mesh)),
                held_aura: rig.held_aura.map(|mesh| (meshes.add(mesh), materials.add(arena::glow(rig.glow, AURA_GLOW)))),
                held_glint: rig.held_glint.map(|(mesh, edge)| (meshes.add(mesh), materials.add(arena::glow(Color::WHITE, EYE_GLOW)), edge)),
                // A borrowed figure may go without its cloth (monsters, many at once).
                wardrobe: rig.wardrobe.filter(|_| c.def().cloth()).map(|wardrobe| WardrobeAssets::load(wardrobe, &mut meshes, &mut bindposes, &mut materials)),
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
    new: Query<(Entity, &ClassId, &Pos, &MeshMaterial3d<StandardMaterial>, Option<&Elite>, Has<Shopkeeper>), Added<Mesh3d>>,
    map: Res<crate::rooms::CurrentMap>,
) {
    for (player, class, pos, body, elite, shopkeeper) in &new {
        let Some(handles) = assets.meshes.get(class) else { continue };
        let class_moves = moves(class.def().figure());
        let size = drawn_size(*class, elite.is_some());
        let elbows = [(); 2].map(|_| commands.spawn((Transform::from_translation(class_moves.elbow_at), Visibility::default())).id());
        let knees = [(); 2].map(|_| commands.spawn((Transform::from_translation(RIG_KNEE), Visibility::default())).id());
        let mut part = |mesh: &Handle<Mesh>, material: &Handle<StandardMaterial>, at: Vec3| {
            let transform = Transform::from_translation(at);
            commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), transform)).id()
        };
        let own = &body.0;
        let rig = Rig {
            head: part(&handles.head, own, RIG_NECK),
            arms: [part(&handles.arm, own, RIG_SHOULDER), part(handles.lead_arm.as_ref().unwrap_or(&handles.arm), own, mirrored(RIG_SHOULDER))],
            elbows,
            legs: [part(&handles.leg, own, RIG_HIP), part(&handles.leg, own, mirrored(RIG_HIP))],
            knees,
            held: part(&handles.held, own, class_moves.hand_at - class_moves.elbow_at),
            aura: handles.held_aura.as_ref().map(|(mesh, material)| part(mesh, material, Vec3::ZERO)),
            gleam: handles.held_glint.as_ref().map(|(mesh, material, edge)| (part(mesh, material, Vec3::ZERO), *edge)),
            gleaming: Gleaming { seed: player.index().index(), ..default() },
            moves: class_moves,
            size,
            stride: 0.0,
            walking: 0.0,
            cut: None,
            backswing: None,
            drips: (0.0, 0),
            empowered: 0.0,
            glide: 0.0,
            joints: Joints::default(),
        };
        let eyes = part(&handles.eyes, &assets.eye_glow, Vec3::ZERO);
        let held_glow = handles.held_glow.as_ref().map(|mesh| part(mesh, &handles.glow, Vec3::ZERO));
        let body_glow = handles.body_glow.as_ref().map(|mesh| part(mesh, &handles.glow, Vec3::ZERO));
        // A forearm on each arm, or only the weapon arm if the other is an empty sleeve.
        let arms_with_forearms = if handles.lead_arm.is_some() { 1 } else { 2 };
        let shins: Vec<Entity> = handles.shin.iter().flat_map(|mesh| [mesh, mesh]).map(|mesh| part(mesh, own, Vec3::ZERO)).collect();
        let forearms: Vec<Entity> = handles.forearm.iter().flat_map(|mesh| std::iter::repeat_n(mesh, arms_with_forearms)).map(|mesh| part(mesh, own, Vec3::ZERO)).collect();
        commands.entity(rig.head).add_child(eyes);
        for (arm, elbow) in rig.arms.into_iter().zip(rig.elbows) {
            commands.entity(arm).add_child(elbow);
        }
        for (elbow, forearm) in rig.elbows.into_iter().zip(forearms) {
            commands.entity(elbow).add_child(forearm);
        }
        for (leg, knee) in rig.legs.into_iter().zip(rig.knees) {
            commands.entity(leg).add_child(knee);
        }
        for (knee, shin) in rig.knees.into_iter().zip(shins) {
            commands.entity(knee).add_child(shin);
        }
        commands.entity(rig.elbows[0]).add_child(rig.held);
        if let Some(glow) = held_glow {
            commands.entity(rig.held).add_child(glow);
        }
        for shown_armed in rig.aura.into_iter().chain(rig.gleam.map(|(gleam, _)| gleam)) {
            commands.entity(shown_armed).insert((Visibility::Hidden, NotShadowCaster, NotShadowReceiver));
            commands.entity(rig.held).add_child(shown_armed);
        }
        if let Some(glow) = body_glow {
            commands.entity(player).add_child(glow);
        }
        commands.entity(player).add_children(&[rig.head, rig.arms[0], rig.arms[1], rig.legs[0], rig.legs[1]]);
        if let Some(wardrobe) = &handles.wardrobe {
            crate::cloth::dress(&mut commands, player, (rig.legs, rig.knees, rig.arms[0]), own, wardrobe);
        }
        if matches!(rig.moves.held, Held::Gripped { .. }) {
            let figure = class.def().figure();
            let (from, length) = arena::blade_span(figure);
            // A glaive's broad edge reaches the ground at its corner before its point; a plain
            // blade's lowest point is its point.
            let corner = if figure == "revenant" { arena::glaive_edge(1.0) } else { Vec3::Y * length };
            commands.entity(player).insert(crate::swish::Blade { held: rig.held, length, from: from / length, shown: 0, quickened: false, striking: false, dragged: None, corner, rasp: 0, embers: (0.0, 0) });
        }
        commands.entity(player).insert((
            rig,
            // The merchant looks out from their stall at whoever comes to trade.
            Facing { look: if shopkeeper { map.map().shop_faces } else { Vec2::X }, last_pos: pos.0, moved: Vec2::ZERO, turned: 0.0, dash_seen: None, steady: None },
            HeldAt::default(),
            SeenThrows::default(),
            Footfalls::default(),
        ));
    }
}

/// `1 - e^(-per_second * dt)`: how far to ease toward a target this frame.
fn rate(time: &Time, per_second: f32) -> f32 {
    1.0 - (-per_second * time.delta_secs()).exp()
}

/// Turns every fighter toward where it wants to face: its locked aim while winding up, its dash,
/// or the way it walks (if it moved this frame). Snaps once there, so it settles.
///
/// A dash taken mid-windup doesn't turn it at all: it holds its look through the dash and after,
/// until its next attack, or until it walks off once the dash is over (`STEADY_SETTLE` after, so
/// the dash's own last steps don't count). A dash in the backswing cuts it short and turns it
/// to the dash, like walking off.
fn turn_fighters(time: Res<Time>, mut fighters: Query<(&mut Facing, &Pos, &AttackState, &Dashing)>) {
    let turn = rate(&time, TURN_RATE);
    let now = time.elapsed_secs();
    for (mut facing, pos, attack, dashing) in &mut fighters {
        let moved = pos.0 - facing.last_pos;
        let mut steady = facing.steady;
        let mut dash_seen = facing.dash_seen;
        match dashing.0 {
            Some(dash) if dash_seen != Some(dash.started_at) => {
                dash_seen = Some(dash.started_at);
                steady = attack.winding_up().is_some().then_some(f32::INFINITY);
            }
            Some(_) => {}
            None if steady == Some(f32::INFINITY) => steady = Some(now),
            None => {}
        }
        // A new attack, or walking off after the dash, lets it turn again.
        let attacked_since = attack.swing.is_some_and(|swing| dash_seen.is_some_and(|at| swing.started_at > at));
        let walked_off = steady.is_some_and(|ended| now - ended > STEADY_SETTLE) && moved.length() > 0.01;
        if attacked_since || walked_off {
            steady = None;
        }
        let wants = match (attack.winding_up(), dashing.0) {
            (Some(windup), _) => windup.dir,
            _ if steady.is_some() => facing.look,
            (None, Some(dash)) => dash.dir,
            (None, None) if moved.length() > 0.01 => moved.normalize(),
            (None, None) => facing.look,
        };
        let look = facing.look.lerp(wants, turn).try_normalize().unwrap_or(wants);
        let look = if look.dot(wants) > 0.9999 { wants } else { look };
        let turned = facing.look.angle_to(look);
        if facing.look != look || facing.last_pos != pos.0 || facing.moved != moved || facing.turned != turned || facing.steady != steady || facing.dash_seen != dash_seen {
            *facing = Facing { look, last_pos: pos.0, moved, turned, dash_seen, steady };
        }
    }
}

/// How long (seconds) after a dash ends before walking turns a fighter that held its look
/// through it (`turn_fighters`).
const STEADY_SETTLE: f32 = 0.15;

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
        (&Cooldowns, &Dashing),
        Option<&SeenThrows>,
        Has<Me>,
        Option<&HitSquash>,
        &mut Footfalls,
        Option<&Chilled>,
    )>,
    mut parts: Query<(&mut Transform, &mut Visibility), Without<Rig>>,
) {
    let ease_walk = rate(&time, WALK_RATE);
    let dt = time.delta_secs().min(0.1);
    let wall_clock = time.elapsed_secs();
    for (mut rig, facing, mut body, mut held_at, class, attack, (cooldowns, dashing), seen, is_me, squash, mut footfalls, chilled) in &mut rigs {
        let rig = &mut *rig;
        // Dashing, it glides along in its pose without a stride, thrown into a lunge.
        let gliding = dashing.0.is_some();
        rig.glide += (if gliding { 1.0 } else { 0.0 } - rig.glide) * rate(&time, GLIDE_RATE);
        // A jump (a spawn, a waygate, a correction) is no stride at all.
        if !gliding && facing.moved.length() < arena_shared::config::JUMP {
            let before = rig.stride;
            rig.stride += facing.moved.length() * STRIDES_PER_METER;
            // A foot comes down every half stride (the legs pass each other there).
            if (before * 2.0).floor() != (rig.stride * 2.0).floor() {
                footfalls.0 += 1;
            }
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
        // Ours: news of the attack going off arrives a moment (`ME_BEHIND_TICKS`) before our clock
        // gets there; until it does, it's still winding up, so the strike plays out in full.
        let windup = SeenThrows::windup(seen, attack)
            .or_else(|| attack.swing.filter(|swing| is_me && attack.released_at.is_some_and(|at| at >= swing.started_at && now < at as f32)));
        // The last attack's speed scales its backswing like its windup, and a hit that armed its
        // `follow_up` speeds it up (normal once it's done).
        let speed = match attack.swing {
            Some(swing) => swing.backswing_speed(&class.def().attack),
            None => rig.backswing.filter(|(at, _)| *at == attack.released_at).map_or(1.0, |(_, speed)| speed),
        };
        if attack.swing.is_some() {
            rig.backswing = Some((attack.released_at, speed));
        }
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
        let (mut draw, mut throw, mut smear, mut second) = strike_phase(*class, moves, attack, posed_windup, posed_released, ahead);
        // Walking off (or dashing off, after it went off) once the attack has gone off cuts its
        // backswing short, for good: the follow-through blends out quickly into the walk.
        let cut = match (windup, attack.released_at) {
            (None, Some(at)) if released.is_some() => {
                let cut = rig.cut.filter(|(of, _)| *of == at).map_or(0.0, |(_, cut)| cut);
                let dashed_off = dashing.0.is_some_and(|dash| dash.started_at > at);
                let cut = if cut > 0.0 || is_walking > 0.0 || dashed_off { cut + (1.0 - cut) * rate(&time, CUT_RATE) } else { 0.0 };
                rig.cut = Some((at, cut));
                cut
            }
            _ => {
                rig.cut = None;
                0.0
            }
        };
        draw *= 1.0 - cut;
        throw *= 1.0 - cut;
        smear *= 1.0 - cut;
        second *= 1.0 - cut;
        // A dash has no pose of its own; a nova's slam is the ability pose. A thrower's flick
        // goes only so far through the throw (`FLICK_REACH`), short of its spent follow-through.
        let mut dashing = 0.0;
        let mut flick = 0.0f32;
        let reach = if moves.throws { FLICK_REACH } else { 1.0 };
        for (slot, ability) in def.abilities() {
            let thrown = seen.and_then(|seen| seen.abilities.get(slot.0 as usize).copied().flatten());
            match (&ability.kind, since_throw(cooldowns.used_at(slot), thrown)) {
                // A strike is called down with the same flick of the arm as a throw.
                (AbilityKind::Projectile(_) | AbilityKind::Strike(_), Some(since)) => flick = flick.max(reach * (1.0 - ease(FLICK.0, FLICK.1, since))),
                (AbilityKind::Nova(_), Some(since)) => dashing = f32::max(dashing, 1.0 - ease(SLAM.0, SLAM.1, since)),
                _ => {}
            }
        }
        let throw = throw.max(flick);
        let phase = Phase { draw, throw, dashing, second };
        // A thrown weapon: the hand is empty from a throw until the next attack is ready (its
        // backswing at normal speed, as `released` counts), when a new one is drawn.
        let rearm = (def.attack.period_ticks(1.0) - def.attack.windup_ticks(1.0)) as f32;
        // The latest throw the pose has let go of: ours as our clock gets to it (the state says
        // so a moment early), another's as soon as its spear is seen (it's released then).
        let seen_throw = seen.and_then(|seen| seen.attack).filter(|_| !is_me);
        let let_go = attack.released_at.filter(|&at| now >= at as f32).max(seen_throw);
        // Empty once the flying spear has left the hand (or it's late: `empty_hands`).
        let thrown = attack.released_at.max(seen_throw);
        let rearming = thrown.filter(|_| moves.throws && matches!(def.attack.kind, AttackKind::Projectile { .. }) && released.is_some_and(|s| s < rearm));
        let late = thrown.is_some_and(|at| present > at as f32 + MAX_HOLD_TICKS);
        // Our own hand letting go this frame: just how it was posed as it did, between frames.
        let frame_ticks = dt * TICK_HZ as f32;
        let released_from = match let_go {
            Some(at) if is_me && moves.throws && held_at.let_go != let_go && now - (at as f32) < frame_ticks => {
                let back = (dt - (now - at as f32) / TICK_HZ as f32).clamp(0.0, dt);
                let posed = pose_releasing(*class, moves, attack, &rig.joints, back, at as f32, flick, (facing.look, step, rig.walking, stride));
                let placed = Transform { translation: body.translation.with_y(posed.body.translation.y), ..posed.body };
                Some(held_in_world(&placed, &posed))
            }
            _ if held_at.let_go == let_go => held_at.released_from,
            _ => None,
        };

        // Every angle chases its pose on a spring, and the rig is put together from them. On
        // top of its moves: standing still it breathes (rocking and rising a little, each
        // fighter on its own beat); dashing it lunges, leaning into it with its arms trailing
        // and its legs split; knocked off its feet it staggers back, arms flung up.
        let mut target = targets(moves, phase, step, rig.walking);
        let free = 1.0 - phase.braced();
        let breathing = (1.0 - rig.walking) * free * (1.0 - rig.glide);
        let breath = (wall_clock * BREATH_RATE + (rig.gleaming.seed as f32 * 0.37).fract() * TAU).sin() * breathing;
        let lunge = rig.glide * free;
        let stagger = chilled.map_or(0.0, |chilled| {
            let span = chilled.lifted;
            if span.covers(now) { ((now - span.from as f32) / (span.until - span.from).max(1) as f32 * std::f32::consts::PI).sin() } else { 0.0 }
        });
        target.lean += BREATH_LEAN * breath - DASH_LEAN * lunge + STAGGER_LEAN * stagger;
        target.head_dip += 0.4 * BREATH_LEAN * breath - 0.12 * lunge - 0.3 * stagger;
        target.lead_leg += DASH_STRIDE * lunge;
        target.back_leg -= DASH_STRIDE * lunge;
        target.lead_arm += STAGGER_ARMS * stagger - DASH_ARMS * lunge;
        target.weapon_arm += 0.6 * STAGGER_ARMS * stagger - DASH_ARMS * lunge * if moves.rests { 0.3 } else { 1.0 };
        let angles = rig.joints.follow(&target, dt, moves);
        let aimed = targets(moves, phase, 0.0, rig.walking);
        let walk = rig.walking * free;
        let posed = assemble(moves, &angles, &aimed, facing.look, stride, walk, stretch * smear, blade, free);
        // Just hit, the whole figure flinches: squashed wide and short, springing back over
        // `SQUASH_SECONDS`, so a blow visibly lands in the body and not just on the health bar.
        let flinch = squash.map_or(0.0, |s| (1.0 - (wall_clock - s.at) / SQUASH_SECONDS).clamp(0.0, 1.0).powi(2));
        let squashed = Vec3::new(1.0 + 0.12 * flinch, 1.0 - 0.16 * flinch, 1.0 + 0.12 * flinch);
        let placed = Transform {
            translation: body.translation.with_y((posed.body.translation.y + BREATH_RISE * (0.5 + 0.5 * breath) * breathing) * rig.size),
            scale: posed.body.scale * rig.size * squashed,
            ..posed.body
        };
        body.set_if_neq(placed);
        // For a fighter that throws its weapon: where it is in the hand this frame (child
        // transforms aren't propagated until after this frame's shots are placed).
        // Let go of, but its flying spear not here yet (the news of it late): it flies on by
        // itself from just where it was let go, along the aim at its speed, not dragged on in
        // the hand, until the flying spear takes over just there (`render::fly_shots`), or for
        // good until it's late (it may never come: a shot that hit or ended at once), when it's
        // back in the hand, hidden till the next attack's ready (`empty_hands`).
        let mut held = posed.held;
        if moves.throws {
            let ahead_of_news = released_from.filter(|_| held_at.launched < let_go && !late);
            let at = match (ahead_of_news, let_go, def.attack.kind.shot()) {
                (Some(from), Some(at), Some(shot)) => {
                    let flown = (now - at as f32).max(0.0) / TICK_HZ as f32 * shot.speed;
                    let flying = Transform { translation: from.translation + arena::to_world(facing.look, 0.0) * flown, ..from };
                    // What it hangs from: the fighter, its weapon arm and elbow (see `add_rigs`).
                    let elbow = Transform::from_translation(placed.translation.with_y(0.0))
                        * posed.body
                        * Transform::from_translation(RIG_SHOULDER).with_rotation(posed.arms[0])
                        * Transform::from_translation(moves.elbow_at).with_rotation(posed.elbows[0]);
                    held = Transform::from_matrix(Mat4::from(elbow.compute_affine().inverse() * flying.compute_affine()));
                    flying
                }
                _ => held_in_world(&placed, &posed),
            };
            let launched = held_at.launched;
            held_at.set_if_neq(HeldAt { at, let_go, launched, released_from, rearming, late });
        }
        let rotations = [
            (rig.head, posed.head),
            (rig.arms[0], posed.arms[0]),
            (rig.arms[1], posed.arms[1]),
            (rig.elbows[0], posed.elbows[0]),
            (rig.elbows[1], posed.elbows[1]),
            (rig.legs[0], posed.legs[0]),
            (rig.legs[1], posed.legs[1]),
            (rig.knees[0], posed.knees[0]),
            (rig.knees[1], posed.knees[1]),
        ];
        for (part, rotation) in rotations {
            if let Ok((mut transform, _)) = parts.get_mut(part)
                && transform.rotation != rotation
            {
                transform.rotation = rotation;
            }
        }
        if let Ok((mut transform, _)) = parts.get_mut(rig.held) {
            transform.set_if_neq(held);
        }
    }
}

#[cfg(test)]
mod tests {
    use arena_shared::protocol::ClassId;

    #[test]
    fn every_class_has_moves() {
        for class in ClassId::all() {
            super::moves(class.def().figure());
        }
    }

    fn positions(mesh: &bevy::prelude::Mesh) -> Vec<bevy::prelude::Vec3> {
        use bevy::mesh::VertexAttributeValues;
        match mesh.attribute(bevy::prelude::Mesh::ATTRIBUTE_POSITION) {
            Some(VertexAttributeValues::Float32x3(p)) => p.iter().map(|&p| p.into()).collect(),
            _ => panic!("a mesh without positions"),
        }
    }

    /// Plays the javelinist's throw frame by frame as `pose_rigs` poses it, at several frame
    /// rates and with the release falling at different points between frames. Just as the hand
    /// lets go (`pose_releasing`), the flying spear (`render::leaving_hand`) is drawn exactly over
    /// the held one as its parts draw it, every vertex, its point over the thrower's own hitbox
    /// (where the spear's real path starts) at the height it flies at (`render::THROWN_HEIGHT`),
    /// passing over the shoulder (the arm under it), however the frames fall.
    #[test]
    fn a_thrown_spear_leaves_the_hand_just_where_it_is_held() {
        use arena_shared::protocol::{AttackState, Swing};
        use bevy::prelude::*;

        use super::*;
        use crate::render::{THROWN_HEIGHT, leaving_hand};

        let key = "javelinist";
        let class = ClassId::by_key(key).unwrap();
        let def = class.def();
        let moves = moves(key);
        let rig = arena::fighter_rig(key);
        let shot = arena::shot_look(def, def.shot(None).unwrap(), false).mesh;
        let (held_mesh, shot_mesh) = (positions(&rig.held), positions(&shot));
        assert_eq!(held_mesh.len(), shot_mesh.len(), "the flying spear is the held one");
        let mut heights = Vec::new();
        for fps in [30.0, 60.0, 144.0] {
            for phase in 0..8 {
                let swing = Swing { started_at: 10, dir: Vec2::X, quick: false, hastened: false, haste: 0.0, primed: false };
                let release = 10 + swing.windup_ticks(&def.attack);
                let mut joints = Joints::default();
                let dt = 1.0 / fps;
                let mut frame = 0;
                let leave = loop {
                    // Our own fighter's clock (`pose_rigs`), starting at a different point
                    // between frames each time.
                    let now = (frame as f32 + phase as f32 / 8.0) * dt * TICK_HZ as f32;
                    frame += 1;
                    let mut attack = AttackState { swing: Some(swing), ..default() };
                    if now >= release as f32 {
                        attack.swing = None;
                        attack.released_at = Some(release);
                        // The frame the hand lets go: posed just as it did.
                        let back = dt - (now - release as f32) / TICK_HZ as f32;
                        break pose_releasing(class, moves, &attack, &joints, back, release as f32, 0.0, (Vec2::X, 0.0, 0.0, 0.0));
                    }
                    let (shown, shown_released, ahead) = ahead_of_springs(class, moves, attack.winding_up(), None, now);
                    let (draw, throw, _, second) = strike_phase(class, moves, &attack, shown, shown_released, ahead);
                    joints.follow(&targets(moves, Phase { draw, throw, dashing: 0.0, second }, 0.0, 0.0), dt, moves);
                };
                let placed = Transform { translation: Vec3::new(3.0, leave.body.translation.y, -2.0), ..leave.body };
                // The held spear as its parts (fighter, arm, elbow, spear) draw it.
                let drawn = GlobalTransform::from(placed)
                    .mul_transform(Transform::from_translation(arena::RIG_SHOULDER).with_rotation(leave.arms[0]))
                    .mul_transform(Transform::from_translation(moves.elbow_at).with_rotation(leave.elbows[0]))
                    .mul_transform(leave.held);
                let flying = leaving_hand(&held_in_world(&placed, &leave));
                let off = held_mesh.iter().zip(&shot_mesh).map(|(&h, &s)| drawn.transform_point(h).distance(flying.transform_point(s))).fold(0.0, f32::max);
                assert!(off < 1e-3, "at {fps} fps, phase {phase}: the flying spear is drawn up to {off} m off the held one");
                heights.push((fps, phase, flying.translation.y));
                // Over the shoulder, the arm under it, not through it.
                let shoulder = GlobalTransform::from(placed).transform_point(arena::RIG_SHOULDER);
                let (grip, along) = (drawn.translation(), drawn.rotation() * Vec3::Y);
                let over = (grip + along * (shoulder - grip).dot(along) - shoulder).y;
                assert!(over > SHOULDER_CLEARANCE, "at {fps} fps, phase {phase}: the spear passes only {over} m over the shoulder");
                // Over its own hitbox, where its real path starts.
                let off_center = (flying.translation - placed.translation).with_y(0.0).length();
                assert!(off_center < arena_shared::config::PLAYER_RADIUS, "at {fps} fps, phase {phase}: let go {off_center} m off its thrower's middle");
            }
        }
        for &(fps, phase, height) in &heights {
            println!("{fps:>5} fps, phase {phase}: let go with its point at {height:.3} m (flies at {THROWN_HEIGHT} m)");
            let slack = RELEASE_HEIGHT_SLACK.iter().find(|(at, _)| *at == fps).map(|(_, slack)| *slack).unwrap();
            assert!((height - THROWN_HEIGHT).abs() < slack, "at {fps} fps, phase {phase}: let go at {height} m, off the height it flies at");
        }
    }

    /// How far (meters) over its thrower's weapon shoulder (the joint) a thrown spear passes as
    /// it's let go, at least.
    const SHOULDER_CLEARANCE: f32 = 0.2;

    /// How far (meters) from the height a thrown spear flies its point may be let go, by frame
    /// rate: the arm snaps through the throw in a few frames at the lowest, which only roughly
    /// follow it.
    const RELEASE_HEIGHT_SLACK: [(f32, f32); 3] = [(30.0, 0.15), (60.0, 0.04), (144.0, 0.02)];
}
