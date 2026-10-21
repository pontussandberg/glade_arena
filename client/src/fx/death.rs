//! A monster's death on screen. Where one dies: a puff of dust, gore and bone chips flung up
//! and out (landing and fading where they fall), a dark splotch that lingers on the ground, and
//! its soul rising away as a pale wisp. Elites burst bigger, bosses bigger still — and a kill
//! near your fighter stirs the camera (`camera::Shake`), the bigger the monster the more.

use std::f32::consts::TAU;

use super::*;
use crate::Me;
use crate::arena::to_world;
use crate::audio::{Sfx, SoundQueue, earshot};
use crate::camera::Shake;
use crate::swish::noise;

/// Whether we last saw this fighter alive: a burst plays only for a death we watch happen, not
/// for one already dead when it first appears.
#[derive(Component)]
pub(super) struct SeenAlive(bool);

/// How hard a kill shakes the camera right next to us, and how far away one still registers.
const SHAKE: f32 = 0.16;
const ELITE_SHAKE: f32 = 0.3;
const BOSS_SHAKE: f32 = 0.6;
const SHAKE_REACH: f32 = 18.0;
const BOSS_SHAKE_REACH: f32 = 60.0;

pub(super) fn burst_deaths(
    mut commands: Commands,
    time: Res<Time>,
    art: Res<FxArt>,
    mut shake: ResMut<Shake>,
    mut sounds: ResMut<SoundQueue>,
    me: Query<&Pos, With<Me>>,
    mut fighters: Query<(Entity, &ClassId, &Pos, &Health, Option<&mut SeenAlive>, Has<Elite>), Changed<Health>>,
    mut seed: Local<u32>,
) {
    let now = time.elapsed_secs();
    for (fighter, class, pos, health, seen, elite) in &mut fighters {
        let Some(mut seen) = seen else {
            commands.entity(fighter).insert(SeenAlive(health.alive()));
            continue;
        };
        let was_alive = std::mem::replace(&mut seen.0, health.alive());
        // Players don't burst: they fall where they stood (`downed`).
        if !was_alive || health.alive() || class.def().playable {
            continue;
        }
        *seed = seed.wrapping_add(1);
        let def = class.def();
        let size = crate::rig::drawn_size(*class, elite);
        burst(&mut commands, &art, now, to_world(pos.0, 0.0), size, def.is_boss(), *seed);
        let (amount, reach) = match () {
            _ if def.is_boss() => (BOSS_SHAKE, BOSS_SHAKE_REACH),
            _ if elite => (ELITE_SHAKE, SHAKE_REACH),
            _ => (SHAKE, SHAKE_REACH),
        };
        if let Ok(me) = me.single() {
            let away = pos.0.distance(me.0);
            shake.add(amount * (1.0 - away / reach).clamp(0.0, 1.0));
            match def.is_boss() {
                true => sounds.push(Sfx::BossKill, 1.0),
                false => sounds.push(Sfx::Kill, (0.3 + 0.7 * earshot(away, 22.0)) * if elite { 1.0 } else { 0.8 }),
            }
        }
    }
}

/// The burst itself, `size` the monster's drawn size, bigger and longer for a boss.
fn burst(commands: &mut Commands, art: &FxArt, now: f32, at: Vec3, size: f32, boss: bool, seed: u32) {
    let r = |k: u32| noise(seed, k);
    let s = size * if boss { 1.4 } else { 1.0 };
    // A puff of dust where it fell.
    let dust = art.splotch[(r(0) * VARIANTS as f32) as usize % VARIANTS as usize].clone();
    put(commands, art, &dust, Part::new(now, [0.04, 0.16, 0.5], Ink::Smoke, at.with_y(0.06)).size(2.2 * s).grow(0.35).facing(r(1) * TAU));
    // A dark splotch that stays a while, sunk into the grass.
    let splotch = art.splotch[(r(2) * VARIANTS as f32) as usize % VARIANTS as usize].clone();
    let linger = if boss { [0.06, 9.0, 14.0] } else { [0.06, 4.5, 8.0] };
    put(commands, art, &splotch, Part::new(now, linger, Ink::Blood, at.with_y(0.02)).size((1.5 + 0.5 * r(3)) * s).grow(0.4).facing(r(4) * TAU));
    // Gore and bone flung up and out, tumbling, lying where they land as they fade.
    let chunks = if boss { 18 } else { 6 + (r(5) * 3.0) as u32 };
    for k in 0..chunks {
        let n = |i: u32| r(10 + k * 7 + i);
        let angle = n(0) * TAU;
        let speed = 1.2 + 2.6 * n(1);
        let velocity = Vec3::new(angle.cos() * speed, 2.0 + 2.6 * n(2), -angle.sin() * speed);
        let tumble = Vec3::new(n(3) - 0.5, n(4) - 0.5, n(5) - 0.5) * 16.0;
        let ink = if n(6) < 0.45 { Ink::Bone } else { Ink::Blood };
        let life = [0.0, 0.7 + 0.5 * n(2), 1.3 + 0.6 * n(1)];
        let part = Part::new(now, life, ink, at + Vec3::Y * 0.5 * s)
            .size((0.07 + 0.09 * n(0)) * s.min(1.6))
            .motion(Motion::Thrown { velocity, gravity: 9.0, tumble })
            .shrink(0.5);
        put(commands, art, &art.chunk, part);
    }
    // Its soul slips away: a pale wisp (or a few, off a boss) drifting up, guttering out.
    let souls = if boss { 3 } else { 1 };
    for k in 0..souls {
        let n = |i: u32| r(200 + k * 5 + i);
        let drift = Vec3::new(n(0) - 0.5, 0.0, n(1) - 0.5) * 0.5;
        let rise = Motion::Thrown { velocity: Vec3::Y * (0.9 + 0.4 * n(2)) + drift, gravity: -0.5, tumble: Vec3::ZERO };
        let part = Part::new(now + 0.12 + 0.15 * k as f32, [0.12, 0.9, 1.6], Ink::Wisp, at + Vec3::Y * 0.6 * s)
            .size(0.11 * s.clamp(0.7, 1.8))
            .motion(rise)
            .flicker(0.35)
            .shrink(0.25);
        put(commands, art, &art.mote, part);
    }
}
