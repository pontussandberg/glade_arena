//! The revenant's look: a fallen paladin in a long coat, a pauldron and manacles, and the
//! glaive it drags.

use super::*;

use crate::cloth::{Column, Drape, Wardrobe, Weave};
use crate::sculpt::{Section, bevel_box, grid, hemmed, sheet, smoothed, sweep, tube};

/// The revenant's body, a fallen paladin's, lean and quick, under its moving cloth
/// (`revenant_wardrobe`): a fitted long coat, its bodice closed down the front with iron clasps
/// between dark lapels, a high collar flaring open at the throat, a wide belt (the coat's tails
/// hang from under it), and a single pauldron of blackened steel on the left shoulder.
pub(super) fn revenant_body() -> Mesh {
    let bodice = tube(
        &smoothed(
            &[
                ring(0.0, 0.74, (0.145, 0.175), palette::ROBE),
                ring(0.005, 0.88, (0.147, 0.177), palette::ROBE),
                ring(0.015, 1.02, (0.153, 0.19), palette::ROBE).folded(0.015, 7),
                ring(0.02, 1.17, (0.155, 0.215), palette::ROBE).squared(2.2),
                // The shoulders: squared out over the shoulder joints (the sleeves come out from
                // under them), sloping up to the collar.
                ring(0.01, 1.27, (0.145, 0.27), palette::ROBE).squared(2.6),
                ring(0.0, 1.34, (0.13, 0.31), palette::ROBE).squared(3.2),
                ring(-0.01, 1.4, (0.11, 0.23), palette::ROBE).squared(2.6),
                ring(-0.01, 1.47, (0.08, 0.1), palette::TATTERS),
            ],
            3,
        ),
        44,
        (true, true),
    );
    let mut parts = vec![bodice];
    // The lapels: dark bands from the collar down to the waist, meeting in a V.
    for side in [1.0, -1.0] {
        let lapel = [(0.105, 1.4, 0.065, 0.026), (0.158, 1.25, 0.05, 0.03), (0.172, 1.08, 0.022, 0.022), (0.162, 0.92, 0.008, 0.014)]
            .map(|(x, y, z, w)| Section::new(Vec3::new(x, y, z * side), Vec2::new(0.006, w), palette::TATTERS));
        parts.push(tube(&smoothed(&lapel, 3), 8, (true, true)));
    }
    // Iron clasps down the closure.
    for y in [0.97, 1.05, 1.13] {
        let depth = 0.153 + 0.02 * (y - 0.88) / 0.3;
        parts.push(bevel_box(Vec3::new(0.012, 0.022, 0.05), 0.004, palette::IRON).translated_by(Vec3::new(depth + 0.02, y, 0.0)));
    }
    // The collar: standing high round the neck, flaring out as it rises, open at the throat.
    const OPEN: f32 = 0.5;
    let collar_rows: Vec<Vec<Vec3>> = (0..=6)
        .map(|i| i as f32 / 6.0)
        .map(|v| {
            (0..=24)
                .map(|j| OPEN + j as f32 / 24.0 * (std::f32::consts::TAU - 2.0 * OPEN))
                .map(|a| Vec3::new(-0.01 + a.cos() * (0.1 + 0.05 * v * v), 1.39 + 0.17 * v, a.sin() * (0.118 + 0.05 * v * v)))
                .collect()
        })
        .collect();
    parts.push(sheet(&collar_rows, &[palette::ROBE, palette::ROBE, palette::ROBE, palette::ROBE, palette::TATTERS, palette::TATTERS], 0.012));
    // The belt, over the top of the coat's tails, its iron buckle a little off center.
    parts.push(belt(0.005, (0.825, 0.895), (0.168, 0.198), palette::DARK_LEATHER));
    parts.push(bevel_box(Vec3::new(0.025, 0.06, 0.065), 0.008, palette::IRON).translated_by(Vec3::new(0.172, 0.86, 0.04)));
    parts.push(shoulder_plates(-1.0));
    sculpted(parts)
}

/// Small plates of blackened steel over the `side` (±1, +Z the right) shoulder: a broad cap and
/// two lames below it, each curving from the front of the shoulder over it to the back,
/// overlapping down the arm.
fn shoulder_plates(side: f32) -> Mesh {
    let mut parts = Vec::new();
    // Each plate (from, to: how far out along the shoulder), sloping down with the shoulder and
    // overlapping the next, the cap the broadest.
    // Arched clear over the coat's squared shoulder (front to back and over its top).
    for (i, (from, to)) in [(0.2, 0.31), (0.28, 0.35), (0.325, 0.385)].into_iter().enumerate() {
        let shrink = 1.0 - 0.06 * i as f32;
        let rows: Vec<Vec<Vec3>> = (0..=4)
            .map(|r| from + (to - from) * r as f32 / 4.0)
            .map(|z| {
                // Out along the shoulder it drops and curls in a little tighter.
                let down = 0.55 * (z - 0.2) + 0.015 * i as f32;
                let (reach, rise) = (0.178 * shrink - 0.05 * (z - 0.2), 0.122 * shrink - 0.04 * (z - 0.2));
                (0..=14)
                    .map(|k| -1.2 + 2.4 * k as f32 / 14.0)
                    .map(|t| Vec3::new(reach * t.sin(), 1.33 - down + rise * t.cos(), z * side))
                    .collect()
            })
            .collect();
        parts.push(sheet(&rows, &[palette::BLACK_STEEL], 0.014));
    }
    // Stacked from the lowest up, each over the one below.
    sculpted(parts.into_iter().rev().collect())
}

/// What glows on the revenant's body: the brooch at its collar, a long diamond of wisp light with
/// a smaller one either side.
pub(super) fn revenant_glow() -> Mesh {
    let at = Vec3::new(0.15, 1.31, 0.0);
    let mut parts = vec![tinted(gem_mesh(0.024).scaled_by(Vec3::new(0.45, 1.8, 1.0)).translated_by(at), Color::WHITE)];
    for side in [1.0, -1.0] {
        let small = gem_mesh(0.013).scaled_by(Vec3::new(0.45, 1.4, 1.0)).rotated_by(Quat::from_rotation_x(side * 0.5));
        parts.push(tinted(small.translated_by(at + Vec3::new(-0.004, -0.008, side * 0.034)), Color::WHITE));
    }
    sculpted(parts)
}

/// Where one tail of the revenant's coat is: `side` (+1 right, -1 left), `u` (0..1) from its front
/// edge round to the split down the back, `v` (0..1) from the waist down to the hem (at the knee in
/// front, sweeping down to the ground behind); `ragged` for the cloth itself (folds and the torn
/// hem), not for the chains it hangs from.
fn coat_tail_point(side: f32, u: f32, v: f32, ragged: bool) -> Vec3 {
    // Open wide at the front, this far (radians) either side of it, and split down the back.
    const OPEN: f32 = 0.55;
    const SPLIT: f32 = 0.05;
    let span = std::f32::consts::PI - SPLIT - OPEN;
    let a = if side > 0.0 { OPEN + u * span } else { std::f32::consts::TAU - OPEN - u * span };
    let flare = v.powf(1.3);
    let (depth, width) = (0.19 + 0.12 * flare, 0.226 + 0.1 * flare);
    let hem = 0.07 + 0.36 * ((1.0 + a.cos()) / 2.0).powf(1.4);
    let mut y = 0.87 + (hem - 0.87) * v;
    let mut out = 1.0;
    if ragged {
        out += v * 0.06 * (a * 7.0 + v * 1.6).sin() * (0.6 + 0.4 * (a * 2.7).sin()) + 0.02 * (a * 5.0 + v * 6.0).sin();
        let scallop = 0.5 + 0.5 * (a * 6.0 + 0.4).sin();
        let strip = (a * 15.0 + 1.1).sin().max(0.0).powi(5);
        y -= 0.17 * (0.3 * scallop + 0.7 * strip) * v.powi(8);
    }
    Vec3::new(a.cos() * depth * out, y, a.sin() * width * out)
}

/// The revenant's moving cloth: the two long tails of its coat, split up the back and open wide
/// at the front, sweeping down behind it, their torn hems smouldering in wisp light; a tattered
/// tabard down the front.
pub(super) fn revenant_wardrobe() -> Wardrobe {
    const ROWS: usize = 18;
    const COLS: usize = 22;
    let tail = |side: f32| {
        let point = |u, v| coat_tail_point(side, u, v, true);
        let chains = (0..4).map(|c| [0.06, 0.3, 0.53, 0.77, 1.0].map(|v| coat_tail_point(side, c as f32 / 3.0, v, false)).to_vec()).collect();
        Drape::new(
            sheet(&grid(ROWS, COLS, (0.0, 1.0), point), &hemmed(ROWS, 3, palette::ROBE, palette::TATTERS), 0.01),
            Some(sheet(&grid(2, COLS, (0.975, 1.0), point), &[Color::WHITE], 0.02)),
            chains,
            // No motes off its hem: its glaive's rasp is the spectral light it gives off.
            Weave { hold: (6.0, 1.2), damping: 5.5, air: 0.35, inertia: 0.4, gravity: 6.5, flutter: 0.7, joined: true, wisps: 0.0 },
        )
    };

    // The tabard: down the front, curving round the legs, its end torn.
    let tabard = tabard(Vec3::new(0.21, 0.84, 0.0), Vec3::new(0.1, -0.6, 0.0), (0.065, 0.055), 1.0, (0.07, 2.3), &[palette::ASH]);

    Wardrobe {
        drapes: vec![tail(1.0), tail(-1.0), tabard],
        trim: palette::WISP,
        body: vec![
            Column { center: Vec2::ZERO, half: Vec2::new(0.17, 0.205), from: 0.3, to: 0.9 },
            Column { center: Vec2::ZERO, half: Vec2::new(0.2, 0.25), from: 0.9, to: 1.4 },
        ],
        leg_radius: 0.1,
        arm: None,
    }
}

/// The revenant's skull, from the neck up (in the head's space): (height, forward, half depth,
/// half width).
const REVENANT_SKULL: [(f32, f32, f32, f32); 9] = [
    (-0.04, 0.0, 0.05, 0.048),
    (0.04, 0.005, 0.058, 0.052),
    (0.09, 0.02, 0.085, 0.07),
    (0.15, 0.012, 0.104, 0.085),
    (0.21, 0.0, 0.112, 0.092),
    (0.27, -0.012, 0.108, 0.09),
    (0.32, -0.022, 0.085, 0.072),
    (0.355, -0.03, 0.04, 0.035),
    (0.365, -0.032, 0.0, 0.0),
];

/// The skull's section at height `y`: (forward, half depth, half width).
fn revenant_skull_at(y: f32) -> (f32, f32, f32) {
    let s = REVENANT_SKULL;
    let k = s.windows(2).position(|w| y <= w[1].0).unwrap_or(s.len() - 2);
    let (a, b) = (s[k], s[k + 1]);
    let t = ((y - a.0) / (b.0 - a.0)).clamp(0.0, 1.0);
    (a.1 + (b.1 - a.1) * t, a.2 + (b.2 - a.2) * t, a.3 + (b.3 - a.3) * t)
}

/// A band of cloth wrapped once round the head: centered `height(t)` and `half(t)` high at `t`
/// radians round from the front, `grow` times the skull's size there.
fn head_wrap(height: impl Fn(f32) -> f32, half: impl Fn(f32) -> f32, grow: f32, color: Color) -> Mesh {
    let band: Vec<Section> = (0..=36)
        .map(|i| i as f32 / 36.0 * std::f32::consts::TAU)
        .map(|t| {
            let y = height(t);
            let (x, depth, width) = revenant_skull_at(y);
            let at = Vec3::new(x + depth * grow * t.cos(), y, width * grow * t.sin());
            Section::new(at, Vec2::new(0.007, half(t)), color).folded(0.05, 3)
        })
        .collect();
    tube(&band, 8, (true, true))
}

/// The revenant's head, wrapped all over in a faded bandana but for a dark slit its eyes burn
/// white in: the crown wound in layered bands, a wrap round the brow (lower at the front) and
/// another over the jaw and mouth, tied in a knot behind.
pub(super) fn revenant_head() -> Mesh {
    let cloth = palette::SHROUD;
    let fold = cloth.darker(0.04);
    // The face, dark, up to the brow (the wrapping covers the rest).
    let skull = tube(
        &smoothed(&REVENANT_SKULL[..6].iter().map(|&(y, x, depth, width)| ring(x, y, (depth, width), palette::TATTERS).exact()).collect::<Vec<_>>(), 2),
        28,
        (true, true),
    );
    // The crown, under the bands: the skull's top, wrapped close.
    let crown = tube(
        &smoothed(&REVENANT_SKULL[4..].iter().map(|&(y, x, depth, width)| ring(x, y, (depth * 1.07, width * 1.07), cloth).folded(0.02, 7)).collect::<Vec<_>>(), 2),
        28,
        (true, true),
    );
    let mut parts = vec![skull, crown];
    // Bands wound round the crown, each tilted its own way, overlapping.
    for (i, (y, tilt, side)) in [(0.262, 0.022, 0.012), (0.3, -0.02, -0.015)].into_iter().enumerate() {
        let color = if i % 2 == 0 { fold } else { cloth };
        parts.push(head_wrap(move |t| y + tilt * t.cos() + side * t.sin(), |_| 0.022, 1.1, color));
    }
    // Round the brow, just above the eyes at the front, wider behind to meet the jaw wrap.
    parts.push(head_wrap(|t| 0.226 - 0.045 * (1.0 - t.cos()) / 2.0, |t| 0.022 + 0.05 * (1.0 - t.cos()) / 2.0, 1.1, cloth));
    // Over the jaw and mouth, up to just under the eyes.
    parts.push(head_wrap(|_| 0.1, |_| 0.052, 1.08, fold));
    parts.push(head_wrap(|t| 0.13 - 0.02 * (1.0 - t.cos()) / 2.0, |_| 0.024, 1.13, cloth));
    // The knot behind: a bulge of cloth, its two cut ends just poking out below it.
    let knot = Vec3::new(-0.13, 0.215, 0.0);
    parts.push(tinted(ball(1.0).scaled_by(Vec3::new(0.036, 0.04, 0.05)).translated_by(knot), cloth));
    for side in [1.0, -1.0] {
        let end = [knot + Vec3::new(-0.01, -0.01, side * 0.018), knot + Vec3::new(-0.035, -0.045, side * 0.03), knot + Vec3::new(-0.045, -0.075, side * 0.036)];
        parts.push(sweep(&end, &[0.016, 0.012, 0.0], 6, fold));
    }
    sculpted(parts)
}

/// Where the revenant's eyes burn, in the slit its wrappings leave.
pub(super) const REVENANT_EYES: Vec3 = Vec3::new(0.108, 0.175, 0.04);

/// A fitted coat sleeve starting under the coat's shoulder and widening a little to a turned-back,
/// tattered cuff, over a dark leather
/// gauntlet closing into a fist, an iron manacle round the wrist trailing a broken chain.
pub(super) fn revenant_arm() -> Mesh {
    let sleeve = tube(
        &smoothed(
            &[
                ring(0.0, 0.0, (0.058, 0.06), palette::ROBE),
                ring(0.0, -0.1, (0.064, 0.066), palette::ROBE).folded(0.02, 6),
                ring(0.0, -0.21, (0.07, 0.07), palette::ROBE).folded(0.035, 6),
                ring(0.0, -0.27, (0.078, 0.078), palette::ROBE).folded(0.04, 6),
                ring(-0.004, -0.28, (0.09, 0.09), palette::TATTERS).folded(0.03, 6),
                ring(-0.008, -0.34, (0.096, 0.096), palette::TATTERS).torn(0.05).trailing(0.04).folded(0.04, 6),
            ],
            2,
        ),
        22,
        (true, true),
    );
    let gauntlet = tube(
        &[
            ring(0.0, -0.2, (0.062, 0.062), palette::DARK_LEATHER),
            ring(0.0, -0.36, (0.07, 0.07), palette::DARK_LEATHER),
            ring(0.0, -0.42, (0.078, 0.074), palette::DARK_LEATHER.darker(0.05)),
            ring(0.0, -0.44, (0.06, 0.058), palette::DARK_LEATHER),
        ],
        20,
        (true, false),
    );
    let manacle = tube(
        &[
            ring(0.0, -0.35, (0.08, 0.08), palette::IRON).exact(),
            ring(0.0, -0.36, (0.085, 0.085), palette::IRON).exact(),
            ring(0.0, -0.39, (0.085, 0.085), palette::IRON).exact(),
            ring(0.0, -0.4, (0.08, 0.08), palette::IRON).exact(),
        ],
        20,
        (true, true),
    );
    let mut parts = vec![sleeve, gauntlet, fist(palette::DARK_LEATHER, -0.44, 0.064), manacle];
    // The broken chain, hanging from the back of the manacle.
    for (i, y) in [-0.41, -0.445, -0.48].into_iter().enumerate() {
        parts.push(chain_link(Vec3::new(-0.092, y, 0.0), i % 2 == 1));
    }
    sculpted(parts)
}

/// The revenant's glaive, gripped at the origin, blade up (+Y): a long dark shaft, bound where
/// the hand holds it, with a spiked butt behind the hand and a steel collar at its head, and a
/// dark single-edged blade making up the outer half: a broad cleaver's, its edge (+X, leading the
/// blow) straight and parallel to its spine, squared off at the end, a hooked spike on its spine. A thin line
/// of wisp light runs along the edge (`glaive_glow`).
pub(super) fn glaive() -> Mesh {
    let blade = faceted(tube(
        &smoothed(&GLAIVE.map(|(y, x, depth, width)| Section::new(Vec3::new(x, y, 0.0), Vec2::new(depth, width), palette::BLACK_STEEL).squared(1.0).exact()), 1),
        4,
        (true, true),
    ));
    let mut parts = vec![blade];
    // The shaft, from the butt to the collar.
    parts.push(faceted(tube(
        &[
            Section::round(Vec3::Y * GLAIVE_BUTT, 0.02, palette::HUNTER_DARK).exact(),
            Section::round(Vec3::Y * (BLADE_START - 0.06), 0.019, palette::HUNTER_DARK).exact(),
        ],
        8,
        (false, false),
    )));
    // Bound where the hand holds it.
    for i in 0..8 {
        let (from, to) = (-0.16 + 0.04 * i as f32, -0.16 + 0.04 * (i + 1) as f32);
        let (r, color) = if i % 2 == 0 { (0.026, palette::ASH) } else { (0.024, palette::DARK_LEATHER) };
        parts.push(band(from, to, r, 10, color));
    }
    // The steel collar the blade sits in, flaring up to it.
    parts.push(faceted(tube(
        &[
            Section::round(Vec3::Y * (BLADE_START - 0.12), 0.021, palette::IRON).exact(),
            Section::round(Vec3::Y * (BLADE_START - 0.07), 0.03, palette::IRON).exact(),
            Section::round(Vec3::Y * (BLADE_START - 0.03), 0.026, palette::IRON).exact(),
            Section::new(Vec3::Y * BLADE_START, Vec2::new(0.034, 0.02), palette::IRON).exact(),
        ],
        6,
        (true, false),
    )));
    // The spiked butt.
    parts.push(faceted(tube(
        &[
            Section::round(Vec3::Y * (GLAIVE_BUTT + 0.02), 0.026, palette::IRON).exact(),
            Section::round(Vec3::Y * GLAIVE_BUTT, 0.028, palette::IRON).exact(),
            Section::round(Vec3::Y * (GLAIVE_BUTT - 0.13), 0.0, palette::IRON).exact(),
        ],
        4,
        (true, false),
    )));
    // The horns at the blade's top corners, the edge's (+X) the taller, leaning out a little, so
    // the top between them is hollowed.
    let (edge, spine) = (GLAIVE[GLAIVE.len() - 1].1 + GLAIVE[GLAIVE.len() - 1].2, GLAIVE[GLAIVE.len() - 1].1 - GLAIVE[GLAIVE.len() - 1].2);
    for (from, to, base) in [(edge - 0.028, Vec3::new(edge + 0.012, BLADE_LENGTH, 0.0), 0.028), (spine + 0.024, Vec3::new(spine - 0.01, GLAIVE_TOP + 0.065, 0.0), 0.024)] {
        let root = Vec3::new(from, GLAIVE_TOP - 0.03, 0.0);
        let horn: Vec<Vec3> = (0..=3).map(|i| root.lerp(to, i as f32 / 3.0) + Vec3::X * (to.x - from).signum() * 0.006 * (i as f32 * 1.1).sin()).collect();
        parts.push(faceted(sweep(&horn, &[base, base * 0.7, base * 0.35, 0.0], 4, palette::BLACK_STEEL)).scaled_by(Vec3::new(1.0, 1.0, 0.4)));
    }
    // The hooked spike on the spine, near the blade's root, curling up.
    let hook: Vec<Vec3> = (0..=5).map(|i| i as f32 / 5.0).map(|t| Vec3::new(-0.03 - 0.15 * t, BLADE_START + 0.08 + 0.13 * t * t, 0.0)).collect();
    parts.push(faceted(sweep(&hook, &[0.016, 0.014, 0.011, 0.008, 0.004, 0.0], 4, palette::BLACK_STEEL)));
    sculpted(parts)
}

/// Where the glaive's butt ends (below the hand), before its spike.
const GLAIVE_BUTT: f32 = -0.3;
/// Where the glaive's blade starts, above the hand.
pub const BLADE_START: f32 = 0.72;

/// The glaive's blade, as (height, how far its middle is set toward the edge (+X), half its
/// breadth (edge to spine), half its thickness): long (about half the whole glaive) and broad, a
/// cleaver's: stepping out from the collar to a straight spine and a straight edge running side
/// by side, squared off at its end (`GLAIVE_TOP`), where a horn rises from each corner, the
/// edge's taller: a top hollowed between them.
const GLAIVE: [(f32, f32, f32, f32); 8] = [
    (BLADE_START, 0.0, 0.034, 0.017),
    (0.78, 0.048, 0.082, 0.015),
    (0.95, 0.052, 0.086, 0.014),
    (1.25, 0.052, 0.086, 0.013),
    (1.5, 0.052, 0.086, 0.012),
    (1.68, 0.052, 0.086, 0.011),
    (1.76, 0.052, 0.086, 0.01),
    (GLAIVE_TOP, 0.052, 0.086, 0.01),
];

/// Where the glaive's blade is squared off, between the horns at its corners (`glaive`).
const GLAIVE_TOP: f32 = 1.79;

/// A point on the glaive's edge, `t` of the way (0..1) from the root of its blade to its point,
/// in the glaive's own space (as `glaive`).
pub fn glaive_edge(t: f32) -> Vec3 {
    let y = BLADE_START + (BLADE_LENGTH - BLADE_START) * t.clamp(0.0, 1.0);
    let k = GLAIVE.windows(2).position(|w| y <= w[1].0).unwrap_or(GLAIVE.len() - 2);
    let (a, b) = (GLAIVE[k], GLAIVE[k + 1]);
    let u = ((y - a.0) / (b.0 - a.0)).clamp(0.0, 1.0);
    Vec3::new(a.1 + (b.1 - a.1) * u + a.2 + (b.2 - a.2) * u, y, 0.0)
}

/// How long the revenant's glaive is, from the grip to its point.
pub const BLADE_LENGTH: f32 = 1.88;

/// What glows on the revenant's glaive: a thin line of wisp light along its edge, on both faces,
/// from the root of the blade to its point.
pub(super) fn glaive_glow() -> Mesh {
    let mut parts = Vec::new();
    for side in [1.0, -1.0] {
        let points: Vec<Vec3> = (0..=10)
            .map(|i| GLAIVE[1].0 + (GLAIVE_TOP - 0.01 - GLAIVE[1].0) * i as f32 / 10.0)
            .map(|y| {
                // Just in from the edge at this height (between `GLAIVE`'s sections).
                let k = GLAIVE.windows(2).position(|w| y <= w[1].0).unwrap_or(0);
                let (a, b) = (GLAIVE[k], GLAIVE[k + 1]);
                let t = (y - a.0) / (b.0 - a.0);
                let (x, depth, width) = (a.1 + (b.1 - a.1) * t, a.2 + (b.2 - a.2) * t, a.3 + (b.3 - a.3) * t);
                // Two thirds of the way out to the edge, on the face (the faces close in to it).
                Vec3::new(x + depth * 0.66, y, side * (width * 0.34 + 0.002))
            })
            .collect();
        let radii: Vec<f32> = (0..=10).map(|i| if i == 10 { 0.0 } else { 0.0045 }).collect();
        parts.push(sweep(&points, &radii, 5, Color::WHITE));
    }
    sculpted(parts)
}

/// Cracks of light split through the revenant's glaive's blade, on both its faces (in its
/// space): from its edge in toward its spine, angular, zigzagging and narrowing as they go, some
/// forking, like the tear the blade leaves in the ground. Glowing.
pub(super) fn glaive_cracks() -> Mesh {
    let mut b = FlatMesh::default();
    let r = |k: u32| crate::swish::noise(0x6C61_7665, k);
    for c in 0..GLAIVE_CRACKS {
        let r = |k: u32| r(c * 64 + k);
        // Spread along the blade, a little uneven.
        let y = BLADE_START + 0.12 + (GLAIVE_TOP - BLADE_START - 0.2) * (c as f32 + 0.25 + 0.5 * r(0)) / GLAIVE_CRACKS as f32;
        let (edge, spine, _) = blade_at(y);
        let breadth = edge - spine;
        // Zigzagging in from the edge: each step turned off straight in, one way then the other.
        let steps = 3 + (r(1) * 3.0) as u32;
        let reach = breadth * (0.5 + 0.4 * r(2));
        let mut points = vec![Vec2::new(edge - 0.003, y)];
        for k in 0..steps {
            let side = if k % 2 == 0 { 1.0 } else { -1.0 } * if r(3) < 0.5 { 1.0 } else { -1.0 };
            let turn = side * (0.25 + 0.4 * r(4 + k));
            let step = reach / steps as f32 / turn.cos();
            let last = *points.last().unwrap();
            points.push(last + Vec2::new(-turn.cos(), turn.sin()) * step);
        }
        let wide = |k: f32| CRACK_WIDTH * (1.0 - 0.8 * k / steps as f32);
        let mut segments: Vec<(Vec2, Vec2, f32, f32)> = points.windows(2).enumerate().map(|(k, w)| (w[0], w[1], wide(k as f32), wide(k as f32 + 1.0))).collect();
        // A fork off its middle, thinner, turned the other way.
        if r(20) < 0.7 {
            let k = steps as usize / 2;
            let from = points[k];
            let turn = -(points[k + 1] - from).to_angle().signum() * (0.5 + 0.4 * r(21));
            let to = from + Vec2::new(-turn.cos(), turn.sin()) * breadth * (0.15 + 0.15 * r(22));
            segments.push((from, to, wide(k as f32) * 0.6, 0.0));
        }
        for (from, to, w0, w1) in segments {
            let across = (to - from).perp().normalize_or(Vec2::Y);
            for face in [1.0, -1.0] {
                // Just off the face, wherever it is along the blade.
                let on = |p: Vec2| Vec3::new(p.x, p.y, face * (blade_at(p.y).2 + 0.0015));
                let corners = [on(from - across * w0 / 2.0), on(from + across * w0 / 2.0), on(to + across * w1 / 2.0), on(to - across * w1 / 2.0)];
                // Seen from either side (whichever way round its corners go).
                b.quad(corners, Color::WHITE);
                b.quad([corners[0], corners[3], corners[2], corners[1]], Color::WHITE);
            }
        }
    }
    b.build()
}

/// How many cracks split the empowered glaive's blade (`glaive_cracks`), and how wide each is
/// where it opens at the edge (meters).
const GLAIVE_CRACKS: u32 = 7;
const CRACK_WIDTH: f32 = 0.024;

/// The glaive's blade at height `y` (`GLAIVE`, between its sections): where its edge and its
/// spine are (x) and half its thickness.
fn blade_at(y: f32) -> (f32, f32, f32) {
    let k = GLAIVE.windows(2).position(|w| y <= w[1].0).unwrap_or(GLAIVE.len() - 2);
    let (a, b) = (GLAIVE[k], GLAIVE[k + 1]);
    let t = ((y - a.0) / (b.0 - a.0)).clamp(0.0, 1.0);
    let (x, depth, width) = (a.1 + (b.1 - a.1) * t, a.2 + (b.2 - a.2) * t, a.3 + (b.3 - a.3) * t);
    (x + depth, x - depth, width)
}
