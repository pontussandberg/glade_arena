//! Our own fighter drawn smoothly between the server's positions, on our clock (`Glide`), or,
//! with the action camera, where our keys will have taken it by the time the server says so
//! (`PredictedWalk`); and every fighter's transform kept on its position.

use super::*;

use std::collections::VecDeque;

use arena_shared::sim;

use crate::Me;
use crate::feedback::{AttackClock, ME_BEHIND_TICKS};
use crate::arena::to_world;

/// Our own fighter, drawn smoothly: each position from the server is noted with the server tick
/// it's from, and the fighter is drawn on our clock (`AttackClock`, a tick behind the latest
/// state), in between those. Updates come 32 times a second, unevenly; drawn as they arrive it
/// would move in jerks, and the camera with it. Others are interpolated by Lightyear; ours isn't,
/// for the least delay after a command, so this adds a tick. A jump (respawn, teleport) isn't
/// eased.
#[derive(Component, Default)]
pub(super) struct Glide {
    /// (server tick, position), oldest first.
    pub(super) samples: std::collections::VecDeque<(f32, Vec2)>,
    /// What it last drew, to tell a new position from the server (anything else) from its own.
    pub(super) drawn: Vec2,
}

impl Glide {
    /// Where it was at `tick`, between the samples around it (the first or last outside them).
    pub(super) fn at(&self, tick: f32) -> Option<Vec2> {
        let after = self.samples.iter().position(|(t, _)| *t > tick);
        match after {
            None => self.samples.back().map(|(_, p)| *p),
            Some(0) => self.samples.front().map(|(_, p)| *p),
            Some(i) => {
                let ((t0, p0), (t1, p1)) = (self.samples[i - 1], self.samples[i]);
                Some(p0.lerp(p1, (tick - t0) / (t1 - t0)))
            }
        }
    }
}


pub(super) fn glide_me(mut commands: Commands, clock: AttackClock, mut me: Query<(Entity, &mut Pos, Option<&mut Glide>), With<Me>>) {
    let (Some(latest), now) = (clock.latest_tick(), clock.now(true)) else { return };
    for (entity, mut pos, glide) in &mut me {
        let Some(mut glide) = glide else {
            let mut glide = Glide { drawn: pos.0, ..default() };
            glide.samples.push_back((latest as f32, pos.0));
            commands.entity(entity).insert(glide);
            continue;
        };
        if pos.0 != glide.drawn {
            let tick = latest as f32;
            let last = glide.samples.back().copied();
            if pos.0.distance(glide.drawn) > JUMP {
                glide.samples.clear();
            } else if let Some((last_tick, last_pos)) = last
                && last_tick < tick - 1.0
            {
                // It stood still since: the server only sends a position when it changes.
                glide.samples.push_back((tick - 1.0, last_pos));
            }
            if glide.samples.back().is_some_and(|(t, _)| *t >= tick) {
                glide.samples.pop_back();
            }
            glide.samples.push_back((tick, pos.0));
        }
        // Keep one sample before now, to draw from.
        while glide.samples.len() > 2 && glide.samples[1].0 <= now {
            glide.samples.pop_front();
        }
        let drawn = glide.at(now).unwrap_or(pos.0);
        glide.drawn = drawn;
        if pos.0 != drawn {
            pos.0 = drawn;
        }
    }
}

/// How much of what the server put the prediction off by is shown away each second.
const CORRECTION_RATE: f32 = 8.0;
/// Ticks between a key going out and the server's state with it coming back, over the round
/// trip itself (the server's own step and the packets' framing): the first guess, until it's
/// been measured (`PredictedWalk::lead`).
const PREDICT_SLACK_TICKS: f32 = 2.0;
/// How much of each measurement of that delay the learned one takes on.
const LEAD_PULL: f32 = 0.3;
/// How long (seconds) what we walked is remembered, to replay.
const WALKS_KEPT: f32 = 2.0;
/// How many ticks ahead of the server's latest position the prediction will walk at most (a
/// stalled connection doesn't run off).
const PREDICT_MOST_TICKS: u32 = 64;

/// Our own fighter with the action camera, drawn where it will be once the server has our keys
/// rather than where it was: its latest position walked on by the keys we've held since, a
/// round trip's worth (`predict_me`), by the same sim the server walks it with. Where the server
/// turns out to disagree, the difference is shown away smoothly rather than snapped.
#[derive(Component, Default)]
pub(super) struct PredictedWalk {
    /// The latest position from the server, and the tick it's from.
    base: Option<(u32, Vec2)>,
    /// What it last drew, to tell a new position from the server (anything else) from its own.
    drawn: Vec2,
    /// How far what's drawn still is from the prediction the server's latest position gives.
    error: Vec2,
    /// Which way our keys walked, each frame: (seconds, way), oldest first.
    walks: VecDeque<(f32, Vec2)>,
    /// How many ticks past the server's latest position our newest keys will be stepped, as
    /// measured from when each change of ours showed in `Walking`; none measured yet at first.
    lead: Option<f32>,
    /// Which way the server last said it stepped us (`Walking`), to catch each change.
    server_walk: Vec2,
    /// How far (meters) the last correction worth noticing moved the prediction (shown in the
    /// status line, to see what a glide was).
    last_fix: f32,
}

impl PredictedWalk {
    /// The lead learned so far (ticks), if any change of our keys has been measured.
    pub(super) fn lead(&self) -> Option<f32> {
        self.lead
    }

    /// How far the last correction worth noticing moved the prediction (meters).
    pub(super) fn last_fix(&self) -> f32 {
        self.last_fix
    }

    /// How far what's drawn still is from the prediction (meters): the glide left to show away.
    pub(super) fn error(&self) -> f32 {
        self.error.length()
    }

    /// Which way we walked at `at` seconds (the last noted before it).
    fn walk_at(&self, at: f32) -> Vec2 {
        self.walks.iter().rev().find(|(t, _)| *t <= at).or(self.walks.front()).map_or(Vec2::ZERO, |(_, way)| *way)
    }

    /// Where we are at tick `at` (fractional, between two ticks' steps), walked on from `base`
    /// tick by tick by `step`, each tick by the keys held when its input went out, taken to be
    /// `at + 1 - tick` ticks before `now` (seconds).
    fn predict(&self, base: (u32, Vec2), at: f32, now: f32, mut step: impl FnMut(Vec2, Vec2, u32) -> Vec2) -> Vec2 {
        let (base_tick, mut pos) = base;
        let last = (at.floor() as u32).max(base_tick).min(base_tick + PREDICT_MOST_TICKS);
        let mut before = pos;
        for tick in base_tick + 1..=last + 1 {
            before = pos;
            pos = step(pos, self.walk_at(now - (at + 1.0 - tick as f32) * TICK_DT), tick);
        }
        before.lerp(pos, at.fract())
    }
}

/// Our fighter's position with the action camera: predicted from the server's latest position
/// (see `PredictedWalk`). A new position from the server replaces the base; what that changes in the
/// prediction goes into the error shown away. A position that hasn't changed since is still news
/// each tick (the server only sends one when it changes). How far ahead of the server's latest
/// position our keys are stepped is guessed from the ping, then measured: each change of our
/// keys shows up in `Walking` at some tick, and the lead is pulled toward what would have put
/// it there, so a stop is predicted where the server stops, not short of it or past it.
pub(super) fn predict_me(
    mut commands: Commands,
    time: Res<Time<Real>>,
    clock: AttackClock,
    map: Res<crate::rooms::CurrentMap>,
    desired: Res<crate::DesiredInput>,
    link: Query<&Link, With<Client>>,
    mut me: Query<(Entity, &mut Pos, &ClassId, Option<&Dashing>, Option<&Chilled>, Option<&Hasted>, Option<&Frenzy>, Option<&Walking>, Option<&mut PredictedWalk>), With<Me>>,
) {
    let Some(latest) = clock.latest_tick() else { return };
    let rtt = link.single().map_or(0.0, |link| link.stats.rtt.as_secs_f32());
    let now = time.elapsed_secs();
    let guessed_lead = rtt * TICK_HZ as f32 + PREDICT_SLACK_TICKS;
    let map = map.map();
    for (entity, mut pos, class, dashing, chilled, hasted, frenzy, walking, predicted) in &mut me {
        let Some(mut predicted) = predicted else {
            commands.entity(entity).insert(PredictedWalk { drawn: pos.0, server_walk: walking.map_or(Vec2::ZERO, |w| w.0), ..default() });
            continue;
        };
        predicted.walks.push_back((now, desired.0.walk));
        while predicted.walks.front().is_some_and(|(t, _)| *t < now - WALKS_KEPT) {
            predicted.walks.pop_front();
        }
        let mut lead = predicted.lead.unwrap_or(guessed_lead);
        if let Some(walking) = walking
            && walking.0 != predicted.server_walk
        {
            predicted.server_walk = walking.0;
            // When our keys last turned to what the server just stepped (the newest run of it,
            // still held): the model steps that at some tick; the server did at `latest`.
            let held = predicted.walks.back().is_some_and(|(_, way)| *way == walking.0);
            let since = predicted.walks.iter().rev().take_while(|(_, way)| *way == walking.0).last().map(|(t, _)| *t);
            if let (true, Some(since)) = (held, since) {
                let at = clock.now(true) + ME_BEHIND_TICKS + lead;
                let modeled = at + 1.0 - (now - since) / TICK_DT;
                let measured = (lead - (modeled - latest as f32)).clamp(0.0, PREDICT_MOST_TICKS as f32 / 2.0);
                lead = if predicted.lead.is_some() { lead + (measured - lead) * LEAD_PULL } else { measured };
                predicted.lead = Some(lead);
            }
        }
        let at = clock.now(true) + ME_BEHIND_TICKS + lead;
        // The server's step for one tick of ours: walking (never rooted by a windup, in action
        // mode; faster in a frenzy, as the server walks it), or a dash running on.
        let (chilled, hasted, frenzy) = (chilled.copied().unwrap_or_default(), hasted.copied().unwrap_or_default(), frenzy.copied().unwrap_or_default());
        let step = |mut dashing: Dashing| {
            move |from: Vec2, walk: Vec2, tick: u32| {
                let input = PlayerInput { walk, aim: Vec2::X, ..default() };
                let boost = 1.0 + frenzy.at(tick as f32);
                sim::move_player_boosted(map, from, &input, *class, &AttackState::default(), &mut dashing, &chilled, &hasted, tick, boost)
            }
        };
        let dashing = dashing.copied().unwrap_or_default();
        let fresh = pos.0 != predicted.drawn;
        let newer = predicted.base.is_none_or(|(tick, _)| tick < latest);
        if fresh || newer {
            let base = (latest, if fresh { pos.0 } else { predicted.base.map_or(pos.0, |(_, p)| p) });
            if let Some(old) = predicted.base {
                let was = predicted.predict(old, at, now, step(dashing));
                let is = predicted.predict(base, at, now, step(dashing));
                predicted.error += was - is;
                if (was - is).length() >= 0.02 {
                    predicted.last_fix = (was - is).length();
                }
            }
            predicted.base = Some(base);
        }
        let Some(base) = predicted.base else { continue };
        let prediction = predicted.predict(base, at, now, step(dashing));
        // A jump (a respawn) isn't eased.
        if predicted.error.length() > JUMP {
            predicted.error = Vec2::ZERO;
        }
        predicted.error *= (-CORRECTION_RATE * time.delta_secs()).exp();
        let drawn = prediction + predicted.error;
        predicted.drawn = drawn;
        if pos.0 != drawn {
            pos.0 = drawn;
        }
    }
}

/// How high (meters) a knock-up this long (ticks) or longer throws a fighter at the top; a
/// shorter one is a lower hop (a stagger, not a launch).
const LIFT_HEIGHT: f32 = 0.45;
const FULL_LIFT_TICKS: f32 = 24.0;

/// Knocked up (`Chilled::lifted`): off the ground, up and back down over the knock-up, on the
/// timeline it's shown on; set down again once it's over.
pub(super) fn lift_fighters(clock: crate::feedback::AttackClock, mut q: Query<(&Chilled, &mut Transform, Has<crate::Me>)>) {
    for (chilled, mut transform, me) in &mut q {
        let now = clock.now(me);
        let span = chilled.lifted;
        let y = if span.covers(now) {
            let ticks = (span.until - span.from).max(1) as f32;
            let t = (now - span.from as f32) / ticks;
            LIFT_HEIGHT * (ticks / FULL_LIFT_TICKS).min(1.0) * 4.0 * t * (1.0 - t)
        } else {
            0.0
        };
        if transform.translation.y != y {
            transform.translation.y = y;
        }
    }
}

pub(super) fn sync_transforms(mut q: Query<(&Pos, &mut Transform), (Changed<Pos>, Without<Projectile>)>) {
    for (pos, mut transform) in &mut q {
        transform.translation = to_world(pos.0, 0.0);
    }
}
