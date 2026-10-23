//! Each effect's little scene (`Fx`): which parts it's made of, where, and how they live. Timings
//! are seconds; an effect that lasts (a zone, a warning) lasts as long as the server says.

use std::f32::consts::{FRAC_PI_2, TAU};

use super::*;

use crate::arena::to_world;
use crate::swish::noise;

/// Effects as they arrive: each made, timed from when it really began (the news of it can come a
/// little late).
pub(super) fn play_new(
    mut commands: Commands,
    time: Res<Time>,
    clock: AttackClock,
    art: Res<FxArt>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut sounds: ResMut<crate::audio::SoundQueue>,
    me: Query<&Pos, With<crate::Me>>,
    new: Query<&Fx, Added<Fx>>,
) {
    let now = fx_now(&clock);
    for fx in &new {
        sound_of(fx, &mut sounds, &me, now);
        let late = secs((now - fx.from as f32).max(0.0));
        let start = time.elapsed_secs() - late;
        let seed = fx.from.wrapping_mul(2_654_435_761) ^ fx.at.x.to_bits() ^ fx.at.y.to_bits().rotate_left(16);
        let scene = Scene { start, seed, at: to_world(fx.at, 0.0), to: to_world(fx.to, 0.0), radius: fx.radius, lasts: secs(fx.until.saturating_sub(fx.from) as f32) };
        let c = &mut commands;
        match fx.kind {
            FxKind::Rime => rime(c, &art, &scene),
            FxKind::Echo => echo_burst(c, &art, &scene),
            FxKind::Meteor => meteor(c, &art, &scene),
            FxKind::Blink => blink(c, &art, &scene),
            FxKind::Zone(FxColor::Frost) => blizzard(c, &art, &scene),
            FxKind::Zone(FxColor::Void) => vortex(c, &art, &scene),
            FxKind::Zone(color) => glow_zone(c, &art, &scene, color),
            FxKind::Blast(FxColor::Fire) => fire_blast(c, &art, &scene),
            FxKind::Blast(FxColor::Thorn) => thorn_burst(c, &art, &scene),
            FxKind::Blast(FxColor::Void) => void_toll(c, &art, &scene),
            FxKind::Blast(FxColor::Storm) => thunder_strike(c, &art, &mut meshes, &scene),
            FxKind::Blast(FxColor::Poison) => splatter(c, &art, &scene, Ink::Poison, Ink::PoisonLight),
            FxKind::Blast(FxColor::Blood) => splatter(c, &art, &scene, Ink::Blood, Ink::Blood),
            FxKind::Blast(FxColor::Frost) => frost_shatter(c, &art, &scene),
            FxKind::Blast(color) => spirit_burst(c, &art, &scene, color),
            FxKind::Doom => doom_mark(c, &art, &scene),
            FxKind::Warn(FxColor::Spirit) => echo(c, &art, &scene),
            FxKind::Warn(color) => warning(c, &art, &scene, color),
            FxKind::Bolt => gathering_storm(c, &art, &scene),
            FxKind::Arc => arc(c, &art, &mut meshes, &scene),
            FxKind::Lob(color) => lob(c, &art, &scene, color),
            FxKind::Coins => coins(c, &art, &scene),
            FxKind::Rise => rise(c, &art, &scene),
            FxKind::Mend => mend(c, &art, &scene),
            FxKind::Wallop => wallop(c, &art, &scene),
            FxKind::Ignite => ignite(c, &art, &scene),
        }
    }
}

/// A burning fighter igniting: the fire on it going up all at once, a flash of flame racing out
/// over the ground, tongues of it flung out and up all round, embers thrown, a scorch where it
/// stood and a puff of smoke after. Smaller and quicker than a blast: a flare, not a bomb.
fn ignite(c: &mut Commands, art: &FxArt, s: &Scene) {
    let r = s.radius;
    put(c, art, s.pick(1, &art.splotch), Part::new(s.start, [0.05, 0.6, 1.4], Ink::Scorch, ground(s.at, 0.02)).size(r * 0.6).facing(s.r(2) * TAU).grow(0.3));
    put(c, art, &art.band, Part::new(s.start, [0.1, 0.1, 0.32], Ink::Flame, ground(s.at, 0.06)).sized(Vec3::new(r, 1.0, r)).grow(0.15).shrink(1.1));
    put(c, art, &art.glint, Part::new(s.start, [0.02, 0.06, 0.2], Ink::Spark, s.at + Vec3::Y * 0.9).size(r * 0.9).facing(s.r(3) * TAU).grow(0.3).shrink(0.5));
    let tongues = 8;
    for i in 0..tongues {
        let k = 20 + i * 4;
        let angle = (i as f32 + s.r(k)) / tongues as f32 * TAU;
        let lick = Quat::from_rotation_y(angle) * Quat::from_rotation_z(0.9 + 0.5 * s.r(k + 1));
        let length = r * (0.5 + 0.4 * s.r(k + 2));
        put(c, art, &art.tongue, Part::new(s.start + 0.02 * s.r(k + 3), [0.06, 0.14, 0.42], Ink::Flame, s.at + Vec3::Y * 0.5).sized(Vec3::new(length, 1.0, length * 0.7)).turn(lick).grow(0.2).shrink(0.3).flicker(0.3));
    }
    for i in 0..10 {
        let k = 100 + i * 6;
        let velocity = s.out(k) * (1.0 + 2.0 * s.r(k + 1)) + Vec3::Y * (2.5 + 2.5 * s.r(k + 2));
        let part = Part::new(s.start, [0.02, 0.5, 0.9 + 0.4 * s.r(k + 3)], Ink::Spark, s.at + Vec3::Y * 0.8).size(0.04 + 0.04 * s.r(k + 4));
        put(c, art, &art.mote, part.motion(Motion::Thrown { velocity, gravity: 5.0, tumble: Vec3::ZERO }).flicker(0.5).shrink(0.3));
    }
    let smoke = Part::new(s.start + 0.1, [0.25, 0.5, 1.3], Ink::Smoke, s.at + Vec3::Y * 0.9).size(r * 0.5);
    put(c, art, s.pick(7, &art.splotch), smoke.motion(Motion::Thrown { velocity: Vec3::Y * 0.7, gravity: -0.2, tumble: Vec3::ZERO }).grow(0.4).shrink(1.6).spin(0.5));
}

/// The warden's shield driving in: the air shoved ahead of it as a fan of dust streaking out
/// low along the thrust, and grit kicked forward off the ground under it. Where the thrust went,
/// from `at` along `dir`.
pub(super) fn shield_push(c: &mut Commands, art: &FxArt, now: f32, at: Vec2, dir: Vec2, seed: u32) {
    let s = Scene { start: now, seed, at: to_world(at + dir * 0.9, 0.0), to: to_world(at + dir * 1.7, 0.0), radius: 0.5, lasts: 0.3 };
    let forward = (s.to - s.at).normalize_or(Vec3::X);
    let across = Vec3::new(-forward.z, 0.0, forward.x);
    // The tongue lies along +X: turned about the up to point the way the shield went. A fan of
    // three, the middle one longest, each racing on ahead as it thins out.
    let yaw = (-forward.z).atan2(forward.x);
    for (i, spread) in [-0.4f32, 0.0, 0.4].into_iter().enumerate() {
        let k = 60 + i as u32 * 4;
        let length = (0.9 - 0.3 * spread.abs()) * (0.85 + 0.3 * s.r(k));
        let streak = Motion::Thrown { velocity: forward * (2.5 + 1.5 * s.r(k + 1)), gravity: 0.0, tumble: Vec3::ZERO };
        let part = Part::new(s.start + 0.01 * i as f32, [0.04, 0.1, 0.32], Ink::Dust, ground(s.at + across * spread * 0.35, 0.05)).sized(Vec3::new(length, 1.0, 0.32));
        put(c, art, &art.tongue, part.facing(yaw + spread).grow(0.3).shrink(1.5).flicker(0.2).motion(streak));
    }
    for i in 0..7 {
        let k = 20 + i * 6;
        let velocity = forward * (1.5 + 2.5 * s.r(k)) + across * (s.r(k + 1) - 0.5) * 1.6 + Vec3::Y * (0.6 + 1.4 * s.r(k + 2));
        let tumble = Vec3::new(s.r(k + 3) - 0.5, s.r(k + 4) - 0.5, s.r(k + 5) - 0.5) * 20.0;
        let part = Part::new(s.start, [0.01, 0.3, 0.5], Ink::Earth, s.at + across * (s.r(k + 6) - 0.5) * 0.8 + Vec3::Y * 0.1).size(0.03 + 0.04 * s.r(k + 1));
        put(c, art, &art.chunk, part.motion(Motion::Thrown { velocity, gravity: 11.0, tumble }).shrink(0.0));
    }
}

/// Someone slammed into a wall: a flash of the hit, a cloud of dust knocked off the stone
/// welling up and hanging, grit and chips knocked off it and falling, and, for as long as they're
/// stunned there (`lasts`), motes of dust reeling round their head.
fn wallop(c: &mut Commands, art: &FxArt, s: &Scene) {
    let r = s.radius;
    let dazed = s.lasts.max(0.3);
    put(c, art, &art.glint, Part::new(s.start, [0.0, 0.1, 0.3], Ink::Glint, s.at + Vec3::Y * 0.9).size(r * 1.3).facing(s.r(1) * TAU).shrink(0.4));
    for i in 0..3u32 {
        let k = 10 + i * 4;
        let puff = Motion::Thrown { velocity: s.out(k) * 0.5 + Vec3::Y * (0.6 + 0.4 * s.r(k + 1)), gravity: -0.25, tumble: Vec3::ZERO };
        let part = Part::new(s.start + 0.03 * i as f32, [0.08, 0.5, 1.3], Ink::Dust, s.at + Vec3::Y * (0.3 + 0.3 * i as f32)).size(r * (0.9 + 0.3 * s.r(k + 2)));
        put(c, art, s.pick(k + 3, &art.splotch), part.grow(0.3).shrink(1.8).spin(0.6).motion(puff));
    }
    for i in 0..11 {
        let k = 30 + i * 6;
        let velocity = s.out(k) * (1.2 + 2.0 * s.r(k + 1)) + Vec3::Y * (1.2 + 2.4 * s.r(k + 2));
        let tumble = Vec3::new(s.r(k + 3) - 0.5, s.r(k + 4) - 0.5, s.r(k + 5) - 0.5) * 24.0;
        let part = Part::new(s.start, [0.01, 0.5, 0.8], Ink::Earth, s.at + Vec3::Y * (0.4 + 0.7 * s.r(k + 6))).size(0.04 + 0.06 * s.r(k + 1));
        put(c, art, &art.chunk, part.motion(Motion::Thrown { velocity, gravity: 12.0, tumble }).shrink(0.0));
    }
    // Seeing stars: dust reeling round the head while the stun lasts.
    for i in 0..5 {
        let motion = Motion::Orbit { center: s.at, angle: i as f32 / 5.0 * TAU, speed: 7.0, radius: (0.42, 0.3), height: (1.55, 1.7) };
        put(c, art, &art.mote, Part::new(s.start + 0.05, [0.1, dazed - 0.1, dazed], Ink::Dust, s.at).size(0.09).motion(motion).flicker(0.25));
    }
}

/// The breaker's maul coming down: the ground breaking under the blow all along its lane, dark
/// cracks spreading from where the head lands, earth thrown up and tumbling back, and a low
/// pall of dust rolling out from it.
pub(super) fn tremor(c: &mut Commands, art: &FxArt, now: f32, at: Vec2, dir: Vec2, seed: u32) {
    let s = Scene { start: now, seed, at: to_world(at + dir * 1.3, 0.0), to: to_world(at + dir * 2.0, 0.0), radius: 0.9, lasts: 0.5 };
    let r = s.radius;
    put(c, art, s.pick(1, &art.cracks), Part::new(s.start, [0.04, 1.0, 1.8], Ink::Scorch, ground(s.at, 0.025)).sized(Vec3::new(r * 1.3, 1.0, r * 1.3)).facing(s.r(2) * TAU).grow(0.2));
    put(c, art, s.pick(3, &art.streaks), Part::new(s.start, [0.05, 0.8, 1.5], Ink::Scorch, ground(s.at, 0.022)).sized(Vec3::new(r * 1.1, 1.0, r * 1.1)).facing(s.r(4) * TAU).grow(0.3));
    // Dust bursting out from under the head: a ragged splotch, not a ring.
    put(c, art, s.pick(5, &art.splotch), Part::new(s.start, [0.08, 0.15, 0.55], Ink::Dust, ground(s.at, 0.05)).size(r * 0.6).facing(s.r(6) * TAU).grow(0.2).shrink(2.4));
    clods(c, art, &s, 10, 2.2, Ink::Earth);
    for i in 0..3u32 {
        let k = 80 + i * 5;
        let roll = Motion::Thrown { velocity: s.out(k) * 0.8 + Vec3::Y * 0.3, gravity: -0.15, tumble: Vec3::ZERO };
        let part = Part::new(s.start + 0.05 * i as f32, [0.12, 0.4, 1.1], Ink::Dust, s.spot(k + 1, 0.6) + Vec3::Y * 0.15).size(r * 0.7);
        put(c, art, s.pick(k + 2, &art.splotch), part.grow(0.3).shrink(1.9).spin(0.5).motion(roll));
    }
}

/// The pyre witch's ring of cinders: the ground igniting all round her. A flash of flame races
/// out to the edge, where a ring of tongues leaps up as it arrives and burns, leaning out, with a
/// few more licking up over the ground inside; the ground is scorched, its cracks glowing and
/// cooling; embers fly up off it, and smoke wells up round the edge after.
pub(super) fn cinder_ring(c: &mut Commands, art: &FxArt, now: f32, at: Vec2, radius: f32, seed: u32) {
    let s = Scene { start: now, seed, at: to_world(at, 0.0), to: to_world(at, 0.0), radius, lasts: 0.8 };
    let r = s.radius;
    put(c, art, s.pick(1, &art.splotch), Part::new(s.start, [0.1, 1.4, 2.6], Ink::Scorch, ground(s.at, 0.02)).sized(Vec3::new(r * 0.95, 1.0, r * 0.95)).facing(s.r(2) * TAU).grow(0.25));
    put(c, art, s.pick(3, &art.cracks), Part::new(s.start, [0.08, 0.45, 1.6], Ink::Ember, ground(s.at, 0.03)).sized(Vec3::new(r * 0.9, 1.0, r * 0.9)).facing(s.r(4) * TAU).grow(0.2).shrink(0.85));
    // The flash racing out to the edge.
    let flash = 0.14;
    put(c, art, &art.band, Part::new(s.start, [flash, flash, 0.42], Ink::Flame, ground(s.at, 0.06)).sized(Vec3::new(r, 1.0, r)).grow(0.1).shrink(1.1));
    // The tongue lies along +X: stood up, leaning out, turned to its spot on the ring.
    for (count, out, tall) in [(14u32, 0.88f32, 0.8f32), (6, 0.45, 0.55)] {
        for i in 0..count {
            let k = 20 + i * 5 + (out * 100.0) as u32;
            let angle = (i as f32 + 0.5 * s.r(k)) / count as f32 * TAU;
            let at = s.at + Vec3::new(angle.cos(), 0.0, -angle.sin()) * r * out + Vec3::Y * 0.05;
            let lick = Quat::from_rotation_y(angle) * Quat::from_rotation_z(FRAC_PI_2 - 0.35 - 0.3 * s.r(k + 1));
            let length = tall * (0.7 + 0.5 * s.r(k + 2));
            let born = s.start + flash * out + 0.04 * s.r(k + 3);
            let part = Part::new(born, [0.06, 0.25 + 0.2 * s.r(k + 4), 0.6 + 0.25 * s.r(k + 4)], Ink::Flame, at).sized(Vec3::new(length, 1.0, length * 0.6));
            put(c, art, &art.tongue, part.turn(lick).grow(0.2).shrink(0.3).flicker(0.3));
        }
    }
    // Embers flying up off the burning ground and drifting down.
    for i in 0..16 {
        let k = 300 + i * 6;
        let velocity = s.out(k) * (0.5 + 1.5 * s.r(k + 1)) + Vec3::Y * (2.0 + 2.5 * s.r(k + 2));
        let part = Part::new(s.start + flash * 0.5 * s.r(k + 5), [0.02, 0.6, 1.0 + 0.6 * s.r(k + 3)], Ink::Spark, s.spot(k + 6, 0.9) + Vec3::Y * 0.2).size(0.04 + 0.04 * s.r(k + 4));
        put(c, art, &art.mote, part.motion(Motion::Thrown { velocity, gravity: 4.0, tumble: Vec3::ZERO }).flicker(0.5).shrink(0.3));
    }
    // Smoke welling up round the edge after.
    for i in 0..5 {
        let k = 400 + i * 6;
        let angle = (i as f32 + s.r(k)) / 5.0 * TAU;
        let at = s.at + Vec3::new(angle.cos(), 0.0, -angle.sin()) * r * 0.8 + Vec3::Y * 0.3;
        let velocity = s.out(k + 1) * 0.3 + Vec3::Y * (0.5 + 0.4 * s.r(k + 2));
        let part = Part::new(s.start + 0.25 + 0.15 * s.r(k + 3), [0.3, 0.6, 1.5], Ink::Smoke, at).size(r * 0.3);
        put(c, art, s.pick(k + 4, &art.splotch), part.motion(Motion::Thrown { velocity, gravity: -0.2, tumble: Vec3::ZERO }).grow(0.4).shrink(1.6).spin(0.6));
    }
}

/// A quake (the breaker's, and the warden's ground slam): the ground heaving under a blow at the
/// caster's feet. Dust bursts up from under it and a shockwave of it races out through the
/// ground, slabs of earth heaving up and dropping back as it passes them, cracks spreading out
/// from the middle, earth thrown up and tumbling back, and a low pall of dust rolling out after.
pub(super) fn quake(c: &mut Commands, art: &FxArt, now: f32, at: Vec2, radius: f32, seed: u32) {
    let s = Scene { start: now, seed, at: to_world(at, 0.0), to: to_world(at, 0.0), radius, lasts: 0.7 };
    let r = s.radius;
    put(c, art, s.pick(1, &art.cracks), Part::new(s.start, [0.1, 1.2, 2.2], Ink::Scorch, ground(s.at, 0.025)).sized(Vec3::new(r * 1.05, 1.0, r * 1.05)).facing(s.r(2) * TAU).grow(0.15));
    put(c, art, s.pick(3, &art.streaks), Part::new(s.start, [0.12, 0.9, 1.8], Ink::Scorch, ground(s.at, 0.022)).sized(Vec3::new(r * 0.9, 1.0, r * 0.9)).facing(s.r(4) * TAU).grow(0.2));
    // Dust bursting up from under the blow: thin enough to see the ground break through it.
    put(c, art, s.pick(5, &art.splotch), Part::new(s.start, [0.08, 0.15, 0.55], Ink::Dust, ground(s.at, 0.05)).size(r * 0.28).facing(s.r(6) * TAU).grow(0.2).shrink(1.9));
    // The shockwave racing out through the ground.
    let wave = 0.18;
    put(c, art, &art.band, Part::new(s.start, [wave, wave, 0.5], Ink::Dust, ground(s.at, 0.06)).sized(Vec3::new(r, 1.0, r)).grow(0.1).shrink(1.15));
    // Slabs of earth heaved up as the wave passes, dropping back where they lay.
    for (count, out) in [(8u32, 0.5f32), (14, 0.9)] {
        for i in 0..count {
            let k = 200 + i * 8 + (out * 50.0) as u32;
            let angle = (i as f32 + 0.6 * s.r(k)) / count as f32 * TAU;
            let size = Vec3::new(0.3 + 0.25 * s.r(k + 1), 0.12 + 0.1 * s.r(k + 2), 0.25 + 0.2 * s.r(k + 3));
            let at = s.at + Vec3::new(angle.cos(), 0.0, -angle.sin()) * r * out + Vec3::Y * (size.y / 2.0 + 0.01);
            let heave = Motion::Thrown { velocity: Vec3::Y * (1.2 + 0.8 * s.r(k + 4)), gravity: 9.0, tumble: Vec3::new(s.r(k + 5) - 0.5, 0.0, s.r(k + 6) - 0.5) * 3.0 };
            let born = s.start + wave * out * 0.9 + 0.03 * s.r(k + 7);
            put(c, art, &art.chunk, Part::new(born, [0.03, 0.45, 0.8], Ink::Earth, at).sized(size).facing(angle).motion(heave).shrink(0.0));
        }
    }
    clods(c, art, &s, 12, 2.4, Ink::Earth);
    // A few puffs of dust rolling out low after the wave, between the slabs, not over them.
    for i in 0..4u32 {
        let k = 80 + i * 5;
        let roll = Motion::Thrown { velocity: s.out(k) * 1.1 + Vec3::Y * 0.25, gravity: -0.12, tumble: Vec3::ZERO };
        let part = Part::new(s.start + 0.08 + 0.06 * i as f32, [0.12, 0.3, 0.85], Ink::Dust, s.spot(k + 1, 0.75) + Vec3::Y * 0.12).size(r * 0.22);
        put(c, art, s.pick(k + 2, &art.splotch), part.grow(0.3).shrink(1.6).spin(0.5).motion(roll));
    }
}

/// The sound an effect makes, if any: loud up close, faint far off; what lands later (a meteor,
/// a lob, a bolt gathering) sounds as it lands, not as the warning shows.
fn sound_of(fx: &Fx, sounds: &mut crate::audio::SoundQueue, me: &Query<&Pos, With<crate::Me>>, now: f32) {
    use crate::audio::{Sfx, earshot};
    let Ok(me) = me.single() else { return };
    let lands = secs((fx.until as f32 - now).max(0.0));
    let (sfx, reach, delay) = match fx.kind {
        FxKind::Blast(FxColor::Fire) => (Sfx::Explosion, 34.0, 0.0),
        FxKind::Meteor => (Sfx::Explosion, 44.0, lands),
        FxKind::Blast(FxColor::Storm) => (Sfx::Thunder, 44.0, 0.0),
        FxKind::Blast(FxColor::Frost) => (Sfx::Shatter, 24.0, 0.0),
        FxKind::Bolt => (Sfx::Thunder, 44.0, lands),
        FxKind::Coins => (Sfx::Coin, 16.0, 0.0),
        FxKind::Wallop => (Sfx::Wallop, 22.0, 0.0),
        FxKind::Ignite => (Sfx::Ignite, 26.0, 0.0),
        FxKind::Arc => (Sfx::Arc, 20.0, 0.0),
        _ => return,
    };
    sounds.push_in(sfx, earshot(fx.at.distance(me.0), reach), delay);
}

/// Where and when an effect plays: from `start` (seconds), at `at` (and toward `to`, in the
/// world), `radius` round, `lasts` seconds; broken by `seed`.
#[derive(Clone, Copy)]
struct Scene {
    start: f32,
    seed: u32,
    at: Vec3,
    to: Vec3,
    radius: f32,
    lasts: f32,
}

impl Scene {
    /// A pseudo-random number in 0..1, the `k`th of this effect.
    fn r(&self, k: u32) -> f32 {
        noise(self.seed, k)
    }

    /// One of `pick`, by this effect's `k`th number.
    fn pick<'a, T>(&self, k: u32, pick: &'a [T]) -> &'a T {
        &pick[(self.r(k) * pick.len() as f32) as usize % pick.len()]
    }

    /// A spot `out` (0 to 1 of the radius, at most) from the middle, at its own angle (the `k`th).
    fn spot(&self, k: u32, out: f32) -> Vec3 {
        let angle = self.r(k) * TAU;
        let r = self.radius * out * self.r(k + 1).sqrt();
        self.at + Vec3::new(angle.cos() * r, 0.0, -angle.sin() * r)
    }

    /// The `k`th way out from the middle, flat.
    fn out(&self, k: u32) -> Vec3 {
        let angle = self.r(k) * TAU;
        Vec3::new(angle.cos(), 0.0, -angle.sin())
    }
}

/// Flat on the ground, this high over it (layers a hair apart).
fn ground(at: Vec3, height: f32) -> Vec3 {
    at.with_y(height)
}

/// Earth thrown up from round `at`: `count` clods flying out and up, tumbling, falling back.
fn clods(c: &mut Commands, art: &FxArt, s: &Scene, count: u32, throw: f32, ink: Ink) {
    for i in 0..count {
        let k = 500 + i * 8;
        let out = s.out(k);
        let size = 0.05 + 0.08 * s.r(k + 1) * s.r(k + 1);
        let velocity = out * throw * (0.4 + s.r(k + 2)) + Vec3::Y * throw * (1.2 + s.r(k + 3));
        let tumble = Vec3::new(s.r(k + 4) - 0.5, s.r(k + 5) - 0.5, s.r(k + 6) - 0.5) * 25.0;
        let at = s.spot(k + 7, 0.5) + Vec3::Y * 0.1;
        let life = [0.02, 0.7, 1.0];
        put(c, art, &art.chunk, Part::new(s.start, life, ink, at).size(size).motion(Motion::Thrown { velocity, gravity: 14.0, tumble }).shrink(0.0));
    }
}

/// Motes rising off round `at`: `count` of them, drifting up and out, flickering, in `ink`, over
/// `life` seconds (starting at random through `spread` seconds).
fn motes(c: &mut Commands, art: &FxArt, s: &Scene, count: u32, ink: Ink, size: f32, rise: f32, life: f32, spread: f32) {
    for i in 0..count {
        let k = 700 + i * 8;
        let born = s.start + spread * s.r(k);
        let at = s.spot(k + 1, 0.9) + Vec3::Y * (0.05 + 0.3 * s.r(k + 3));
        let velocity = s.out(k + 4) * 0.4 + Vec3::Y * rise * (0.6 + 0.8 * s.r(k + 5));
        let part = Part::new(born, [0.08, life * 0.5, life * (0.7 + 0.3 * s.r(k + 6))], ink, at).size(size * (0.5 + s.r(k + 7)));
        put(c, art, &art.mote, part.motion(Motion::Thrown { velocity, gravity: -0.4, tumble: Vec3::ZERO }).flicker(0.4).shrink(0.2));
    }
}

/// Hoarfrost: rime grows over the ground where you stepped. A pale glaze spreads first, then ice
/// crystals branch out over it, a few shards crack up out of it, and light glints off it here and
/// there; then it thaws, the crystals shrinking back as it fades.
fn rime(c: &mut Commands, art: &FxArt, s: &Scene) {
    let r = s.radius;
    let thaw = s.lasts.max(1.0);
    let life = [0.25, thaw - 0.8, thaw];
    put(c, art, s.pick(1, &art.splotch), Part::new(s.start, life, Ink::Glaze, ground(s.at, 0.02)).sized(Vec3::new(r * 1.1, 1.0, r * 1.1)).facing(s.r(2) * TAU).grow(0.4));
    for layer in 0..2u32 {
        let size = r * (1.05 - 0.4 * layer as f32);
        let part = Part::new(s.start + 0.05 * layer as f32, [0.4, thaw - 0.9, thaw], Ink::Frost, ground(s.at, 0.03 + 0.003 * layer as f32));
        put(c, art, s.pick(3 + layer, &art.frost), part.sized(Vec3::new(size, 1.0, size)).facing(s.r(5 + layer) * TAU).grow(0.15).shrink(0.7));
    }
    // Shards cracking up out of it, leaning out, melting away at the end.
    for i in 0..3 {
        let k = 40 + i * 6;
        let at = s.spot(k, 0.65);
        let out = (at - s.at).with_y(0.0);
        let angle = if out.length() > 1e-3 { out.z.atan2(-out.x) + std::f32::consts::PI } else { s.r(k + 2) * TAU };
        let tall = 0.18 + 0.2 * s.r(k + 3);
        let part = Part::new(s.start + 0.05 + 0.1 * s.r(k + 4), [0.12, thaw - 0.7, thaw - 0.2], Ink::Frost, at);
        put(c, art, &art.shard, part.size(tall).facing(angle).grow(0.0).shrink(0.0));
    }
    // Light catching on it.
    for i in 0..5 {
        let k = 80 + i * 4;
        let born = s.start + (thaw - 0.6) * s.r(k);
        let part = Part::new(born, [0.12, 0.2, 0.45], Ink::Glint, s.spot(k + 1, 0.8) + Vec3::Y * 0.05);
        put(c, art, &art.glint, part.size(0.22 + 0.12 * s.r(k + 2)).facing(s.r(k + 3) * TAU).grow(0.2).shrink(0.2).spin(2.0));
    }
}

/// Heart of Winter's blizzard: frost thick over the ground, snow whirling round in the wind, gusts
/// streaking round, a cold mist.
fn blizzard(c: &mut Commands, art: &FxArt, s: &Scene) {
    let (r, lasts) = (s.radius, s.lasts.max(1.0));
    put(c, art, s.pick(1, &art.splotch), Part::new(s.start, [0.4, lasts - 0.8, lasts], Ink::Glaze, ground(s.at, 0.02)).sized(Vec3::new(r * 1.15, 1.0, r * 1.15)).grow(0.3));
    for layer in 0..3u32 {
        let size = r * (1.1 - 0.3 * layer as f32);
        let part = Part::new(s.start + 0.08 * layer as f32, [0.5, lasts - 0.9, lasts], Ink::Frost, ground(s.at, 0.03 + 0.003 * layer as f32));
        put(c, art, s.pick(3 + layer, &art.frost), part.sized(Vec3::new(size, 1.0, size)).facing(s.r(6 + layer) * TAU).grow(0.2).shrink(0.8));
    }
    // Snow swept round in the wind.
    for i in 0..26 {
        let k = 200 + i * 6;
        let radius = r * (0.25 + 0.8 * s.r(k));
        let height = (0.2 + 1.6 * s.r(k + 1), 0.1 + 1.2 * s.r(k + 2));
        let motion = Motion::Orbit { center: s.at, angle: s.r(k + 3) * TAU, speed: 2.4 + 1.6 * s.r(k + 4), radius: (radius, radius * 0.85), height };
        put(c, art, &art.mote, Part::new(s.start + 0.3 * s.r(k + 5), [0.3, lasts - 0.6, lasts], Ink::Glint, s.at).size(0.05 + 0.04 * s.r(k + 5)).motion(motion).flicker(0.5));
    }
    // Gusts: streaks of wind racing round.
    for i in 0..5 {
        let k = 400 + i * 6;
        let motion = Motion::Orbit { center: s.at, angle: s.r(k) * TAU, speed: 3.2, radius: (r * (0.5 + 0.4 * s.r(k + 1)), r * 0.6), height: (0.4 + s.r(k + 2), 0.3) };
        let part = Part::new(s.start + 0.2 * i as f32, [0.3, lasts - 0.6, lasts], Ink::SpiritMist, s.at).sized(Vec3::new(1.2, 1.0, 0.5)).turn(Quat::from_rotation_y(FRAC_PI_2)).motion(motion);
        put(c, art, &art.tongue, part);
    }
}

/// The Void Orb's vortex: a hole torn in the ground, thin arms of void light whirling in to it
/// over darker ones turning the other way, motes of the void dragged in from all round.
fn vortex(c: &mut Commands, art: &FxArt, s: &Scene) {
    let (r, lasts) = (s.radius, s.lasts.max(0.6));
    let life = [0.3, lasts - 0.5, lasts];
    put(c, art, s.pick(1, &art.splotch), Part::new(s.start, life, Ink::Void, ground(s.at, 0.02)).size(r * 0.4).grow(0.2).shrink(0.2));
    put(c, art, s.pick(2, &art.whirl), Part::new(s.start, life, Ink::Void, ground(s.at, 0.03)).sized(Vec3::new(r, 1.0, r)).spin(-1.4).grow(0.3).shrink(0.3));
    put(c, art, s.pick(3, &art.whirl), Part::new(s.start, life, Ink::VoidMist, ground(s.at, 0.04)).sized(Vec3::new(r * 1.05, 1.0, r * 1.05)).spin(-3.2).grow(0.3).shrink(0.0));
    put(c, art, s.pick(4, &art.splotch), Part::new(s.start, life, Ink::VoidMist, ground(s.at, 0.025)).size(r).grow(0.5));
    // Dragged in from round about, over and over.
    for i in 0..18 {
        let k = 300 + i * 6;
        let born = s.start + lasts * 0.8 * s.r(k);
        let motion = Motion::Orbit { center: s.at, angle: s.r(k + 1) * TAU, speed: -4.0, radius: (r * (0.8 + 0.4 * s.r(k + 2)), 0.05), height: (0.3 + 0.7 * s.r(k + 3), 0.05) };
        put(c, art, &art.mote, Part::new(born, [0.1, 0.5, 0.75], Ink::VoidLight, s.at).size(0.06).motion(motion).flicker(0.3));
    }
}

/// Any other lasting ground: a soft mist of its color, breathing, motes rising.
fn glow_zone(c: &mut Commands, art: &FxArt, s: &Scene, color: FxColor) {
    let (ink, mist) = inks(color);
    let lasts = s.lasts.max(0.5);
    put(c, art, s.pick(1, &art.splotch), Part::new(s.start, [0.3, lasts - 0.5, lasts], mist, ground(s.at, 0.03)).size(s.radius).grow(0.2));
    motes(c, art, s, 10, ink, 0.07, 0.8, 1.0, lasts);
}

/// A blast of fire (Ember Heart, a keg, a volatile monster): the ground scorched
/// black, its cracks glowing and cooling, tongues of flame licking out and up, a ring of flame
/// racing out, embers flying up, earth thrown, smoke rising after.
fn fire_blast(c: &mut Commands, art: &FxArt, s: &Scene) {
    let r = s.radius;
    put(c, art, s.pick(1, &art.splotch), Part::new(s.start, [0.06, 2.0, 3.2], Ink::Scorch, ground(s.at, 0.02)).sized(Vec3::new(r * 0.85, 1.0, r * 0.85)).facing(s.r(2) * TAU).grow(0.3));
    put(c, art, s.pick(3, &art.streaks), Part::new(s.start, [0.1, 1.6, 2.8], Ink::Scorch, ground(s.at, 0.021)).sized(Vec3::new(r * 1.2, 1.0, r * 1.2)).facing(s.r(4) * TAU).grow(0.4));
    put(c, art, s.pick(5, &art.cracks), Part::new(s.start, [0.05, 0.35, 1.8], Ink::Ember, ground(s.at, 0.03)).sized(Vec3::new(r * 0.8, 1.0, r * 0.8)).facing(s.r(6) * TAU).grow(0.2).shrink(0.85));
    // The flash racing out.
    put(c, art, &art.band, Part::new(s.start, [0.12, 0.12, 0.4], Ink::Flame, ground(s.at, 0.06)).sized(Vec3::new(r, 1.0, r)).grow(0.2).shrink(1.15));
    // Tongues of flame licking out and up from the middle.
    let tongues = 9;
    for i in 0..tongues {
        let k = 20 + i * 4;
        let angle = (i as f32 + s.r(k)) / tongues as f32 * TAU;
        let lick = Quat::from_rotation_y(angle) * Quat::from_rotation_z(0.6 + 0.4 * s.r(k + 1));
        let length = r * (0.55 + 0.4 * s.r(k + 2));
        put(c, art, &art.tongue, Part::new(s.start + 0.03 * s.r(k + 3), [0.08, 0.15, 0.5], Ink::Flame, s.at + Vec3::Y * 0.1).sized(Vec3::new(length, 1.0, length * 0.8)).turn(lick).grow(0.2).shrink(0.4).flicker(0.25));
    }
    // Embers flying up and drifting down.
    for i in 0..14 {
        let k = 100 + i * 6;
        let velocity = s.out(k) * (1.0 + 2.0 * s.r(k + 1)) + Vec3::Y * (3.0 + 3.0 * s.r(k + 2));
        let part = Part::new(s.start, [0.02, 0.6, 1.0 + 0.6 * s.r(k + 3)], Ink::Spark, s.at + Vec3::Y * 0.3).size(0.05 + 0.04 * s.r(k + 4));
        put(c, art, &art.mote, part.motion(Motion::Thrown { velocity, gravity: 5.0, tumble: Vec3::ZERO }).flicker(0.5).shrink(0.3));
    }
    clods(c, art, s, 7, 1.6, Ink::Earth);
    // Smoke welling up after.
    for i in 0..4 {
        let k = 160 + i * 6;
        let velocity = s.out(k) * 0.3 + Vec3::Y * (0.6 + 0.4 * s.r(k + 1));
        let part = Part::new(s.start + 0.1 + 0.15 * s.r(k + 2), [0.3, 0.6, 1.6], Ink::Smoke, s.spot(k + 3, 0.5) + Vec3::Y * 0.3).size(r * 0.45);
        put(c, art, s.pick(k + 4, &art.splotch), part.motion(Motion::Thrown { velocity, gravity: -0.2, tumble: Vec3::ZERO }).grow(0.4).shrink(1.6).spin(0.6));
    }
}

/// Bramble Mantle's lash: pale thorns bursting up out of the ground all round, leaning out, and
/// sinking back; brambles creeping over the ground where they broke out, leaves and clods thrown.
fn thorn_burst(c: &mut Commands, art: &FxArt, s: &Scene) {
    let r = s.radius;
    put(c, art, s.pick(1, &art.streaks), Part::new(s.start, [0.1, 0.9, 1.5], Ink::Thorn, ground(s.at, 0.025)).sized(Vec3::new(r, 1.0, r)).facing(s.r(2) * TAU).grow(0.3));
    put(c, art, s.pick(3, &art.cracks), Part::new(s.start, [0.06, 0.5, 1.2], Ink::Scorch, ground(s.at, 0.02)).sized(Vec3::new(r * 0.7, 1.0, r * 0.7)).facing(s.r(4) * TAU).grow(0.3));
    for i in 0..16 {
        let k = 30 + i * 6;
        let at = s.spot(k, 0.95);
        let out = (at - s.at).with_y(0.0).normalize_or(Vec3::X);
        // Leaning out, away from you.
        let lean = Quat::from_rotation_arc(Vec3::Y, (Vec3::Y + out * (0.4 + 0.6 * s.r(k + 2))).normalize());
        let tall = 0.45 + 0.5 * s.r(k + 3);
        let part = Part::new(s.start + 0.06 * s.r(k + 4), [0.06, 0.35, 0.65], Ink::Bark, at).sized(Vec3::new(0.14, tall, 0.14)).turn(lean).grow(0.0).shrink(0.0);
        put(c, art, &art.thorn, part);
    }
    clods(c, art, s, 6, 1.4, Ink::Thorn);
}

/// A frostbolt shattering where it hits: a flash of cold, frost flung over the ground and
/// melting, splinters of ice thrown out and falling, a breath of cold mist.
fn frost_shatter(c: &mut Commands, art: &FxArt, s: &Scene) {
    let r = s.radius;
    put(c, art, &art.glint, Part::new(s.start, [0.02, 0.05, 0.2], Ink::Glint, s.at + Vec3::Y * 0.6).size(r * 0.9).facing(s.r(1) * TAU).grow(0.4).shrink(0.6));
    put(c, art, s.pick(2, &art.frost), Part::new(s.start, [0.06, 0.5, 1.2], Ink::Frost, ground(s.at, 0.03)).sized(Vec3::new(r * 0.85, 1.0, r * 0.85)).facing(s.r(3) * TAU).grow(0.25).shrink(0.8));
    put(c, art, s.pick(4, &art.splotch), Part::new(s.start, [0.05, 0.3, 0.9], Ink::Glaze, ground(s.at, 0.02)).size(r * 0.75).grow(0.4));
    for i in 0..7 {
        let k = 40 + i * 6;
        let velocity = s.out(k) * (2.0 + 2.5 * s.r(k + 1)) + Vec3::Y * (1.5 + 2.0 * s.r(k + 2));
        let tumble = Vec3::new(s.r(k + 3) - 0.5, s.r(k + 4) - 0.5, s.r(k + 5) - 0.5) * 30.0;
        let part = Part::new(s.start, [0.01, 0.35, 0.6], Ink::Frost, s.at + Vec3::Y * 0.7).size(0.05 + 0.06 * s.r(k + 1));
        put(c, art, &art.chunk, part.motion(Motion::Thrown { velocity, gravity: 12.0, tumble }).shrink(0.0));
    }
    motes(c, art, s, 5, Ink::Glint, 0.05, 0.6, 0.6, 0.15);
}

/// A spirit's burst (a decoy shattering, a comeback): a ripple of spirit light racing
/// out over the ground, a round glow of it under them swelling and fading, and wisps rising.
fn spirit_burst(c: &mut Commands, art: &FxArt, s: &Scene, color: FxColor) {
    let (ink, mist) = inks(color);
    let r = s.radius;
    put(c, art, &art.band, Part::new(s.start, [0.18, 0.2, 0.55], ink, ground(s.at, 0.06)).sized(Vec3::new(r, 1.0, r)).grow(0.15).shrink(1.1));
    put(c, art, &art.band, Part::new(s.start + 0.08, [0.2, 0.2, 0.6], mist, ground(s.at, 0.05)).sized(Vec3::new(r * 0.75, 1.0, r * 0.75)).grow(0.1).shrink(1.25));
    put(c, art, &art.disc, Part::new(s.start, [0.08, 0.3, 0.9], mist, ground(s.at, 0.025)).sized(Vec3::new(r * 0.8, 1.0, r * 0.8)).grow(0.3));
    motes(c, art, s, 12, ink, 0.08, 1.6, 1.0, 0.25);
}

/// A Doom Bell tolling: the ground gives in, the void drawn into the mark, a ring of void light
/// closing in on it, and void light flaring up out of the middle.
fn void_toll(c: &mut Commands, art: &FxArt, s: &Scene) {
    let r = s.radius.max(1.0);
    put(c, art, s.pick(1, &art.splotch), Part::new(s.start, [0.1, 0.6, 1.4], Ink::Void, ground(s.at, 0.02)).sized(Vec3::new(r, 1.0, r)).grow(1.8));
    put(c, art, s.pick(2, &art.streaks), Part::new(s.start, [0.1, 0.5, 1.2], Ink::Void, ground(s.at, 0.021)).sized(Vec3::new(r * 1.4, 1.0, r * 1.4)).facing(s.r(3) * TAU).grow(1.5));
    put(c, art, &art.band, Part::new(s.start, [0.25, 0.25, 0.4], Ink::VoidLight, ground(s.at, 0.05)).sized(Vec3::new(r * 0.3, 1.0, r * 0.3)).grow(4.0).shrink(0.2));
    put(c, art, s.pick(4, &art.cracks), Part::new(s.start + 0.2, [0.05, 0.3, 0.9], Ink::VoidLight, ground(s.at, 0.03)).sized(Vec3::new(r * 0.7, 1.0, r * 0.7)).facing(s.r(5) * TAU).grow(0.3));
    motes(c, art, s, 10, Ink::VoidLight, 0.09, 2.4, 0.8, 0.2);
}

/// Lightning come down (Thunder Crown): a thin bolt flickering down out of the sky, its glow
/// round it, a flash where it lands, the ground burned in a star of scorch with cracks glowing
/// and fading, sparks thrown, a puff of smoke.
fn thunder_strike(c: &mut Commands, art: &FxArt, meshes: &mut Assets<Mesh>, s: &Scene) {
    let r = s.radius;
    let sky = s.at + Vec3::new(1.5 * (s.r(9) - 0.5), 11.0, 1.5 * (s.r(10) - 0.5));
    let core: Vec<Handle<Mesh>> = (0..3).map(|f| meshes.add(shapes::lightning(s.seed.wrapping_add(f), sky, s.at, 0.07))).collect();
    let glow: Vec<Handle<Mesh>> = (0..3).map(|f| meshes.add(shapes::lightning(s.seed.wrapping_add(f), sky, s.at, 0.32))).collect();
    put(c, art, &core[0].clone(), Part::new(s.start, [0.0, 0.1, 0.22], Ink::Storm, Vec3::ZERO).frames(core).flicker(0.35));
    put(c, art, &glow[0].clone(), Part::new(s.start, [0.0, 0.08, 0.25], Ink::StormMist, Vec3::ZERO).frames(glow).flicker(0.3));
    put(c, art, &art.glint, Part::new(s.start, [0.0, 0.06, 0.25], Ink::Glint, s.at + Vec3::Y * 0.3).size(r * 1.4).facing(s.r(11) * TAU).shrink(0.4));
    put(c, art, s.pick(1, &art.streaks), Part::new(s.start, [0.04, 1.6, 2.6], Ink::Scorch, ground(s.at, 0.02)).sized(Vec3::new(r * 1.1, 1.0, r * 1.1)).facing(s.r(2) * TAU).grow(0.2));
    put(c, art, s.pick(3, &art.splotch), Part::new(s.start, [0.04, 1.4, 2.4], Ink::Scorch, ground(s.at, 0.021)).size(r * 0.45));
    put(c, art, s.pick(4, &art.cracks), Part::new(s.start, [0.03, 0.25, 1.0], Ink::Storm, ground(s.at, 0.03)).sized(Vec3::new(r * 0.8, 1.0, r * 0.8)).facing(s.r(5) * TAU).flicker(0.3));
    for i in 0..12 {
        let k = 60 + i * 6;
        let velocity = s.out(k) * (2.0 + 3.0 * s.r(k + 1)) + Vec3::Y * (1.5 + 2.5 * s.r(k + 2));
        let part = Part::new(s.start, [0.01, 0.25, 0.5 + 0.3 * s.r(k + 3)], Ink::Storm, s.at + Vec3::Y * 0.2).size(0.04);
        put(c, art, &art.mote, part.motion(Motion::Thrown { velocity, gravity: 9.0, tumble: Vec3::ZERO }).flicker(0.6));
    }
    let smoke = Part::new(s.start + 0.1, [0.3, 0.5, 1.4], Ink::Smoke, s.at + Vec3::Y * 0.3).size(r * 0.5);
    put(c, art, s.pick(6, &art.splotch), smoke.motion(Motion::Thrown { velocity: Vec3::Y * 0.7, gravity: -0.2, tumble: Vec3::ZERO }).grow(0.4).shrink(1.6).spin(0.6));
}

/// A burst of something wet (a bloater, an enraged monster's blood): a splash over the ground,
/// drips thrown out from it, globs flying and landing, bubbles rising.
fn splatter(c: &mut Commands, art: &FxArt, s: &Scene, ink: Ink, light: Ink) {
    let r = s.radius;
    put(c, art, s.pick(1, &art.splotch), Part::new(s.start, [0.05, 1.6, 2.6], ink, ground(s.at, 0.02)).sized(Vec3::new(r * 0.8, 1.0, r * 0.8)).facing(s.r(2) * TAU).grow(0.3));
    put(c, art, s.pick(3, &art.streaks), Part::new(s.start, [0.08, 1.4, 2.4], ink, ground(s.at, 0.021)).sized(Vec3::new(r * 1.15, 1.0, r * 1.15)).facing(s.r(4) * TAU).grow(0.3));
    put(c, art, &art.band, Part::new(s.start, [0.1, 0.1, 0.35], light, ground(s.at, 0.05)).sized(Vec3::new(r, 1.0, r)).grow(0.2).shrink(1.1));
    clods(c, art, s, 9, 2.0, light);
    motes(c, art, s, 8, light, 0.1, 0.7, 1.4, 0.8);
}

/// Something about to land here (a bomb's fuse, a volatile monster swelling): no marker, but the
/// ground itself souring in its color: a faint stain out to where it'll reach, and the deeper
/// stain at its heart spreading out to meet it as the moment comes, cracks of it glowing, motes
/// of it rising. It's gone as what it warned of happens.
fn warning(c: &mut Commands, art: &FxArt, s: &Scene, color: FxColor) {
    let (ink, mist) = inks(color);
    let (r, lasts) = (s.radius, s.lasts.max(0.2));
    put(c, art, s.pick(1, &art.splotch), Part::new(s.start, [0.15, lasts, lasts + 0.1], mist, ground(s.at, 0.02)).size(r).facing(s.r(2) * TAU).grow(0.7));
    put(c, art, s.pick(3, &art.splotch), Part::new(s.start, [lasts, lasts, lasts + 0.1], mist, ground(s.at, 0.025)).size(r * 0.95).facing(s.r(4) * TAU).grow(0.1));
    put(c, art, s.pick(5, &art.cracks), Part::new(s.start, [lasts, lasts, lasts + 0.1], ink, ground(s.at, 0.03)).sized(Vec3::new(r * 0.9, 1.0, r * 0.9)).facing(s.r(6) * TAU).grow(0.15).flicker(0.2));
    motes(c, art, s, 8, ink, 0.06, 0.8, 0.7, lasts * 0.8);
}

/// A Doom Bell's mark, where the marked one was struck: the void seeping up through cracks in the
/// ground, spreading as the toll nears, and dark motes circling over it.
fn doom_mark(c: &mut Commands, art: &FxArt, s: &Scene) {
    let lasts = s.lasts.max(0.3);
    let r = s.radius.max(0.8);
    put(c, art, s.pick(1, &art.cracks), Part::new(s.start, [lasts, lasts, lasts + 0.15], Ink::VoidLight, ground(s.at, 0.03)).sized(Vec3::new(r * 1.4, 1.0, r * 1.4)).facing(s.r(2) * TAU).grow(0.2).flicker(0.15));
    put(c, art, s.pick(3, &art.splotch), Part::new(s.start, [lasts, lasts, lasts + 0.15], Ink::Void, ground(s.at, 0.02)).size(r).grow(0.2));
    for i in 0..4 {
        let motion = Motion::Orbit { center: s.at, angle: i as f32 / 4.0 * TAU, speed: 2.5, radius: (r * 0.7, r * 0.3), height: (1.6, 0.4) };
        put(c, art, &art.mote, Part::new(s.start, [0.2, lasts, lasts + 0.1], Ink::VoidLight, s.at).size(0.07).motion(motion).flicker(0.3));
    }
}

/// An Echo Stone's echo gathering where the ability hit: a pale glow welling up there, wavering,
/// ripples of spirit light running back in toward it (the hit's own, played backward), wisps of
/// it drawn in. It goes off as `echo_burst`.
fn echo(c: &mut Commands, art: &FxArt, s: &Scene) {
    let (r, lasts) = (s.radius, s.lasts.max(0.2));
    let end = [lasts, lasts, lasts + 0.08];
    put(c, art, s.pick(3, &art.splotch), Part::new(s.start, end, Ink::SpiritMist, ground(s.at, 0.025)).size(r * 0.6).grow(0.3).flicker(0.4));
    // Ripples running in, one after another, each gone as it reaches the middle.
    for i in 0..3 {
        let born = s.start + lasts * i as f32 / 3.0;
        let left = (s.start + lasts - born).max(0.1);
        let ripple = Part::new(born, [left, left, left + 0.05], Ink::Spirit, ground(s.at, 0.05 + 0.002 * i as f32)).sized(Vec3::new(r * 0.15, 1.0, r * 0.15)).grow(8.0);
        put(c, art, &art.band, ripple);
    }
    for i in 0..10 {
        let k = 50 + i * 6;
        let motion = Motion::Orbit { center: s.at, angle: s.r(k) * TAU, speed: 3.0, radius: (r * 1.1, 0.1), height: (0.2 + 0.8 * s.r(k + 1), 0.15) };
        put(c, art, &art.mote, Part::new(s.start + lasts * 0.5 * s.r(k + 2), [0.1, lasts * 0.4, lasts * 0.5], Ink::Spirit, s.at).size(0.06).motion(motion).flicker(0.3));
    }
}

/// An echo going off: the blow struck again out of nowhere, a flash where it lands, a breath of
/// spirit light, and three ripples of spirit light racing out one after the other, each fainter and
/// shorter than the last (the echo, echoing), wisps rising after.
fn echo_burst(c: &mut Commands, art: &FxArt, s: &Scene) {
    let r = s.radius;
    put(c, art, &art.glint, Part::new(s.start, [0.02, 0.06, 0.25], Ink::Glint, s.at + Vec3::Y * 0.5).size(r * 0.8).facing(s.r(1) * TAU).grow(0.3).shrink(0.5));
    put(c, art, s.pick(4, &art.splotch), Part::new(s.start, [0.06, 0.25, 0.8], Ink::SpiritMist, ground(s.at, 0.025)).size(r * 0.85).grow(0.3));
    for (i, (reach, ink)) in [(1.0, Ink::Spirit), (0.8, Ink::Spirit), (0.6, Ink::SpiritMist)].into_iter().enumerate() {
        let born = s.start + 0.14 * i as f32;
        let ripple = Part::new(born, [0.2, 0.22, 0.5], ink, ground(s.at, 0.06 - 0.003 * i as f32)).sized(Vec3::new(r * reach, 1.0, r * reach)).grow(0.12).shrink(1.15);
        put(c, art, &art.band, ripple);
    }
    motes(c, art, s, 10, Ink::Spirit, 0.07, 1.8, 1.0, 0.4);
}

/// A meteor coming down: first its burning shadow on the ground where it'll land (`warning`'s
/// fire, spreading as it nears), then the rock itself, falling steep out of the sky ablaze,
/// streaming fire and sparks, landing as it goes off (its blast is `fire_blast`).
fn meteor(c: &mut Commands, art: &FxArt, s: &Scene) {
    warning(c, art, s, FxColor::Fire);
    let lasts = s.lasts.max(0.3);
    // In sight for the end of its fall only: out of the sky, steeply, from a side of its own.
    let fall = (lasts * 0.6).max(0.25);
    let born = s.start + lasts - fall;
    let side = s.out(1) * 3.0;
    let from = s.at + side + Vec3::Y * 14.0;
    let tumble = Vec3::new(s.r(2) - 0.5, s.r(3) - 0.5, s.r(4) - 0.5) * 12.0;
    let falling = |tumble| Motion::Arc { from, to: s.at + Vec3::Y * 0.2, height: 0.0, tumble };
    let life = [0.02, fall, fall + 0.02];
    put(c, art, &art.chunk, Part::new(born, life, Ink::Ember, from).size(0.5).motion(falling(tumble)).shrink(1.0));
    put(c, art, &art.mote, Part::new(born, life, Ink::Flame, from).size(1.1).motion(falling(Vec3::ZERO)).flicker(0.3).shrink(1.0));
    // Fire streaming off it: each bit of the trail a little behind it on its fall.
    for i in 1..8 {
        let lag = fall * 0.06 * i as f32;
        let part = Part::new(born + lag, [0.02, fall, fall + 0.02], if i % 2 == 0 { Ink::Spark } else { Ink::Flame }, from);
        put(c, art, &art.mote, part.size(0.7 - 0.07 * i as f32).motion(falling(Vec3::ZERO)).flicker(0.5).shrink(1.0));
    }
}

/// A monster blinking: it's sucked into a knot of the void where it stood (motes of it drawn in,
/// a dark ripple closing), and bursts back out of one where it lands (motes thrown out, a ripple
/// opening, a flash).
fn blink(c: &mut Commands, art: &FxArt, s: &Scene) {
    put(c, art, s.pick(1, &art.splotch), Part::new(s.start, [0.04, 0.2, 0.6], Ink::Void, ground(s.at, 0.02)).size(0.9).grow(1.6));
    put(c, art, &art.band, Part::new(s.start, [0.18, 0.18, 0.3], Ink::VoidLight, ground(s.at, 0.05)).sized(Vec3::new(0.3, 1.0, 0.3)).grow(4.0).shrink(0.2));
    for i in 0..10 {
        let k = 40 + i * 6;
        let motion = Motion::Orbit { center: s.at, angle: s.r(k) * TAU, speed: 5.0, radius: (1.2, 0.05), height: (0.3 + 1.4 * s.r(k + 1), 0.9) };
        put(c, art, &art.mote, Part::new(s.start, [0.02, 0.2, 0.3], Ink::VoidLight, s.at).size(0.07).motion(motion).flicker(0.3));
    }
    let there = Scene { at: s.to, radius: 1.2, seed: s.seed ^ 0x55, ..*s };
    put(c, art, &art.glint, Part::new(s.start + 0.05, [0.02, 0.06, 0.25], Ink::Glint, s.to + Vec3::Y * 1.0).size(1.4).facing(s.r(9) * TAU).shrink(0.4));
    put(c, art, &art.band, Part::new(s.start + 0.05, [0.12, 0.12, 0.4], Ink::VoidLight, ground(s.to, 0.05)).sized(Vec3::new(1.4, 1.0, 1.4)).grow(0.15).shrink(1.2));
    put(c, art, there.pick(1, &art.splotch), Part::new(s.start + 0.05, [0.05, 0.25, 0.7], Ink::VoidMist, ground(s.to, 0.025)).size(1.2).grow(0.3));
    for i in 0..10 {
        let k = 80 + i * 6;
        let velocity = there.out(k) * (2.0 + 2.0 * there.r(k + 1)) + Vec3::Y * (1.0 + 2.0 * there.r(k + 2));
        put(c, art, &art.mote, Part::new(s.start + 0.05, [0.01, 0.2, 0.45], Ink::VoidLight, s.to + Vec3::Y * 0.9).size(0.06).motion(Motion::Thrown { velocity, gravity: 4.0, tumble: Vec3::ZERO }).flicker(0.4));
    }
}

/// Thunder Crown's bolt gathering: the air over the mark charging, sparks crackling in toward it
/// and the ground darkening under it, then the bolt (its strike is `thunder_strike`).
fn gathering_storm(c: &mut Commands, art: &FxArt, s: &Scene) {
    let (r, lasts) = (s.radius, s.lasts.max(0.1));
    put(c, art, s.pick(1, &art.splotch), Part::new(s.start, [lasts, lasts, lasts + 0.05], Ink::StormMist, ground(s.at, 0.025)).size(r * 0.8).grow(0.2));
    for i in 0..10 {
        let k = 50 + i * 6;
        let motion = Motion::Orbit { center: s.at, angle: s.r(k) * TAU, speed: 6.0, radius: (r, 0.05), height: (0.3 + 1.5 * s.r(k + 1), 0.2) };
        put(c, art, &art.mote, Part::new(s.start, [0.02, lasts, lasts + 0.05], Ink::Storm, s.at).size(0.04).motion(motion).flicker(0.6));
    }
}

/// Storm Coil's arc: a thin crackling bolt leaping from one to the next, its glow round it,
/// flickering, a flash and sparks where it lands.
fn arc(c: &mut Commands, art: &FxArt, meshes: &mut Assets<Mesh>, s: &Scene) {
    let (from, to) = (s.at + Vec3::Y * 1.0, s.to + Vec3::Y * 0.9);
    let core: Vec<Handle<Mesh>> = (0..3).map(|f| meshes.add(shapes::lightning(s.seed.wrapping_add(f * 31), from, to, 0.045))).collect();
    let glow: Vec<Handle<Mesh>> = (0..3).map(|f| meshes.add(shapes::lightning(s.seed.wrapping_add(f * 31), from, to, 0.2))).collect();
    put(c, art, &core[0].clone(), Part::new(s.start, [0.0, 0.22, 0.4], Ink::Storm, Vec3::ZERO).frames(core).flicker(0.35));
    put(c, art, &glow[0].clone(), Part::new(s.start, [0.0, 0.18, 0.4], Ink::StormMist, Vec3::ZERO).frames(glow).flicker(0.2));
    put(c, art, &art.glint, Part::new(s.start, [0.0, 0.08, 0.3], Ink::Glint, to).size(0.9).facing(s.r(7) * TAU).shrink(0.3));
    for i in 0..6 {
        let k = 40 + i * 6;
        let velocity = s.out(k) * (1.5 + 1.5 * s.r(k + 1)) + Vec3::Y * (1.0 + 1.5 * s.r(k + 2));
        put(c, art, &art.mote, Part::new(s.start, [0.01, 0.2, 0.45], Ink::Storm, to).size(0.04).motion(Motion::Thrown { velocity, gravity: 9.0, tumble: Vec3::ZERO }).flicker(0.5));
    }
}

/// Something thrown (a powder keg, a plague bomb): it flies in an arc, tumbling, spitting sparks
/// (or dripping) on the way.
fn lob(c: &mut Commands, art: &FxArt, s: &Scene, color: FxColor) {
    let lasts = s.lasts.max(0.2);
    let tumble = Vec3::new(s.r(1) - 0.5, s.r(2) - 0.5, s.r(3) - 0.5) * 18.0;
    let arc = |height: f32| Motion::Arc { from: s.at + Vec3::Y * 1.2, to: s.to + Vec3::Y * 0.2, height, tumble };
    let height = 1.5 + 0.15 * s.at.distance(s.to);
    let (body, mesh, size, trail) = match color {
        FxColor::Poison => (Ink::Poison, &art.mote, Vec3::splat(0.32), Ink::PoisonLight),
        _ => (Ink::Earth, &art.keg, Vec3::new(0.24, 0.3, 0.24), Ink::Spark),
    };
    put(c, art, mesh, Part::new(s.start, [0.02, lasts, lasts + 0.02], body, s.at).sized(size).motion(arc(height)));
    // Its fuse spitting sparks (a bomb's glow), following it.
    for i in 0..6 {
        let delay = lasts * i as f32 / 7.0;
        let motion = Motion::Arc { from: s.at + Vec3::Y * 1.3, to: s.to + Vec3::Y * 0.3, height, tumble: Vec3::ZERO };
        let part = Part::new(s.start, [0.02, lasts, lasts + 0.02], trail, s.at).size(0.08).motion(motion).flicker(0.7);
        put(c, art, &art.mote, Part { born: part.born + delay * 0.15, ..part });
    }
}

/// Gold (Midas Coin, a chest, a boss's fall): coins thrown up, spinning, falling and lying a
/// moment on the ground, glints of light off them.
fn coins(c: &mut Commands, art: &FxArt, s: &Scene) {
    let count = (6.0 + 4.0 * s.radius) as u32;
    for i in 0..count {
        let k = 20 + i * 6;
        let velocity = s.out(k) * (0.8 + 1.6 * s.r(k + 1)) * s.radius.max(1.0) + Vec3::Y * (4.0 + 2.5 * s.r(k + 2));
        let tumble = Vec3::new(18.0 + 10.0 * s.r(k + 3), 0.0, 6.0 * s.r(k + 4));
        let part = Part::new(s.start + 0.05 * s.r(k + 5), [0.02, 1.2, 1.6], Ink::Gold, s.at + Vec3::Y * 0.6).sized(Vec3::new(0.16, 0.16, 0.16));
        put(c, art, &art.coin, part.motion(Motion::Thrown { velocity, gravity: 13.0, tumble }).shrink(0.0));
    }
    for i in 0..6 {
        let k = 200 + i * 4;
        let part = Part::new(s.start + 0.2 + 0.8 * s.r(k), [0.08, 0.12, 0.35], Ink::Glint, s.spot(k + 1, 1.0) + Vec3::Y * (0.3 + 0.8 * s.r(k + 2)));
        put(c, art, &art.glint, part.size(0.3).turn(Quat::from_rotation_x(-FRAC_PI_2 * 0.6)).spin(3.0).grow(0.2).shrink(0.2));
    }
}

/// A monster clawing up out of the ground: the earth breaking open, dark and cracked, clods
/// thrown up, a breath of grave mist.
fn rise(c: &mut Commands, art: &FxArt, s: &Scene) {
    let r = s.radius.max(0.6);
    put(c, art, s.pick(1, &art.splotch), Part::new(s.start, [0.15, 1.2, 2.2], Ink::Scorch, ground(s.at, 0.02)).sized(Vec3::new(r, 1.0, r)).facing(s.r(2) * TAU).grow(0.2));
    put(c, art, s.pick(3, &art.cracks), Part::new(s.start, [0.1, 1.0, 2.0], Ink::Scorch, ground(s.at, 0.022)).sized(Vec3::new(r * 1.5, 1.0, r * 1.5)).facing(s.r(4) * TAU).grow(0.3));
    clods(c, art, s, 8, 1.4, Ink::Earth);
    motes(c, art, s, 6, Ink::PoisonLight, 0.08, 0.6, 1.2, 0.4);
}

/// A rot priest's prayer: a sickly haze breathing up off the ground round it, motes of it rising
/// over the monsters it mends.
fn mend(c: &mut Commands, art: &FxArt, s: &Scene) {
    let r = s.radius;
    put(c, art, s.pick(1, &art.splotch), Part::new(s.start, [0.25, 0.45, 1.0], Ink::PoisonMist, ground(s.at, 0.03)).size(r).grow(0.5));
    motes(c, art, s, 18, Ink::PoisonLight, 0.07, 1.8, 1.1, 0.5);
}

/// The ink an effect of `color` glows in, and its mist.
fn inks(color: FxColor) -> (Ink, Ink) {
    match color {
        FxColor::Fire => (Ink::Flame, Ink::Smoke),
        FxColor::Spirit => (Ink::Spirit, Ink::SpiritMist),
        FxColor::Frost => (Ink::Frost, Ink::Glaze),
        FxColor::Gold => (Ink::Gold, Ink::SpiritMist),
        FxColor::Blood => (Ink::Blood, Ink::Blood),
        FxColor::Poison => (Ink::PoisonLight, Ink::Poison),
        FxColor::Void => (Ink::VoidLight, Ink::Void),
        FxColor::Storm => (Ink::Storm, Ink::StormMist),
        FxColor::Thorn => (Ink::Thorn, Ink::Scorch),
    }
}
