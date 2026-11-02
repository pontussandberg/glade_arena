//! The faint trail following a blade wherever it moves fast, brighter while its Follow
//! Through is on.

use super::*;

use std::collections::VecDeque;

use crate::fx::draw::{Drawn, haze};

/// How long the trail lingers (seconds), how fast the blade's tip must move for it to show
/// (m/s: it fades in from the first to the second), and the stretch of the blade it follows
/// (from its base, through where it's brightest, to its tip, as fractions of its length).
pub(crate) const TRAIL_LIFE: f32 = 0.16;
pub(crate) const TRAIL_SPEED: (f32, f32) = (6.0, 13.0);
/// How bright the trail is when the blade isn't quickened (`Blade::quickened`; it's 1 when it is).
pub(crate) const TRAIL_DIM: f32 = 0.5;
/// How see-through the trail is at its brightest.
const TRAIL_ALPHA: f32 = 0.47;

/// A blade's trail: where the blade has lately been (in the world). Drawn among the effects'
/// parts (`fx::draw`), laid out anew each frame it shows: made over as a mesh every frame, it
/// cost the browser new buffers on the GPU every frame of every swing.
#[derive(Component, Default)]
pub(crate) struct Trail {
    pub(super) samples: VecDeque<(f32, [Vec3; 3])>,
}

pub(super) fn add_trails(mut commands: Commands, fighters: Query<Entity, (With<Blade>, Without<Trail>)>) {
    for fighter in &fighters {
        commands.entity(fighter).insert(Trail::default());
    }
}

/// A ribbon through the blade's recent places (`(alpha, [base, middle, tip])`, oldest first),
/// clear at the blade's base, brightest toward its tip, `color` (linear, hazed as far off as it
/// is), laid out among the effects' parts.
fn trail_ribbon(places: &[(f32, [Vec3; 3])], color: Vec3, drawn: &mut Drawn) {
    let shade = [0.0, 0.6, 1.0];
    let first = drawn.corners() as u32;
    drawn.add_corners(places.iter().flat_map(|&(alpha, place)| (0..3).map(move |k| (place[k], color.extend(alpha * shade[k] * TRAIL_ALPHA)))));
    drawn.add_triangles((1..places.len() as u32).flat_map(|i| {
        let (p0, p1) = (first + (i - 1) * 3, first + i * 3);
        (0..2).flat_map(move |k| [p0 + k, p0 + k + 1, p1 + k + 1, p0 + k, p1 + k + 1, p1 + k])
    }));
}

/// Notes where each blade is now (after it's posed and placed), drops what's older than the trail
/// lasts, and redraws its trail while it shows: bright where the blade moved fast, fading with
/// age.
pub(crate) fn trail_blades(
    time: Res<Time>,
    mut fighters: Query<(&Blade, &mut Trail, &InheritedVisibility)>,
    blades: Query<&GlobalTransform>,
    camera: Single<(&GlobalTransform, Option<&DistanceFog>), With<crate::camera::CameraRig>>,
    mut drawn: ResMut<Drawn>,
) {
    let now = time.elapsed_secs();
    let (eye, fog) = *camera;
    let air = Vec4::from_array(AIR.to_linear().to_f32_array()).truncate();
    let hazed = fog.map(|fog| (Vec4::from_array(fog.color.to_linear().to_f32_array()).truncate(), fog));
    for (blade, mut trail, visible) in &mut fighters {
        let Ok(held) = blades.get(blade.held) else { continue };
        // From where the blade starts, through its middle, to its tip.
        let place = [blade.from, (blade.from + 1.0) / 2.0, 1.0].map(|up| blade.at(held, up));
        let brightness = if blade.quickened { 1.0 } else { TRAIL_DIM };
        let color = hazed.map_or(air, |(haze_color, fog)| air.lerp(haze_color, haze(fog, eye.translation().distance(place[2]))));
        follow(&mut trail, now, place, brightness, TRAIL_LIFE, visible.get(), color, &mut drawn);
    }
}

/// A trail noting where its blade is now (`place`: its base, middle and tip), dropping what's
/// older than `life` (seconds), and drawn while it shows, `brightness` at most, `color`.
#[allow(clippy::too_many_arguments)]
fn follow(trail: &mut Trail, now: f32, place: [Vec3; 3], brightness: f32, life: f32, visible: bool, color: Vec3, drawn: &mut Drawn) {
    // A jump (through a waygate, or a respawn) leaves no trail across the map behind it.
    if trail.samples.back().is_some_and(|(_, last)| last[0].distance(place[0]) > arena_shared::config::JUMP) {
        trail.samples.clear();
    }
    trail.samples.push_back((now, place));
    while trail.samples.front().is_some_and(|(at, _)| now - at > life) {
        trail.samples.pop_front();
    }
    let samples = trail.samples.make_contiguous();
    // How bright it is at each sample: by how fast the tip was moving there, and how fresh.
    let alpha = |i: usize| {
        let (at, place) = samples[i];
        let speed = match i {
            0 => 0.0,
            _ => place[2].distance(samples[i - 1].1[2]) / (at - samples[i - 1].0).max(1e-4),
        };
        let fast = ((speed - TRAIL_SPEED.0) / (TRAIL_SPEED.1 - TRAIL_SPEED.0)).clamp(0.0, 1.0);
        let fresh = 1.0 - (now - at) / life;
        (fast * fresh * fresh * brightness).min(1.0)
    };
    if visible && (0..samples.len()).any(|i| alpha(i) > 0.01) {
        let places: Vec<(f32, [Vec3; 3])> = (0..samples.len()).map(|i| (alpha(i), samples[i].1)).collect();
        trail_ribbon(&places, color, drawn);
    }
}
