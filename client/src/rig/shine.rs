//! A weapon's light: a gleam catching along a blade's edge now and then (`gleam_blades`), and
//! its aura while its attack's `follow_up` is armed (`show_auras`).

use super::*;

use crate::Me;
use crate::feedback::AttackClock;
use crate::render::shown;

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
pub(super) struct Gleaming {
    pub(super) next: f32,
    pub(super) from: f32,
    pub(super) at: Vec3,
    pub(super) peak: f32,
    pub(super) seed: u32,
}

/// Every couple of seconds a gleam catches somewhere along a held blade's edge, a star of
/// light swelling to a random brightness and going out again, each blade on its own time.
pub(super) fn gleam_blades(time: Res<Time>, mut rigs: Query<&mut Rig>, mut parts: Query<(&mut Visibility, &mut Transform)>) {
    let now = time.elapsed_secs();
    for mut rig in &mut rigs {
        let Some((gleam, edge)) = rig.gleam else { continue };
        let g = &mut rig.gleaming;
        let mut random = || {
            g.seed = g.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (g.seed >> 8) as f32 / (1 << 24) as f32
        };
        if g.next == 0.0 {
            g.next = now + random() * GLEAM_EVERY.1;
        }
        if now >= g.next {
            let (along, peak, wait, side) = (random(), random(), random(), random());
            g.from = now;
            // On either edge of a blade sharp both sides (mirrored across it; nothing changes for
            // an edge in its middle).
            g.at = edge(0.1 + 0.85 * along) * if side < 0.5 { Vec3::new(1.0, 1.0, -1.0) } else { Vec3::ONE };
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

/// A weapon's aura (cracks of light through a blade, a spearhead's glow) shows while the attack's
/// `follow_up` is armed or quickening the windup in progress, or the next attack is primed
/// (Deadeye), embers flaking off a blade's edge (`swish::shed_embers`).
/// A blade's trail brightens with them (`swish::Blade::quickened`).
pub(super) fn show_auras(
    clock: Option<AttackClock>,
    mut fighters: Query<(&Rig, &AttackState, Has<Me>, Option<&mut crate::swish::Blade>)>,
    mut parts: Query<&mut Visibility>,
) {
    for (rig, attack, is_me, blade) in &mut fighters {
        // Hidden without a synced clock (e.g. after a disconnect): no telling when the window ends.
        let on = attack.primed.is_some() || clock.as_ref().is_some_and(|clock| attack.empowered_at(clock.now(is_me)));
        if let Some(mut blade) = blade
            && blade.quickened != on
        {
            blade.quickened = on;
        }
        if let Some(mut visibility) = rig.aura.and_then(|aura| parts.get_mut(aura).ok()) {
            visibility.set_if_neq(shown(on));
        }
    }
}
