//! Shots in flight: where each is drawn every frame (`fly_shots`), and a thrown spear's hand-off
//! from the hand, its curve onto its real path and the wake of wind it leaves behind.

use super::*;

use bevy::light::NotShadowCaster;

use arena_shared::sim;

use crate::feedback::AttackClock;
use crate::arena::{self, to_world};
use crate::rig::{HeldAt, MAX_HOLD_TICKS};

/// Shots fly at chest height; fighters stand on the floor (their feet are at the mesh origin).
const PROJECTILE_HEIGHT: f32 = 0.9;
/// Thrown spears fly a little higher, at the height the throw lets go of them (`rig/` times the
/// release to it), so they leave the hand straight down their line instead of sinking onto it.
pub(crate) const THROWN_HEIGHT: f32 = 1.2;

/// How high a shot flies: a thrown spear at `THROWN_HEIGHT`, the rest at `PROJECTILE_HEIGHT`.
pub(super) fn shot_height(thrown: bool) -> f32 {
    if thrown { THROWN_HEIGHT } else { PROJECTILE_HEIGHT }
}
/// How long a thrown spear takes to turn from how it was held to the way it flies, and to catch
/// up its real path from a hand behind it (seconds).
const SETTLE_SECONDS: f32 = 0.2;
/// How far (meters) a thrown spear flies before it has come in from where it left the hand (to
/// the side and above) onto its real path at `THROWN_HEIGHT`: one long, gentle curve, no dive.
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
    pub(super) wake: Handle<StandardMaterial>,
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
/// own fighter is drawn a moment behind the news of its throw), then it takes over the held
/// javelin's place (where the throw sends it, see `HeldAt`),
/// then comes in onto its real path (which starts at the thrower's center, where hits are judged)
/// along one gentle curve over its first `CONVERGE_METERS`, across and down (or up) at once:
/// heading in from the moment it leaves the hand, it meets the path running along it, its nose
/// following the curve, turned from how it was held within `SETTLE_SECONDS`. A dark streak on the ground straight under it shows where it really is.
/// A hand ahead of the real path is given back more slowly, so the spear never seems to slow
/// below 3/4 speed; one behind it is caught up within `SETTLE_SECONDS` however far behind it is,
/// so others' spears (which reach us late, already meters down their path) shoot out of the hand
/// fast and are where they really are almost at once. It leaves its wake behind it as it goes
/// (`Wake`). A thrown Q isn't the held javelin (that stays in the hand), nor is one an item threw
/// (a barrage's, a bounce): it shoots straight out from where the sim threw it, already pointing
/// the way it flies.
pub(super) fn fly_shots(
    mut commands: Commands,
    time: Res<Time>,
    clock: AttackClock,
    visuals: Res<Visuals>,
    mut holders: Query<(&PlayerId, &mut HeldAt)>,
    mut shots: Query<(&Projectile, &mut Transform, &mut Visibility, Option<&mut Thrown>, Option<&Spin>), With<Mesh3d>>,
) {
    let now = time.elapsed_secs();
    let tick = clock.now(true);
    for (projectile, mut transform, mut visibility, thrown, spin) in &mut shots {
        let speed = projectile.class.def().shot(projectile.ability).map_or(0.0, |s| s.speed);
        let height = shot_height(thrown.is_some());
        let at_tick = tick.max(projectile.spawn_tick as f32 - 1.0);
        let on_path = to_world(sim::projectile_pos(projectile, at_tick), height);
        let along = Quat::from_rotation_y(projectile.dir.to_angle());
        let forward_dir = along * Vec3::X;
        let Some(mut thrown) = thrown else {
            let spun = spin.map_or(Quat::IDENTITY, |s| Quat::from_rotation_x(s.0 * now));
            transform.set_if_neq(Transform::from_translation(on_path).with_rotation(along * spun));
            continue;
        };
        let launch = match thrown.launch {
            Some(launch) => launch,
            // Not yet let go of: the thrower's hand still swinging it (our own fighter is drawn a
            // moment behind the news of its throw). Out of sight until it is, or until it's late.
            None if projectile.ability.is_none()
                && !projectile.extra
                && tick < projectile.spawn_tick as f32 + MAX_HOLD_TICKS
                && holders.iter().any(|(id, held)| id.0 == projectile.owner && held.let_go.is_none_or(|at| at < projectile.spawn_tick)) =>
            {
                visibility.set_if_neq(Visibility::Hidden);
                continue;
            }
            None => {
                visibility.set_if_neq(Visibility::Inherited);
                // Where it starts, which way it points and how big it is: an ability's where the
                // sim threw it (just outside the thrower's body), along its flight; an
                // auto-attack just as the held javelin is this frame (its point, held pointing up,
                // +Y, a shot along +X, as big as the thrower draws it), the hand letting go of it.
                let start = if projectile.ability.is_some() || projectile.extra {
                    Some((to_world(projectile.origin, height), along, Vec3::ONE))
                } else {
                    holders.iter_mut().find(|(id, _)| id.0 == projectile.owner).map(|(_, mut held)| {
                        held.launched = held.launched.max(Some(projectile.spawn_tick));
                        // Our own: from just where the hand let go between frames, flown on
                        // along its path since (off it as far as the hand was then).
                        let (hand, path) = match held.released_from.filter(|_| held.let_go == Some(projectile.spawn_tick)) {
                            Some(hand) => (hand, to_world(sim::projectile_pos(projectile, projectile.spawn_tick as f32), height)),
                            None => (held.at, on_path),
                        };
                        let leaving = leaving_hand(&hand);
                        (leaving.translation + on_path - path, leaving.rotation, leaving.scale)
                    })
                };
                *thrown.launch.insert(match start {
                    Some((from, rotation, scale)) => {
                        // No more than `MAX_CATCH_UP` behind its real path.
                        let mut offset = from - on_path;
                        let excess = offset.dot(forward_dir) + MAX_CATCH_UP;
                        offset -= forward_dir * excess.min(0.0);
                        Launch { at: now, from, offset, held: rotation, scale }
                    }
                    // No thrower to be seen: straight from its path.
                    None => Launch { at: now, from: on_path, offset: Vec3::ZERO, held: along, scale: Vec3::ONE },
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
        let give_back = SETTLE_SECONDS.max(8.0 * ahead / speed.max(1.0));
        // Across and down onto its path over `CONVERGE_METERS` of flight, on a parabola: leaving
        // the hand heading in, meeting the path along it; the nose along the curve.
        let off = across + Vec3::Y * rise;
        let flown = (on_path - (launch.from - launch.offset)).dot(forward_dir).max(0.0);
        let rest = (1.0 - flown / CONVERGE_METERS).max(0.0);
        let at = on_path + off * rest * rest + forward_dir * ahead * left(give_back);
        let heading = (forward_dir - off * 2.0 * rest / CONVERGE_METERS).normalize();
        let flying = Quat::from_rotation_arc(forward_dir, heading) * along;
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

/// A piece of a spear's impact (`strike_home`), from `started`: the flash, or a splinter flying
/// at `velocity`.
#[derive(Component)]
pub(super) struct Impact {
    started: f32,
    life: f32,
    size: f32,
    velocity: Option<Vec3>,
}

/// A thrown spear gone short of its range hit something (a fighter, a wall, a rock, a tree): a
/// flash where its point was, and splinters thrown back the way it came, out and up. One that flew
/// its whole range just ends, and so does every one cleared away mid-flight: at the end of a won
/// match, or with the map changed.
#[allow(clippy::too_many_arguments)]
pub(super) fn strike_home(
    trigger: On<Remove, Projectile>,
    mut commands: Commands,
    time: Res<Time>,
    room: Res<crate::rooms::CurrentRoom>,
    map: Res<crate::rooms::CurrentMap>,
    mut visuals: ResMut<Visuals>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    shots: Query<(&Projectile, &Transform, &Thrown, &InheritedVisibility)>,
) {
    let cleared = room.0.as_ref().is_some_and(|view| view.winner.is_some()) || map.is_changed();
    let Ok((projectile, transform, thrown, visible)) = shots.get(trigger.entity) else { return };
    if cleared {
        return;
    }
    let Some(range) = projectile.class.def().shot(projectile.ability).map(|shot| shot.range) else { return };
    let point = transform.translation;
    let flown = arena::to_gameplay(point).distance(projectile.origin);
    if thrown.launch.is_none() || !visible.get() || flown > range - STOPPED_SHORT {
        return;
    }
    let now = time.elapsed_secs();
    let light = visuals.material(&mut materials, projectile.owner, Look::Spark);
    let bit = || (Mesh3d(visuals.impact.clone()), MeshMaterial3d(light.clone()), Transform::from_translation(point).with_scale(Vec3::ZERO), NotShadowCaster);
    commands.spawn((Impact { started: now, life: FLASH_SECONDS, size: FLASH_SIZE, velocity: None }, bit()));
    let back = -(transform.rotation * Vec3::X).with_y(0.0).normalize_or(Vec3::X);
    let across = Vec3::new(-back.z, 0.0, back.x);
    let seed = trigger.entity.index().index();
    for k in 0..SPLINTERS {
        let r = |i: u32| crate::swish::noise(seed, k * 8 + i);
        let speed = SPLINTER_SPEED.0 + (SPLINTER_SPEED.1 - SPLINTER_SPEED.0) * r(0);
        let velocity = (back + across * (r(1) * 2.0 - 1.0) * 1.1).normalize() * speed + Vec3::Y * SPLINTER_RISE * (0.4 + r(2));
        let size = SPLINTER_SIZE.0 + (SPLINTER_SIZE.1 - SPLINTER_SIZE.0) * r(3);
        let life = SPLINTER_LIFE.0 + (SPLINTER_LIFE.1 - SPLINTER_LIFE.0) * r(4);
        commands.spawn((Impact { started: now, life, size, velocity: Some(velocity) }, bit()));
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
