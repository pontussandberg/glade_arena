//! Our own fighter drawn smoothly between the server's positions, on our clock (`Glide`), and
//! every fighter's transform kept on its position.

use super::*;

use crate::Me;
use crate::feedback::AttackClock;
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
