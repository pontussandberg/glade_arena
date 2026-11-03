//! Shots in flight: where each is drawn every frame (`fly_shots`), a thrown spear's hand-off
//! from the hand, its curve onto its real path and the wake of wind it leaves behind, and the
//! end of a shot (`strike_home`): a spear's strike, a bolt's burst.

use super::*;

use bevy::light::NotShadowCaster;

use arena_shared::sim;

use crate::feedback::AttackClock;
use crate::arena::{self, to_world};
use crate::plain::PlainMaterial;
use crate::rig::{HeldAt, MAX_HOLD_TICKS};

/// How long a thrown spear takes to turn from how it was held to the way it flies, and to catch
/// up its real path from a hand behind it (seconds).
const SETTLE_SECONDS: f32 = 0.2;
/// How far (meters) a thrown spear flies before it has come in from where it left the hand (to
/// the side and above) onto its real path: one long, gentle curve, no dive.
const CONVERGE_METERS: f32 = 7.0;
/// Others' spears reach us a round trip (and a bit) after they're thrown, when their real path
/// is already meters ahead of the hand. A spear never starts more than this far (meters) behind
/// it; beyond that (a very high ping) the rest is skipped.
const MAX_CATCH_UP: f32 = 8.0;
/// How long a piece of the wake stays where it was left (seconds), and how much of that it keeps
/// its full width before thinning away to nothing.
const WAKE_SECONDS: f32 = 0.32;
const WAKE_HOLDS: f32 = 0.35;
/// How far the spear's point gets from where it left the hand before it leaves any wake: none
/// in the thrower's face.
const WAKE_AFTER: f32 = 1.0;

/// A thrown spear as drawn: it leaves the thrower's hand where the javelin was held, then eases
/// onto its real path (`fly_shots`), leaving a wake of wind behind it (`Wake`), in `wake`'s
/// material.
#[derive(Component)]
pub(super) struct Thrown {
    pub(super) launch: Option<Launch>,
    pub(super) wake: Handle<PlainMaterial>,
    /// Where its wake came off it last frame, where the next piece of its wake starts.
    pub(super) wake_at: Option<Vec3>,
}

/// A piece of a thrown spear's wake: a thin, even line of wind from where it came off the spear
/// one frame to where it did the next, left there in the world from `born` (seconds), `length` long. Piece
/// after piece, they make one smooth line along the spear's flight, holding a moment, then thinning
/// away from the oldest end: the wind it parted, left behind (`fade_wakes`).
#[derive(Component)]
pub(super) struct Wake {
    born: f32,
    length: f32,
}

/// A shot that turns about its flight this fast (radians per second): a frostbolt.
#[derive(Component)]
pub(super) struct Spin(pub(super) f32);

/// When (seconds) and where a thrown spear left the hand: its point, how far that is off its
/// real path and how it was held (as a shot's rotation).
#[derive(Clone, Copy)]
pub(super) struct Launch {
    pub(super) at: f32,
    pub(super) from: Vec3,
    pub(super) offset: Vec3,
    pub(super) held: Quat,
    /// As big as the thrower drew it in the hand.
    pub(super) scale: Vec3,
    /// How far (ticks) behind its real path it left the hand, along it: our own held drawn, let
    /// go only once the hand has driven it up to the top of the throw (`HeldAt::drive`).
    pub(super) lag: f32,
    /// How far (meters) it flies before it has come in onto its real path.
    pub(super) converge: f32,
}

/// A thrown spear as it leaves the hand that holds it at `held` (`HeldAt::at`, the grip, the
/// spear along +Y): drawn just over the held one, its point where that one's is (the flying
/// spear's mesh has its point at its origin, along +X), turned and as big as it's held.
pub(crate) fn leaving_hand(held: &Transform) -> Transform {
    Transform {
        translation: held.transform_point(Vec3::Y * arena::GRIP_TO_TIP),
        rotation: held.rotation * Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
        scale: held.scale,
    }
}

/// Places every shot each frame from where and when it was thrown, on our own fighter's clock
/// (between ticks too, so it glides instead of stepping): a shot coming at us is drawn where it is
/// relative to us. Never before where it starts.
///
/// A thrown spear leaves the hand: out of sight until the thrower's hand has let go of it (our
/// own fighter is drawn a moment behind the news of its throw). Our own then appears dead on the
/// crosshair's line at the very start of its path, level with the body (never out in front of
/// it), and catches up to where it really is as it settles. Another's takes over
/// the held javelin's place (where the throw sends it, see `HeldAt`),
/// then comes in onto its real path (which starts at the thrower's center, where hits are judged)
/// along one gentle curve over its first `CONVERGE_METERS`, across and down (or up) at once:
/// heading in from the moment it leaves the hand, it meets the path running along it, its nose
/// following the curve, turned from how it was held within `SETTLE_SECONDS`. A dark streak on the ground straight under it shows where it really is.
/// A hand ahead of the real path is given back more slowly, so the spear never seems to slow
/// below 3/4 speed; one behind it is caught up within `SETTLE_SECONDS` however far behind it is,
/// so others' spears (which reach us late, already meters down their path) shoot out of the hand
/// fast and are where they really are almost at once. It leaves its wake behind it as it goes
/// (`Wake`). A thrown ability's shot isn't the held javelin (that stays in the hand), nor is one an item threw
/// (a barrage's, a bounce): it shoots straight out from where the sim threw it, already pointing
/// the way it flies.
pub(super) fn fly_shots(
    mut commands: Commands,
    time: Res<Time>,
    clock: AttackClock,
    visuals: Res<Visuals>,
    mut holders: Query<(&PlayerId, &mut HeldAt, Has<Me>)>,
    mut shots: Query<(&Projectile, &mut Transform, &mut Visibility, Option<&mut Thrown>, Option<&Spin>), With<Mesh3d>>,
) {
    let now = time.elapsed_secs();
    let tick = clock.now(true);
    for (projectile, mut transform, mut visibility, thrown, spin) in &mut shots {
        let speed = projectile.class.def().shot(projectile.ability).map_or(0.0, |s| s.speed);
        let at_tick = tick.max(projectile.spawn_tick as f32 - 1.0);
        // Just where the sim has it: aimed up or down (the camera's), it rises or falls
        // as it flies (and a thrown one drops from the hand on), its nose along its flight. A
        // thrown one is let go a little higher than that and comes onto its path (`Thrown`).
        let path_at = |tick: f32| {
            let height = sim::projectile_height(projectile, tick);
            let on_path = to_world(sim::projectile_pos(projectile, tick), height);
            let along = Quat::from_rotation_y(projectile.dir.to_angle());
            let tilt = Quat::from_rotation_z(sim::projectile_pitch(projectile, tick).atan());
            (height, on_path, along, tilt)
        };
        let (height, on_path, along, tilt) = path_at(at_tick);
        let forward_dir = along * Vec3::X;
        let Some(mut thrown) = thrown else {
            let spun = spin.map_or(Quat::IDENTITY, |s| Quat::from_rotation_x(s.0 * now));
            transform.set_if_neq(Transform::from_translation(on_path).with_rotation(along * tilt * spun));
            continue;
        };
        let launch = match thrown.launch {
            Some(launch) => launch,
            // Not yet let go of: the thrower's hand still swinging it (our own fighter is drawn a
            // moment behind the news of its throw). Out of sight until it is, or until it's late.
            None if projectile.ability.is_none()
                && !projectile.extra
                && holders.iter().any(|(id, held, _)| {
                    id.0 == projectile.owner && tick < projectile.spawn_tick as f32 + held.drive + MAX_HOLD_TICKS && held.let_go.is_none_or(|at| at < projectile.spawn_tick)
                }) =>
            {
                visibility.set_if_neq(Visibility::Hidden);
                continue;
            }
            None => {
                visibility.set_if_neq(Visibility::Inherited);
                // Where it starts, which way it points and how big it is: an ability's where the
                // sim threw it (just outside the thrower's body), along its flight; our own
                // auto-attack on its path, on the crosshair's line, the throw selling it, as big
                // as it's drawn in the hand; another's just as the held javelin is this frame (its
                // point, held pointing up, +Y, a shot along +X), the hand letting go of it.
                let start = if projectile.ability.is_some() || projectile.extra {
                    Some((to_world(projectile.origin, height), along, Vec3::ONE, 0.0, CONVERGE_METERS))
                } else {
                    holders.iter_mut().find(|(id, ..)| id.0 == projectile.owner).map(|(_, mut held, me)| {
                        held.launched = held.launched.max(Some(projectile.spawn_tick));
                        let leaving = leaving_hand(&held.at);
                        if me {
                            // Dead on the crosshair's line, but at the very start of its path,
                            // level with the body -- never popping in out in front of it -- and
                            // caught up to where it really is as it settles.
                            let lag = at_tick - (projectile.spawn_tick as f32 - 1.0);
                            let (_, from, along, tilt) = path_at(at_tick - lag);
                            (from, along * tilt, leaving.scale, lag, CONVERGE_METERS)
                        } else {
                            (leaving.translation, leaving.rotation, leaving.scale, 0.0, CONVERGE_METERS)
                        }
                    })
                };
                *thrown.launch.insert(match start {
                    Some((from, rotation, scale, lag, converge)) => {
                        // No more than `MAX_CATCH_UP` behind its real path.
                        let mut offset = from - path_at(at_tick - lag).1;
                        let excess = offset.dot(forward_dir) + MAX_CATCH_UP;
                        offset -= forward_dir * excess.min(0.0);
                        Launch { at: now, from, offset, held: rotation, scale, lag, converge }
                    }
                    // No thrower to be seen: straight from its path.
                    None => Launch { at: now, from: on_path, offset: Vec3::ZERO, held: along, scale: Vec3::ONE, lag: 0.0, converge: CONVERGE_METERS },
                })
            }
        };
        let since = now - launch.at;
        let rise = launch.offset.y;
        let flat = launch.offset.with_y(0.0);
        let ahead = flat.dot(forward_dir);
        let across = flat - forward_dir * ahead;
        // Eased out (1 to 0 over `seconds`), so the spear runs onto its path without a kink.
        let left = |seconds: f32| (1.0 - since / seconds).max(0.0).powi(2);
        // One left behind along its path catches up along it within `SETTLE_SECONDS`, on just
        // the curve it flies (onto the crosshair's line), only a moment late.
        let (_, on_path, along, tilt) = if launch.lag > 0.0 { path_at(at_tick - launch.lag * left(SETTLE_SECONDS)) } else { (height, on_path, along, tilt) };
        let forward_dir = along * Vec3::X;
        let give_back = SETTLE_SECONDS.max(8.0 * ahead / speed.max(1.0));
        // Across and down onto its path over `Launch::converge` of flight, on a parabola: leaving
        // the hand heading in, meeting the path along it; the nose along the curve.
        let off = across + Vec3::Y * rise;
        let flown = (on_path - (launch.from - launch.offset)).dot(forward_dir).max(0.0);
        let rest = (1.0 - flown / launch.converge).max(0.0);
        let at = on_path + off * rest * rest + forward_dir * ahead * left(give_back);
        let heading = (forward_dir - off * 2.0 * rest / launch.converge).normalize();
        let flying = Quat::from_rotation_arc(forward_dir, heading) * along * tilt;
        let turned = 1.0 - left(SETTLE_SECONDS);
        transform.set_if_neq(Transform::from_translation(at).with_rotation(launch.held.slerp(flying, turned)).with_scale(launch.scale));
        // A piece of wake from where it came off the spear last frame to where it does now, left in
        // the world.
        // Off its point: the line runs along it, and when it strikes home it reaches right there.
        let wake_at = transform.translation;
        if let Some(from) = thrown.wake_at
            && at.distance(launch.from) >= WAKE_AFTER
            && let Some(dir) = (wake_at - from).try_normalize()
        {
            // A touch longer than the gap, so the pieces meet without seams.
            let length = from.distance(wake_at) * 1.05;
            let placed = Transform::from_translation(from).with_rotation(Quat::from_rotation_arc(Vec3::X, dir)).with_scale(Vec3::new(length, 1.0, 1.0));
            commands.spawn((Wake { born: now, length }, Mesh3d(visuals.wake.clone()), MeshMaterial3d(thrown.wake.clone()), placed, NotShadowCaster));
        }
        thrown.wake_at = Some(wake_at);
    }
}

/// A thrown spear striking home: a flash at its point and splinters of light thrown back out of
/// it, so it doesn't just vanish. How long the flash lasts and how big it gets (seconds, meters);
/// how many splinters, how fast they fly back and up (m/s), how long they last, how big, how drawn
/// out along their flight, and how hard they fall (m/s²).
const FLASH_SECONDS: f32 = 0.08;
const FLASH_SIZE: f32 = 0.055;
const SPLINTERS: u32 = 7;
const SPLINTER_SPEED: (f32, f32) = (2.2, 4.2);
const SPLINTER_RISE: f32 = 1.4;
const SPLINTER_LIFE: (f32, f32) = (0.18, 0.34);
const SPLINTER_SIZE: (f32, f32) = (0.012, 0.022);
const SPLINTER_STRETCH: f32 = 4.0;
const SPLINTER_FALL: f32 = 9.0;
/// A spear gone this much short of its range (meters) was stopped by something, not spent.
const STOPPED_SHORT: f32 = 0.4;
/// A shot removed within this long of a map change (seconds) was cleared away by it, not spent
/// on anything: no strike to show.
const MAP_CLEAR_SECONDS: f32 = 0.5;
/// A spear ending this near a fighter's body (meters) struck it, not what's around it.
const TING_SLACK: f32 = 0.4;
/// A spear stands in a body a little smaller than it flew (`STUCK_SCALE` of it), its point run
/// in past the whole blade and a hand of shaft (`BODY_BURY`), so a crowd of them reads as pins,
/// not a thicket; into a wall, a crate or the ground it stays full size, buried near the whole
/// blade (`TERRAIN_BURY`).
const STUCK_SCALE: f32 = 0.9;
const BODY_BURY: f32 = (arena::SPEAR_BLADE_LENGTH + 0.35) * STUCK_SCALE;
const TERRAIN_BURY: f32 = arena::SPEAR_BLADE_LENGTH * 0.95;
/// How long a spear stuck in terrain stands, how long one stuck in a body does, and how many may
/// stand at once before the oldest goes early -- roomy enough (measured free to render) that
/// the backstop stays out of sight even spamming throws at practice attack speeds. Its time
/// come, it slides on in along its own line, swallowed by what holds it, over `STUCK_SINK`
/// seconds.
const STUCK_SECONDS: f32 = 10.0;
const BODY_STUCK_SECONDS: f32 = 10.0;
const STUCK_AT_MOST: usize = 120;
const STUCK_SINK: f32 = 0.4;
/// A bolt bursting as it's spent (`strike_home`): how long its flash lasts and how big it gets
/// (seconds, meters); how many sparks fly out of it every way, how fast (m/s) and for how long.
const BURST_SECONDS: f32 = 0.12;
const BURST_SIZE: f32 = 0.14;
const BURST_SPARKS: u32 = 10;
const BURST_SPEED: (f32, f32) = (1.6, 3.6);
const BURST_LIFE: (f32, f32) = (0.2, 0.4);

/// A piece of a spear's impact (`strike_home`), from `started`: the flash, or a splinter flying
/// at `velocity`.
#[derive(Component)]
pub(super) struct Impact {
    started: f32,
    life: f32,
    size: f32,
    velocity: Option<Vec3>,
}

/// A spear left standing where it struck (`strike_home`), from `born` (seconds): `held` in a
/// body (a child of it, riding along, gone with it) or in the ground or a wall, until its time
/// comes and it slides away into what holds it (`fade_stuck_spears`; `sinking` since when, from
/// where it stood).
#[derive(Component)]
pub(super) struct StuckSpear {
    born: f32,
    held: bool,
    sinking: Option<(f32, Vec3)>,
}

/// Shots just removed, captured as the removal is applied (`strike_home`) and shown the same
/// frame once the frame's news is in (`resolve_strikes`): judged in the observer itself, a
/// removal riding the packet that also changes the map or ends the match would be judged against
/// last frame's map, clock and clear marks.
#[derive(Resource, Default)]
pub(super) struct Strikes(Vec<Strike>);

/// What a strike needs of the shot the moment it went: everything else is put back together from
/// the sim.
struct Strike {
    projectile: Projectile,
    /// As big as it was drawn in flight (never shown flying, as big as its thrower draws it).
    scale: Vec3,
    thrown: bool,
    visible: bool,
    mesh: Handle<Mesh>,
    light: Handle<PlainMaterial>,
    seed: u32,
}

/// Notes a removed shot for `resolve_strikes`, with what only exists while it's being removed.
pub(super) fn strike_home(
    trigger: On<Remove, Projectile>,
    mut strikes: ResMut<Strikes>,
    shots: Query<(&Projectile, Option<&Thrown>, &InheritedVisibility, &Mesh3d, &MeshMaterial3d<PlainMaterial>)>,
) {
    let Ok((projectile, thrown, visible, mesh, own_light)) = shots.get(trigger.entity) else { return };
    let scale = thrown.and_then(|t| t.launch.map(|l| l.scale)).unwrap_or(Vec3::splat(crate::rig::drawn_size(projectile.class, false)));
    strikes.0.push(Strike {
        projectile: *projectile,
        scale,
        thrown: thrown.is_some(),
        visible: visible.get(),
        mesh: mesh.0.clone(),
        light: own_light.0.clone(),
        seed: trigger.entity.index().index(),
    });
}

/// A thrown spear gone short of its range hit something (a fighter, a wall, a rock, a tree), and
/// one that falls (`Shot::falls`) ends on something (or the ground) past it too: a flash where
/// its point was, splinters thrown back the way it came, out and up, and the spear itself left
/// stuck where it struck (`StuckSpear`): its point run into the body it hit, riding along with
/// it, or just its tip into the terrain that stopped it; one ending in the air (over water, off
/// the map) just ends. The drawn spear is a beat behind or past the server's clock when the news
/// of its end arrives, so nothing here goes by where it was drawn: the end is put back together
/// from the sim -- the tick the news came from (`AttackClock::latest_tick`), the flight both
/// sides compute alike, and the map. A bolt that flew its whole range
/// bursts there, in its own light, as if it lost hold of itself: a bigger flash, sparks flung
/// out every way. A bolt stopped short just ends
/// (its hit is the fighter's to show), and so does every one cleared away mid-flight: at the
/// end of a won match, or with the map changed.
#[allow(clippy::too_many_arguments)]
pub(super) fn resolve_strikes(
    mut strikes: ResMut<Strikes>,
    mut commands: Commands,
    time: Res<Time>,
    clock: AttackClock,
    room: Res<crate::rooms::CurrentRoom>,
    map: Res<crate::rooms::CurrentMap>,
    map_changed: Res<crate::rooms::MapChangedAt>,
    mut visuals: ResMut<Visuals>,
    mut materials: ResMut<Assets<PlainMaterial>>,
    mut sounds: ResMut<crate::audio::SoundQueue>,
) {
    if strikes.0.is_empty() {
        return;
    }
    let cleared = room.0.as_ref().is_some_and(|view| view.winner.is_some()) || time.elapsed_secs() - map_changed.0 < MAP_CLEAR_SECONDS;
    for strike in std::mem::take(&mut strikes.0) {
        if cleared {
            continue;
        }
        resolve_strike(&strike, (&mut commands, &time, &clock, &map, &mut visuals, &mut materials, &mut sounds));
    }
}

/// One removed shot's strike (`resolve_strikes`).
fn resolve_strike(
    strike: &Strike,
    (commands, time, clock, map, visuals, materials, sounds): (
        &mut Commands,
        &Res<Time>,
        &AttackClock,
        &Res<crate::rooms::CurrentMap>,
        &mut ResMut<Visuals>,
        &mut ResMut<Assets<PlainMaterial>>,
        &mut ResMut<crate::audio::SoundQueue>,
    ),
) {
    let projectile = &strike.projectile;
    let Some(shot) = projectile.class.def().shot(projectile.ability) else { return };
    let Some(removed) = clock.latest_tick() else { return };
    let ground = map.0.map();
    let at = |tick: f32| to_world(sim::projectile_pos(projectile, tick), sim::projectile_height(projectile, tick));
    let posed = |tick: f32| Quat::from_rotation_y(projectile.dir.to_angle()) * Quat::from_rotation_z(sim::projectile_pitch(projectile, tick).atan());
    // The tick the map itself would end it, looked for up to a couple past the removal (the
    // removal's tick is read a packet late): removed earlier than that, it ended in a fighter.
    let map_end = (projectile.spawn_tick..=removed.saturating_add(2))
        .find(|&t| sim::projectile_expired(ground, sim::projectile_pos(projectile, t as f32), projectile, t));
    let end_tick = map_end.unwrap_or(removed);
    let flown = sim::projectile_flown(projectile, removed as f32);
    let spent = flown > shot.range - STOPPED_SHORT;
    let now = time.elapsed_secs();
    let seed = strike.seed;
    // A spear strikes whether or not it was ever shown flying: thrown point blank, it ends
    // before the hand has visibly let go of it, and the strike is all there is to see of it.
    if !strike.thrown {
        if spent && strike.visible {
            burst(commands, visuals, strike.light.clone(), at(end_tick as f32), now, seed);
        }
        return;
    }
    if spent && !shot.falls {
        return;
    }
    // Where the map ended it, or would have a tick or two on: the spear stands exactly there if
    // nothing stopped it sooner; nothing in its way at all (spent in the air over water, off the
    // map), it just ends.
    let ground_end = map_end.and_then(|e| {
        let step = |tick: f32| (sim::projectile_pos(projectile, tick), sim::projectile_height(projectile, tick));
        ground.shot_end(step(e as f32 - 1.0), step(e as f32)).map(|found| (to_world(found.at, found.height), posed(e as f32)))
    });
    // Removed before the map would have ended it: it struck a fighter on the stretch judged the
    // tick it was removed -- reaching back far enough to cover a point-blank hit, whose removal
    // (lingering a tick for the wire) and the tick it's read at both trail the hit itself. Which
    // body is `pin_stuck_spears`' to settle (the damage dealt names it), so the spear first
    // stands free at that stretch's far end, seeking (`PinTo`); the removal and the map's end
    // read a tick apart either way, so a seeker that finds no one takes the map's end instead.
    let into_fighter = map_end.is_none_or(|e| e > removed);
    let seek = into_fighter.then(|| {
        let from = at((removed as f32 - 3.0).max(projectile.spawn_tick as f32 - 1.0));
        let to = at(map_end.map_or(removed as f32 + 1.0, |e| e as f32));
        (from, to)
    });
    let end = if into_fighter { None } else { ground_end };
    // The flash and splinters right at the strike; ended in the air, where the sim ends it.
    let turned = end.map_or_else(|| posed(removed as f32), |(_, turned)| turned);
    let point = match (seek, end) {
        (Some((_, to)), _) => to,
        (_, Some((strike, _))) => strike,
        _ => at(end_tick as f32),
    };
    let light = visuals.material(materials, Look::Spark);
    let bit = || (Mesh3d(visuals.impact.clone()), MeshMaterial3d(light.clone()), Transform::from_translation(point).with_scale(Vec3::ZERO), NotShadowCaster);
    commands.spawn((Impact { started: now, life: FLASH_SECONDS, size: FLASH_SIZE, velocity: None }, bit()));
    let back = -(turned * Vec3::X).with_y(0.0).normalize_or(Vec3::X);
    let across = Vec3::new(-back.z, 0.0, back.x);
    for k in 0..SPLINTERS {
        let r = |i: u32| crate::swish::noise(seed, k * 8 + i);
        let speed = SPLINTER_SPEED.0 + (SPLINTER_SPEED.1 - SPLINTER_SPEED.0) * r(0);
        let velocity = (back + across * (r(1) * 2.0 - 1.0) * 1.1).normalize() * speed + Vec3::Y * SPLINTER_RISE * (0.4 + r(2));
        let size = SPLINTER_SIZE.0 + (SPLINTER_SIZE.1 - SPLINTER_SIZE.0) * r(3);
        let life = SPLINTER_LIFE.0 + (SPLINTER_LIFE.1 - SPLINTER_LIFE.0) * r(4);
        commands.spawn((Impact { started: now, life, size, velocity: Some(velocity) }, bit()));
    }
    // Into anything but a fighter (whose hit sounds for itself), a miss: a faint chip on stone,
    // heard near it, or however far off if it's our own spear.
    if !into_fighter {
        sounds.add(crate::audio::Sfx::SpearTing, 1.0).from(arena::to_gameplay(point), crate::audio::Reach::Combat).by(projectile.owner);
    }
    // The spear stays where it struck: its tip run into the terrain that ended it, or seeking
    // the body it stands in (`pin_stuck_spears`).
    let placed = |tip: Vec3| Transform::from_translation(tip).with_rotation(turned).with_scale(strike.scale);
    match (seek, end) {
        (Some((from, to)), _) => {
            let placed = placed(to);
            let fallback = ground_end.map(|(strike, turned)| (strike + turned * Vec3::X * TERRAIN_BURY, turned));
            commands.spawn((StuckSpear { born: now, held: true, sinking: None }, PinTo { owner: projectile.owner, from, to, radius: shot.radius, born: now, fallback }, Mesh3d(strike.mesh.clone()), MeshMaterial3d(strike.light.clone()), placed));
        }
        (None, Some((at, _))) => {
            let placed = placed(at + turned * Vec3::X * TERRAIN_BURY);
            commands.spawn((StuckSpear { born: now, held: false, sinking: None }, Mesh3d(strike.mesh.clone()), MeshMaterial3d(strike.light.clone()), placed));
        }
        _ => {}
    }
}

/// How long a seeking spear waits for the damage to name its body before settling for the
/// nearest one along its stretch instead (seconds), and how much earlier than the spear's end
/// the naming hit may have come: the two travel separately and either can arrive a beat first.
const PIN_GRACE: f32 = 0.25;
const NAMED_SLACK: f32 = 0.35;

/// A stuck spear still seeking the body it stands in: it struck one of `owner`'s foes somewhere
/// along `from`..`to` (the stretch judged the tick it was removed, a shot `radius` wide), and
/// the one it damaged gives itself away by a fresh hit of `owner`'s. No one to be found at all,
/// it takes `fallback`, where the map would have ended it: the removal and the map's end are
/// read a tick apart either way, so a wall hit can first look like a fighter's.
#[derive(Component)]
pub(super) struct PinTo {
    owner: PeerId,
    from: Vec3,
    to: Vec3,
    radius: f32,
    born: f32,
    fallback: Option<(Vec3, Quat)>,
}

/// Pins each seeking spear (`PinTo`, `resolve_strikes`) into its body: the foe along its stretch
/// whose fresh hit the spear's owner dealt around the spear's own end (noted here from
/// `RecentHits` the frame each hit arrives, and matched within `NAMED_SLACK` either way, so
/// neither packet order nor the owner's other hits moments later can mislead it; through
/// several, an Impaler's, the farthest along, the one
/// it stuck in), its point run `BODY_BURY` in from the side it came, left a child of the
/// fighter, riding along with it (and gone with it). The damage not come within `PIN_GRACE`, the
/// nearest foe along the stretch; no one there at all, the terrain the map blames (`fallback`),
/// with the chip ting a terrain strike gets; nothing anywhere, it stays standing free.
/// Foes are judged where the server last had them (`Pos`), but the spear is pinned onto the body
/// as drawn; the shopkeeper and allies are passed over, as the server flies shots through them.
#[allow(clippy::type_complexity)]
pub(super) fn pin_stuck_spears(
    mut commands: Commands,
    time: Res<Time>,
    mut sounds: ResMut<crate::audio::SoundQueue>,
    mut spears: Query<(Entity, &PinTo, &mut StuckSpear, &mut Transform)>,
    fighters: Query<(Entity, &PlayerId, &Transform, &Pos, Option<&Team>, (&ClassId, Has<Elite>), Option<&RecentHits>, Has<Shopkeeper>), Without<PinTo>>,
    mut seen: Local<bevy::platform::collections::HashMap<Entity, u32>>,
    mut named: Local<Vec<(Entity, PeerId, f32)>>,
) {
    let now = time.elapsed_secs();
    // Who was just hit by whom: each new hit noted the frame it arrives (a fighter's hits already
    // there when it's first seen are only noted), kept a moment.
    seen.retain(|&body, _| fighters.contains(body));
    named.retain(|&(_, _, at)| now - at < 1.0);
    let mut teams: bevy::platform::collections::HashMap<PeerId, Option<Team>> = default();
    let scanning = !spears.is_empty();
    for (body, id, _, _, team, _, hits, _) in &fighters {
        if scanning {
            teams.insert(id.0, team.copied());
        }
        let Some(hits) = hits else { continue };
        let Some(&last) = seen.get(&body) else {
            seen.insert(body, hits.seq());
            continue;
        };
        if hits.seq() == last {
            continue;
        }
        named.extend(hits.0.iter().filter(|hit| hit.seq > last).filter_map(|hit| hit.by.map(|by| (body, by, now))));
        seen.insert(body, hits.seq());
    }
    for (spear, pin, mut stuck, mut transform) in &mut spears {
        let own_team = teams.get(&pin.owner).copied().flatten();
        let heading = (pin.to - pin.from).normalize_or(Vec3::X);
        let level = heading.with_y(0.0);
        let ahead = level.normalize_or(heading);
        let span = (pin.to - pin.from).with_y(0.0).length();
        // The farthest named body along the stretch, and the nearest of any kind, in one pass.
        let (mut named_body, mut nearest): (Option<(Entity, Transform, f32, f32, f32)>, Option<(Entity, Transform, f32, f32, f32)>) = (None, None);
        for (body, id, drawn, pos, team, (class, elite), _, keeper) in &fighters {
            if id.0 == pin.owner || keeper || own_team.zip(team.copied()).is_some_and(|(mine, theirs)| mine.allied(theirs)) {
                continue;
            }
            let size = class.def().hit_size(elite);
            let to_body = (to_world(pos.0, 0.0) - pin.from).with_y(0.0);
            let along = to_body.dot(ahead);
            let height = pin.from.y + heading.y * along / level.length().max(1e-3);
            let feet = drawn.translation.y;
            let near = (-TING_SLACK..=span + TING_SLACK).contains(&along)
                && to_body.distance(ahead * along) < body_radius(size, height - feet) + pin.radius + TING_SLACK
                && (feet - TING_SLACK..feet + BODY_HEIGHT * size + TING_SLACK).contains(&height);
            if !near {
                continue;
            }
            let found = (body, *drawn, along, height, size);
            if named.iter().any(|&(hit, by, at)| hit == body && by == pin.owner && (at - pin.born).abs() <= NAMED_SLACK) && named_body.is_none_or(|best| along > best.2) {
                named_body = Some(found);
            }
            if nearest.is_none_or(|best| along < best.2) {
                nearest = Some(found);
            }
        }
        let settle = now - pin.born > PIN_GRACE;
        let Some((body, drawn, _, height, size)) = named_body.or(settle.then_some(nearest).flatten()) else {
            if settle {
                if let Some((tip, turned)) = pin.fallback {
                    let scale = transform.scale;
                    *transform = Transform::from_translation(tip).with_rotation(turned).with_scale(scale);
                    stuck.held = false;
                    sounds.add(crate::audio::Sfx::SpearTing, 1.0).from(arena::to_gameplay(tip), crate::audio::Reach::Combat).by(pin.owner);
                }
                commands.entity(spear).remove::<PinTo>();
            }
            continue;
        };
        let tip = drawn.translation.with_y(height) - ahead * body_radius(size, height - drawn.translation.y) + transform.rotation * Vec3::X * BODY_BURY;
        let placed = Transform::from_translation(tip).with_rotation(transform.rotation).with_scale(transform.scale * STUCK_SCALE);
        *transform = Transform::from_matrix(drawn.to_matrix().inverse() * placed.to_matrix());
        commands.entity(spear).insert(ChildOf(body)).remove::<PinTo>();
    }
}

/// A bolt bursting at `point` as it's spent, in `light`: its flash, and sparks flung out every
/// way (up more than down), falling and winking out like a spear's splinters.
fn burst(commands: &mut Commands, visuals: &Visuals, light: Handle<PlainMaterial>, point: Vec3, now: f32, seed: u32) {
    let bit = || (Mesh3d(visuals.impact.clone()), MeshMaterial3d(light.clone()), Transform::from_translation(point).with_scale(Vec3::ZERO), NotShadowCaster);
    commands.spawn((Impact { started: now, life: BURST_SECONDS, size: BURST_SIZE, velocity: None }, bit()));
    for k in 0..BURST_SPARKS {
        let r = |i: u32| crate::swish::noise(seed, k * 8 + i);
        let dir = Vec3::new(r(0) * 2.0 - 1.0, r(1) * 1.4 - 0.4, r(2) * 2.0 - 1.0).normalize_or(Vec3::Y);
        let speed = BURST_SPEED.0 + (BURST_SPEED.1 - BURST_SPEED.0) * r(3);
        let size = SPLINTER_SIZE.0 + (SPLINTER_SIZE.1 - SPLINTER_SIZE.0) * r(4);
        let life = BURST_LIFE.0 + (BURST_LIFE.1 - BURST_LIFE.0) * r(5);
        commands.spawn((Impact { started: now, life, size, velocity: Some(dir * speed) }, bit()));
    }
}

/// Stuck spears slide away into what holds them when their time comes -- driven on along their
/// own line, ever faster, until swallowed by the body, wall or ground they stand in -- one in a
/// body after `BODY_STUCK_SECONDS`, one in terrain after `STUCK_SECONDS`, or early, oldest
/// first, once the standing crowd is over its budget (`STUCK_AT_MOST`). With the
/// map gone from under them, all are gone at once.
pub(super) fn fade_stuck_spears(
    mut commands: Commands,
    time: Res<Time>,
    map: Res<crate::rooms::CurrentMap>,
    mut spears: Query<(Entity, &mut StuckSpear, &mut Transform)>,
) {
    let now = time.elapsed_secs();
    if map.is_changed() {
        for (entity, ..) in &spears {
            commands.entity(entity).despawn();
        }
        return;
    }
    let mut standing: Vec<(Entity, f32)> = Vec::new();
    for (entity, mut spear, transform) in &mut spears {
        let stays = if spear.held { BODY_STUCK_SECONDS } else { STUCK_SECONDS };
        match spear.sinking {
            None if now - spear.born >= stays => spear.sinking = Some((now, transform.translation)),
            None if !spear.held => standing.push((entity, spear.born)),
            _ => {}
        }
    }
    if standing.len() > STUCK_AT_MOST {
        standing.sort_by(|a, b| a.1.total_cmp(&b.1));
        for &(entity, _) in &standing[..standing.len() - STUCK_AT_MOST] {
            if let Ok((_, mut spear, transform)) = spears.get_mut(entity) {
                spear.sinking = Some((now, transform.translation));
            }
        }
    }
    for (entity, spear, mut transform) in &mut spears {
        let Some((started, from)) = spear.sinking else { continue };
        let sunk = (now - started) / STUCK_SINK;
        if sunk >= 1.0 {
            commands.entity(entity).despawn();
        } else {
            transform.translation = from + transform.rotation * Vec3::X * (arena::SPEAR_LENGTH * sunk * sunk);
        }
    }
}

/// The flash swells and is gone; splinters fly, fall, and wink out.
pub(super) fn fly_impacts(mut commands: Commands, time: Res<Time>, mut impacts: Query<(Entity, &mut Impact, &mut Transform)>) {
    let (now, dt) = (time.elapsed_secs(), time.delta_secs().min(0.05));
    for (entity, mut impact, mut transform) in &mut impacts {
        let t = (now - impact.started) / impact.life;
        if t >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }
        let size = impact.size;
        match impact.velocity.as_mut() {
            None => transform.scale = Vec3::splat(size * (t * std::f32::consts::PI).sin()),
            Some(velocity) => {
                velocity.y -= SPLINTER_FALL * dt;
                transform.translation += *velocity * dt;
                transform.translation.y = transform.translation.y.max(0.02);
                let size = size * (1.0 - t * t);
                transform.rotation = Quat::from_rotation_arc(Vec3::Y, velocity.normalize_or(Vec3::Y));
                transform.scale = Vec3::new(size, size * SPLINTER_STRETCH, size);
            }
        }
    }
}

/// Each piece of wake keeps its width a moment, then thins away to nothing, and is gone.
pub(super) fn fade_wakes(mut commands: Commands, time: Res<Time>, mut wakes: Query<(Entity, &Wake, &mut Transform)>) {
    let now = time.elapsed_secs();
    for (entity, wake, mut transform) in &mut wakes {
        let age = (now - wake.born) / WAKE_SECONDS;
        if age >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }
        let width = (1.0 - (age - WAKE_HOLDS).max(0.0) / (1.0 - WAKE_HOLDS)).powi(2);
        transform.scale = Vec3::new(wake.length, width, width);
    }
}
