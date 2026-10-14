//! Shots in flight: where each is drawn every frame (`fly_shots`), and a thrown spear's hand-off
//! from the hand, its curve onto its real path and its rubbery wind.

use super::*;

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
/// Where a thrown spear's wind starts (just behind its butt) and how long it gets.
pub(super) const WIND_FRONT: f32 = arena::SPEAR_LENGTH;
const WIND_LENGTH: f32 = 2.4;
/// The wind's rubber: its spring (stiffness rad/s, damping; loose, so it overshoots), and how
/// much and how often (per second) it pulses.
const RUBBER_STIFFNESS: f32 = 14.0;
const RUBBER_DAMPING: f32 = 0.3;
const RUBBER_PULSE: f32 = 0.18;
const RUBBER_RATE: f32 = 3.5;

/// A thrown spear as drawn: it leaves the thrower's hand where the javelin was held, then eases
/// onto its real path (`fly_shots`), trailing its wind (a child entity).
#[derive(Component)]
pub(super) struct Thrown {
    pub(super) wind: Entity,
    pub(super) launch: Option<Launch>,
    /// The wind's length, on a loose spring.
    pub(super) length: crate::spring::Spring,
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
/// fast and are where they really are almost at once. Its wind stretches back to where it left
/// the hand, up to `WIND_LENGTH`. A thrown Q isn't the held javelin (that stays in the hand): it
/// shoots straight out from where the sim threw it, already pointing the way it flies.
pub(super) fn fly_shots(
    time: Res<Time>,
    clock: AttackClock,
    mut holders: Query<(&PlayerId, &mut HeldAt)>,
    mut shots: Query<(&Projectile, &mut Transform, &mut Visibility, Option<&mut Thrown>, Option<&Spin>), With<Mesh3d>>,
    mut winds: Query<&mut Transform, Without<Projectile>>,
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
                let start = if projectile.ability.is_some() {
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
        // The wind, rubbery like a sprint's: its length chases how far the spear has flown (up to
        // `WIND_LENGTH`), pulsing, on a loose spring, so it stretches out past that and snaps back.
        let flown = (at.distance(launch.from) - WIND_FRONT).clamp(0.0, WIND_LENGTH);
        let wanted = flown * (1.0 + RUBBER_PULSE * (since * RUBBER_RATE * std::f32::consts::TAU).sin());
        let dt = time.delta_secs();
        let length = thrown.length.follow(wanted, dt, RUBBER_STIFFNESS, RUBBER_DAMPING, f32::INFINITY).max(0.0);
        if let Ok(mut wind) = winds.get_mut(thrown.wind) {
            // In meters: undoing the spear's own drawn size (`Launch::scale`).
            let scale = Vec3::new(length / launch.scale.x, 1.0, 1.0);
            if wind.scale != scale {
                wind.scale = scale;
            }
        }
    }
}
