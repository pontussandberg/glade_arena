//! The javelinist's look: a one-armed death hunter in a hooded coat, a pale bandana sewn
//! shut over the mouth, a knife at the belt, and the headsman's spear he carries and throws.

use super::*;

/// The executioner's spear, standing along +Y with its point at the origin, carried and thrown
/// alike: a headsman's blade on a pole. A long, broad, straight blade narrowing to a cruel point,
/// flat in Z (so it shows from above as it flies), its edges ground bright (`metal`) either side
/// of a thick dark midrib (`dark`); two hooked ears at its base curling forward like horns; a long
/// socket with langets riveted down the shaft, a rag (`rag`) knotted below them, its two ends
/// streaming back; a dark shaft (`shaft`), faceted like worked wood, bound in `wrap` where it's
/// gripped between two rings; and a heavy butt spike.
pub(super) fn javelin(shaft: Color, metal: Color, dark: Color, wrap: Color, rag: Color) -> Mesh {
    const LENGTH: f32 = SPEAR_LENGTH;
    const HEAD: f32 = 0.62;
    // The blade: (how far back from the point, half its thickness, half its breadth).
    let blade = [(HEAD, 0.016, 0.028), (0.58, 0.018, 0.062), (0.5, 0.018, 0.084), (0.36, 0.017, 0.08), (0.2, 0.014, 0.06), (0.07, 0.008, 0.03), (0.0, 0.0, 0.0)];
    let edges = faceted(tube(
        &smoothed(&blade.map(|(back, depth, width)| Section::new(Vec3::Y * -back, Vec2::new(depth, width), metal).squared(1.0).exact()), 2),
        4,
        (true, false),
    ));
    // The midrib: thicker than the blade and narrower, so only the ground edges show bright.
    let rib = faceted(tube(
        &smoothed(
            &blade[..blade.len() - 1].iter().map(|&(back, depth, width)| Section::new(Vec3::Y * -(back + 0.012), Vec2::new(depth * 1.5, width * 0.62), dark).squared(1.0).exact()).collect::<Vec<_>>(),
            2,
        ),
        4,
        (true, true),
    ));
    let mut parts = vec![edges, rib];
    // The ears: out from the blade's base and curling forward along it, flat like the blade.
    for side in [1.0, -1.0] {
        let ear = [(HEAD - 0.03, 0.03), (HEAD + 0.01, 0.085), (HEAD - 0.02, 0.14), (HEAD - 0.1, 0.165), (HEAD - 0.2, 0.158)]
            .map(|(back, out)| Vec3::new(0.0, -back, side * out));
        parts.push(faceted(sweep(&ear, &[0.02, 0.017, 0.013, 0.008, 0.0], 4, dark)).scaled_by(Vec3::new(0.55, 1.0, 1.0)));
    }
    // The socket, ridged where it takes the blade and where it grips the shaft.
    let socket = [(HEAD - 0.01, 0.024), (HEAD + 0.02, 0.034), (HEAD + 0.05, 0.028), (HEAD + 0.13, 0.027), (HEAD + 0.15, 0.033), (HEAD + 0.17, 0.028)];
    parts.push(faceted(tube(&socket.map(|(back, r)| Section::round(Vec3::Y * -back, r, dark).exact()), 8, (true, true))));
    // Langets: iron strips down two sides of the shaft from the socket, each held by two rivets.
    for side in [1.0, -1.0] {
        let strip = bevel_box(Vec3::new(0.008, 0.26, 0.022), 0.003, dark);
        parts.push(strip.translated_by(Vec3::new(side * 0.026, -(HEAD + 0.29), 0.0)));
        for at in [HEAD + 0.24, HEAD + 0.38] {
            parts.push(tinted(ball(0.007).translated_by(Vec3::new(side * 0.031, -at, 0.0)), metal));
        }
    }
    // The rag knotted below the langets, its two ends streaming back along the shaft.
    let knot = HEAD + 0.45;
    parts.push(tinted(ball(1.0).scaled_by(Vec3::new(0.034, 0.03, 0.036)).translated_by(Vec3::new(0.012, -knot, 0.0)), rag));
    for (side, length) in [(1.0, 0.36), (-1.0, 0.27)] {
        let end: Vec<Vec3> = (0..=4).map(|i| i as f32 / 4.0).map(|t| Vec3::new(0.03 + 0.03 * t, -(knot + length * t), side * (0.012 + 0.02 * t + 0.012 * (t * 7.0).sin()))).collect();
        parts.push(sweep(&end, &[0.018, 0.02, 0.019, 0.016, 0.0], 5, rag).scaled_by(Vec3::new(0.45, 1.0, 1.0)));
    }
    let wood = faceted(tube(
        &[(HEAD + 0.12, 0.024), (0.95, 0.024), (1.5, 0.023), (LENGTH - 0.12, 0.022)]
            .map(|(back, r)| Section::round(Vec3::Y * -back, r, shaft).squared(2.2)),
        8,
        (false, false),
    ));
    parts.push(wood);
    // The grip: bound between two rings.
    parts.push(band(-1.1, -1.13, 0.03, 10, dark));
    for i in 0..7 {
        let at = 1.14 + 0.026 * i as f32;
        parts.push(band(-at, -(at + 0.022), 0.027, 10, wrap));
    }
    parts.push(band(-1.33, -1.36, 0.03, 10, dark));
    // The butt: a heavy ferrule and a long spike.
    parts.push(faceted(tube(
        &[(LENGTH - 0.2, 0.025), (LENGTH - 0.17, 0.031), (LENGTH - 0.12, 0.028), (LENGTH - 0.1, 0.022), (LENGTH, 0.0)]
            .map(|(back, r)| Section::round(Vec3::Y * -back, r, dark).exact()),
        4,
        (true, false),
    )));
    sculpted(parts)
}
/// The javelinist's longer arm: where its elbow and hand are instead (`rig::Moves::elbow_at`).
pub const JAVELINIST_ELBOW: Vec3 = Vec3::new(0.0, -0.34, 0.0);
pub const JAVELINIST_HAND: Vec3 = Vec3::new(0.0, -0.7, 0.0);

/// The spear in hand, gripped a little behind its middle.
pub(super) fn held_spear() -> Mesh {
    carried_javelin().translated_by(Vec3::Y * GRIP_TO_TIP)
}

/// The executioner's spear in its own colors, point at the origin.
pub(super) fn carried_javelin() -> Mesh {
    javelin(palette::HUNTER_DARK, palette::SILVER, palette::BLACK_STEEL, palette::DARK_LEATHER, palette::OXBLOOD)
}

/// A point `t` (0..1, from the base of its blade to its point) along an edge of the javelin
/// held (`held_spear`), in the spear's space: its right one (the left mirrors it across Z).
pub fn spear_edge(t: f32) -> Vec3 {
    // How far back from the point along the blade, and how broad it is there (`javelin`).
    const EDGE: [(f32, f32); 6] = [(0.58, 0.062), (0.5, 0.084), (0.36, 0.08), (0.2, 0.06), (0.07, 0.03), (0.0, 0.0)];
    let back = EDGE[0].0 * (1.0 - t.clamp(0.0, 1.0));
    let k = EDGE.windows(2).position(|w| back >= w[1].0).unwrap_or(EDGE.len() - 2);
    let (a, b) = (EDGE[k], EDGE[k + 1]);
    let u = ((a.0 - back) / (a.0 - b.0)).clamp(0.0, 1.0);
    Vec3::new(0.0, GRIP_TO_TIP - back, a.1 + (b.1 - a.1) * u)
}

/// The executioner's body, under its moving cloth (`javelinist_wardrobe`): upright and squared,
/// a fitted, hooded coat over an ashen undertunic showing at the knee, its hood's cowl lying over
/// the shoulders and gathered round the neck (the hood itself is on the head); a leather harness
/// slung across the chest from the right shoulder, ringed in iron; a wide belt over the coat's
/// top with an iron buckle, a sheathed knife hung from it at the left hip (`hip_knife`). Few
/// details: the hood and the blade overhead are the figure.
pub(super) fn javelinist_body() -> Mesh {
    const FOLDS: u32 = 9;
    let tunic = tube(
        &smoothed(
            &[
                ring(0.0, 0.42, (0.15, 0.185), palette::HUNTER_ASH).torn(0.05).folded(0.05, FOLDS),
                ring(0.0, 0.6, (0.145, 0.18), palette::HUNTER_ASH).folded(0.03, FOLDS),
                ring(0.0, 0.84, (0.145, 0.18), palette::HUNTER),
                ring(0.01, 0.98, (0.152, 0.192), palette::HUNTER),
                ring(0.018, 1.13, (0.16, 0.225), palette::HUNTER).squared(2.4),
                ring(0.005, 1.3, (0.145, 0.25), palette::HUNTER).squared(2.8),
                ring(-0.01, 1.41, (0.11, 0.19), palette::HUNTER).squared(2.2),
                ring(-0.01, 1.49, (0.075, 0.1), palette::HUNTER_DARK),
                ring(-0.01, 1.57, (0.07, 0.095), palette::HUNTER_DARK),
            ],
            3,
        ),
        40,
        (true, true),
    );
    let mut parts = vec![tunic];
    // The harness: a strap round the chest, high on the right shoulder and low at the left
    // hip, an iron ring where it crosses the breast.
    let strap: Vec<Section> = (0..=40)
        .map(|i| i as f32 / 40.0 * std::f32::consts::TAU)
        .map(|t| {
            let z = t.sin();
            let y = 1.06 + 0.2 * z;
            let grow = 1.07 + 0.04 * z.max(0.0);
            let at = Vec3::new(0.012 + 0.158 * grow * t.cos(), y, 0.205 * grow * z);
            Section::new(at, Vec2::new(0.007, 0.024), palette::DARK_LEATHER)
        })
        .collect();
    parts.push(tube(&strap, 6, (true, true)));
    let ring_at = Vec3::new(0.186, 1.08, 0.03);
    let iron_ring: Vec<Vec3> = (0..=12).map(|i| i as f32 / 12.0 * std::f32::consts::TAU).map(|a| ring_at + Vec3::new(0.0, a.cos() * 0.03, a.sin() * 0.03)).collect();
    parts.push(sweep(&iron_ring, &[0.007; 13], 6, palette::IRON));
    // The belt goes over the coat's top (the coat hangs from under it).
    parts.push(belt(0.01, (0.84, 0.95), (0.183, 0.223), palette::DARK_LEATHER));
    parts.push(bevel_box(Vec3::new(0.03, 0.08, 0.075), 0.01, palette::IRON).translated_by(Vec3::new(0.2, 0.895, 0.0)));
    // A long knife in a leather sheath, hung from the belt at the left front hip.
    parts.push(hip_knife());
    // The coat's hood, down off the head: gathered loose round the neck, its lip rolled; the
    // hood itself (on the head, `javelinist_hood`) rises out of it, and the rest of it lies over
    // the shoulders as moving cloth (`javelinist_mantle`).
    parts.push(tube(
        &smoothed(
            &[
                ring(-0.016, 1.4, (0.155, 0.19), palette::HUNTER),
                ring(-0.016, 1.46, (0.165, 0.2), palette::HUNTER).folded(0.03, 5),
                ring(-0.016, 1.505, (0.157, 0.172), palette::HUNTER).folded(0.032, 5),
                ring(-0.016, 1.545, (0.145, 0.158), palette::HUNTER_DARK),
            ],
            3,
        ),
        40,
        (true, true),
    ));
    sculpted(parts)
}

/// A long knife sheathed in dark leather, hung straight down from the executioner's belt at the
/// left front on a short loop: an iron throat and tip on the sheath, a leather-bound grip over a
/// short iron guard, an iron pommel.
pub(super) fn hip_knife() -> Mesh {
    // Built hanging down (-Y) from the top of its sheath, flat across X, thin through Z.
    let (leather, iron) = (palette::DARK_LEATHER, palette::IRON);
    let sheath = tube(
        &[
            ring(0.0, 0.0, (0.03, 0.012), leather),
            ring(0.0, -0.09, (0.032, 0.013), leather),
            ring(0.0, -0.2, (0.026, 0.011), leather),
            ring(0.0, -0.26, (0.016, 0.009), leather),
            ring(0.0, -0.29, (0.006, 0.005), leather),
        ],
        12,
        (true, true),
    );
    let band = |y: f32, half: (f32, f32)| tube(&[ring(0.0, y, half, iron).exact(), ring(0.0, y - 0.016, half, iron).exact()], 12, (true, true));
    let mut parts = vec![sheath, band(0.004, (0.034, 0.015)), band(-0.25, (0.02, 0.012))];
    parts.push(bevel_box(Vec3::new(0.075, 0.012, 0.03), 0.003, iron).translated_by(Vec3::Y * 0.012));
    parts.push(tube(&[ring(0.0, 0.018, (0.013, 0.011), palette::LEATHER), ring(0.0, 0.1, (0.012, 0.01), palette::LEATHER)], 10, (true, true)));
    for y in [0.04, 0.065, 0.09] {
        parts.push(tube(&[ring(0.0, y, (0.015, 0.013), leather).exact(), ring(0.0, y - 0.006, (0.015, 0.013), leather).exact()], 10, (true, true)));
    }
    parts.push(tinted(ball(1.0).scaled_by(Vec3::new(0.02, 0.016, 0.017)).translated_by(Vec3::Y * 0.113), iron));
    let knife = sculpted(parts);
    // Hung straight down at the belt's left front, in the coat's opening (clear of its skirt),
    // flat to the belt.
    let angle: f32 = -0.25;
    let at = Vec3::new(0.01 + 0.2 * angle.cos(), 0.82, 0.24 * angle.sin());
    let out = Vec3::new(angle.cos() / 0.2, 0.0, angle.sin() / 0.24).normalize();
    let up = Vec3::Y;
    let flat = (out - up * out.dot(up)).normalize();
    let across = up.cross(flat);
    let turn = Quat::from_mat3(&Mat3::from_cols(across, up, flat));
    // The loop it hangs by, from the belt down to the sheath's throat.
    let belt_at = Vec3::new(0.01 + 0.19 * angle.cos(), 0.86, 0.23 * angle.sin());
    let loop_strap = sweep(&[belt_at, at + flat * 0.004], &[0.012, 0.012], 6, leather);
    sculpted(vec![knife.rotated_by(turn).translated_by(at), loop_strap])
}

/// How high the coat's skirt starts, up under the belt.
pub(super) const COAT_TOP: f32 = 0.94;

/// Where the coat's skirt is, at `u` (0..1) round it from its right front edge, round the back, to
/// its left front edge, and `v` (0..1) from the belt down to the hem; `ragged` for the cloth
/// itself (folds, unevenness and the torn hem), not for the chains it hangs from.
pub(super) fn coat_point(u: f32, v: f32, ragged: bool) -> Vec3 {
    // Open at the front, this far (radians) either side of it.
    const OPEN: f32 = 0.45;
    let a = OPEN + u * (std::f32::consts::TAU - 2.0 * OPEN);
    // From under the belt (tucked in inside it), flaring out below it.
    let flare = v.powf(1.25);
    let (depth, width) = (0.158 + 0.164 * flare, 0.198 + 0.164 * flare);
    // The hem: just off the ground behind, a little higher at the front.
    let hem = 0.035 + 0.075 * (1.0 + a.cos()) / 2.0;
    let mut y = COAT_TOP + (hem - COAT_TOP) * v;
    let mut out = 1.0;
    if ragged {
        // Its folds, from below the belt down.
        let below = (v / 0.15).clamp(0.0, 1.0);
        out += v * 0.06 * (a * 8.0 + v * 1.8).sin() * (0.6 + 0.4 * (a * 3.1).sin()) + 0.02 * below * (a * 5.0 + v * 7.0).sin();
        let scallop = 0.5 + 0.5 * (a * 5.0 + 0.8).sin();
        let strip = (a * 13.0 + 2.0).sin().max(0.0).powi(6);
        y -= 0.1 * (0.35 * scallop + 0.65 * strip) * v.powi(8);
    }
    Vec3::new(a.cos() * depth * out, y, a.sin() * width * out)
}

/// How far apart (radians, round the cowl) the two sides of its slit under the right arm are at
/// rest: just apart, so each side moves as its own (`javelinist_mantle`).
pub(super) const MANTLE_SLIT: f32 = 0.05;
/// How thick the arm is to the cowl lying on it (`javelinist_mantle`).
pub(super) const MANTLE_ARM: f32 = 0.08;
/// How far down the arm (from the shoulder) the cowl can lie on it: over the top of the arm, not
/// up along it when it's raised.
pub(super) const MANTLE_ON_ARM: f32 = 0.2;

/// A point on the cowl over the executioner's shoulders, `u` (0..1) round it from one side of
/// its slit under the right arm (`MANTLE_SLIT`) round the front and back to the other, `v`
/// (0..1) from where it hangs from round the neck out over the shoulders down to its hem:
/// close to the shoulders, just clear of the arm (`MANTLE_ARM`) on the right, rounded over it,
/// and closer still down over the empty left shoulder, in soft folds deepening toward the hem;
/// `ragged` for the cloth itself (folds and its cut hem), not for the chains it hangs from.
pub(super) fn mantle_point(u: f32, v: f32, ragged: bool) -> Vec3 {
    // (v, depth, width, height) down it: from round the neck, over the top of the shoulder, down
    // past it to the hem; over the arm, and over the empty side.
    type Profile = [(f32, f32, f32, f32); 4];
    const OVER_ARM: Profile = [(0.0, 0.16, 0.19, 1.48), (0.3, 0.178, 0.29, 1.435), (0.6, 0.192, 0.385, 1.36), (1.0, 0.198, 0.405, 1.19)];
    const EMPTY: Profile = [(0.0, 0.16, 0.19, 1.48), (0.3, 0.175, 0.26, 1.42), (0.6, 0.188, 0.295, 1.33), (1.0, 0.193, 0.315, 1.19)];
    let at = |profile: &Profile| {
        let i = profile.iter().rposition(|p| p.0 <= v).unwrap_or(0).min(profile.len() - 2);
        let (a, b) = (profile[i], profile[i + 1]);
        let t = ((v - a.0) / (b.0 - a.0)).clamp(0.0, 1.0);
        Vec3::new(a.1 + (b.1 - a.1) * t, a.2 + (b.2 - a.2) * t, a.3 + (b.3 - a.3) * t)
    };
    let angle = std::f32::consts::FRAC_PI_2 + MANTLE_SLIT + u * (std::f32::consts::TAU - 2.0 * MANTLE_SLIT);
    // Over the empty left shoulder (-Z), easing in across the front and back.
    let empty = (-angle.sin() / 0.5).clamp(0.0, 1.0);
    let Vec3 { x: depth, y: width, z: y } = at(&OVER_ARM).lerp(at(&EMPTY), crate::ease::smooth(empty));
    // Rounded over the shoulders, a little fuller than an ellipse.
    let square = |c: f32| c.signum() * c.abs().powf(2.0 / 2.15);
    let (mut out, mut y) = (1.0, y);
    if ragged {
        out += 0.035 * v * (angle * 7.0 + 0.4).sin();
        y -= 0.03 * v.powi(6) * (angle * 9.0).sin().abs();
    }
    Vec3::new(-0.016 + square(angle.cos()) * depth * out, y, square(angle.sin()) * width * out)
}

/// The hood's cowl lying over the executioner's shoulders, as moving cloth: heavy, lying on the
/// throwing arm (`Wardrobe::arm`), slit under it from the shoulder down, so raised it parts the
/// cloth, falling in front of and behind it.
pub(super) fn javelinist_mantle() -> Drape {
    const ROWS: usize = 18;
    const COLS: usize = 96;
    // As many as a skin can have joints (`cloth::MAX_JOINTS`), one per chain point.
    const CHAINS: usize = 30;
    let point = |u, v| mantle_point(u, v, true);
    let cloth = sheet(&grid(ROWS, COLS, (0.0, 1.0), point), &hemmed(ROWS, 2, palette::HUNTER, palette::HUNTER_DARK), 0.022);
    let chains: Vec<Vec<Vec3>> = (0..=CHAINS).map(|c| [0.0, 0.14, 0.28, 0.42, 0.56, 0.7, 0.85, 1.0].map(|v| mantle_point(c as f32 / CHAINS as f32, v, false)).to_vec()).collect();
    // Thick cotton, snug round the shoulders: firm round the neck, falling free toward the hem
    // when the arm parts it, hardly swinging as the body turns, settling quickly.
    Drape::new(cloth, None, chains, Weave { hold: (8.0, 2.5), damping: 8.0, air: 0.1, inertia: 0.05, gravity: 8.0, flutter: 0.05, joined: true, wisps: 0.0 })
}

/// All the executioner's moving cloth: the coat skirt and the cowl (`javelinist_mantle`).
pub(super) fn javelinist_wardrobe() -> Wardrobe {
    const ROWS: usize = 18;
    const COLS: usize = 64;
    let point = |u, v| coat_point(u, v, true);
    let coat = sheet(&grid(ROWS, COLS, (0.0, 1.0), point), &hemmed(ROWS, 3, palette::HUNTER, palette::HUNTER_DARK), 0.024);
    // The glowing hem, wrapped round the cloth's last few centimeters.
    let trim = sheet(&grid(2, COLS, (0.975, 1.0), point), &[Color::WHITE], 0.034);
    let coat_chains: Vec<Vec<Vec3>> = (0..=8).map(|c| [0.07, 0.3, 0.53, 0.77, 1.0].map(|v| coat_point(c as f32 / 8.0, v, false)).to_vec()).collect();
    let coat = Drape::new(
        coat,
        Some(trim),
        coat_chains,
        // Heavy cloth: hanging straight, swinging slow and settling, hardly drifting.
        Weave { hold: (7.0, 2.2), damping: 7.5, air: 0.15, inertia: 0.28, gravity: 10.0, flutter: 0.15, joined: true, wisps: 5.0 },
    );

    let drapes = vec![coat, javelinist_mantle()];

    Wardrobe {
        drapes,
        trim: palette::SOUL,
        body: vec![
            Column { center: Vec2::ZERO, half: Vec2::new(0.152, 0.192), from: 0.3, to: 1.05 },
            // The chest and shoulders, under the cowl.
            Column { center: Vec2::new(-0.01, 0.0), half: Vec2::new(0.15, 0.25), from: 1.05, to: 1.45 },
        ],
        leg_radius: 0.125,
        // The cowl lies on the top of the arm.
        arm: Some((MANTLE_ARM, MANTLE_ON_ARM)),
    }
}

/// Fitted sleeves, creased above the elbow, down to it, rounded there so the arm shows no gap
/// as it bends (the forearm is `javelinist_forearm`).
pub(super) fn javelinist_arm() -> Mesh {
    let sleeve = tube(
        &smoothed(
            &[
                ring(-0.01, 0.06, (0.0, 0.0), palette::HUNTER),
                ring(-0.01, 0.05, (0.05, 0.055), palette::HUNTER),
                ring(0.0, 0.0, (0.07, 0.072), palette::HUNTER),
                ring(0.0, -0.15, (0.068, 0.07), palette::HUNTER).folded(0.03, 6),
                ring(0.0, -0.27, (0.066, 0.066), palette::HUNTER).folded(0.04, 6),
                ring(0.0, JAVELINIST_ELBOW.y, (0.064, 0.064), palette::HUNTER),
            ],
            2,
        ),
        24,
        (true, true),
    );
    sculpted(vec![sleeve, tinted(ball(0.064).translated_by(JAVELINIST_ELBOW), palette::HUNTER)])
}

/// The forearm, from the elbow (`JAVELINIST_ELBOW`): the sleeve's creased end into a long
/// leather bracer strapped tight over a dark-gloved hand (at `JAVELINIST_HAND`).
pub(super) fn javelinist_forearm() -> Mesh {
    let (elbow, wrist) = (JAVELINIST_ELBOW.y, JAVELINIST_HAND.y + 0.12);
    let sleeve = tube(
        &[
            ring(0.0, elbow, (0.064, 0.064), palette::HUNTER).folded(0.04, 6),
            ring(0.0, elbow - 0.015, (0.066, 0.066), palette::HUNTER),
        ],
        24,
        (true, true),
    );
    let bracer = tube(
        &[
            ring(0.0, elbow - 0.005, (0.062, 0.062), palette::DARK_LEATHER),
            ring(0.0, elbow - 0.02, (0.072, 0.072), palette::DARK_LEATHER),
            ring(0.0, wrist, (0.068, 0.066), palette::DARK_LEATHER),
            ring(0.0, wrist - 0.02, (0.06, 0.058), palette::DARK_LEATHER),
        ],
        20,
        (true, false),
    );
    // The fist at the hand (`fist` draws it at `RIG_HAND`).
    let hand = fist(palette::DARK_LEATHER, RIG_HAND.y + 0.12, 0.056).translated_by(JAVELINIST_HAND - RIG_HAND);
    let mut parts = vec![sleeve, bracer, hand];
    // Straps round the bracer, each with a small iron buckle on the outside.
    for y in [elbow - 0.03, elbow - 0.1, elbow - 0.17] {
        parts.push(tube(&[ring(0.0, y, (0.078, 0.078), palette::LEATHER).exact(), ring(0.0, y - 0.022, (0.076, 0.076), palette::LEATHER).exact()], 20, (true, true)));
        parts.push(bevel_box(Vec3::new(0.012, 0.026, 0.026), 0.004, palette::IRON).translated_by(Vec3::new(0.0, y - 0.011, -0.078)));
    }
    sculpted(parts).translated_by(-JAVELINIST_ELBOW)
}

/// How far in toward the body the empty left sleeve hangs from where a shoulder would be.
pub(super) const STUMP_IN: f32 = 0.06;

/// The empty left sleeve of a one-armed man: no shoulder in it, hanging limp and flattened out
/// from under the coat's cowl, gathered in and tied off in a knot, its two ends hanging.
pub(super) fn javelinist_stump() -> Mesh {
    let cloth = palette::HUNTER;
    // Flat against the side, wide front to back and thin across, just cloth.
    let sleeve = tube(
        &smoothed(
            &[
                ring(0.0, -0.02, (0.05, 0.012), cloth),
                ring(0.005, -0.1, (0.058, 0.014), cloth).folded(0.02, 6),
                ring(0.01, -0.2, (0.052, 0.013), cloth).folded(0.03, 6),
                ring(0.012, -0.27, (0.04, 0.012), cloth).folded(0.02, 5),
                ring(0.012, -0.31, (0.026, 0.01), cloth),
            ],
            2,
        ),
        24,
        (true, true),
    );
    let knot = Vec3::new(0.012, -0.322, 0.0);
    let mut parts = vec![sleeve, tinted(ball(1.0).scaled_by(Vec3::new(0.03, 0.024, 0.016)).translated_by(knot), cloth.darker(0.04))];
    // The ends, short and flat: one hanging a little forward, one a little back.
    for (end, color) in [(Vec3::new(0.025, -0.09, 0.0), cloth), (Vec3::new(-0.02, -0.065, 0.0), cloth.darker(0.06))] {
        let strip = sweep(&[Vec3::ZERO, end * 0.5 + Vec3::X * end.x * 0.3, end], &[0.014, 0.011, 0.003], 6, color);
        parts.push(strip.scaled_by(Vec3::new(1.0, 1.0, 0.4)).translated_by(knot));
    }
    // In against the side (in from where a shoulder would be), under the cowl.
    sculpted(parts).translated_by(Vec3::Z * STUMP_IN)
}

/// The thigh, in close trousers, down to the knee (the shin is `javelinist_shin`): seen at the
/// coat's opening as it walks.
pub(super) fn javelinist_leg() -> Mesh {
    let cloth = palette::HUNTER_DARK;
    let thigh = tube(
        &[
            ring(0.0, 0.05, (0.09, 0.09), cloth),
            ring(0.0, -0.2, (0.095, 0.09), cloth),
            ring(0.01, RIG_KNEE.y, (0.088, 0.084), cloth),
        ],
        16,
        (true, true),
    );
    // The knee, rounded so the leg shows no gap as it bends.
    sculpted(vec![thigh, tinted(ball(0.086).translated_by(RIG_KNEE + Vec3::X * 0.01), cloth)])
}

/// The shin, from the knee (`RIG_KNEE`): close trousers into a soft, wrapped boot.
pub(super) fn javelinist_shin() -> Mesh {
    let (cloth, boot) = (palette::HUNTER_DARK, palette::DARK_LEATHER);
    let (knee, kneecap) = (0.088, 0.086);
    tube(
        &[
            ring(0.01, RIG_KNEE.y + 0.02, (kneecap, kneecap * 0.96), cloth),
            ring(0.0, -0.55, (knee * 0.92, knee * 0.88), cloth),
            ring(0.0, -0.6, (0.078, 0.076), boot),
            ring(0.015, -0.7, (0.085, 0.078), boot),
            ring(0.04, -0.76, (0.11, 0.082), boot).squared(3.0),
            ring(0.05, -0.8, (0.115, 0.08), boot).squared(3.0),
        ],
        16,
        (true, true),
    )
    .translated_by(-RIG_KNEE)
}

/// The middle of the death hunter's head (in the head's space, the neck at the origin, facing
/// +X), and its half sizes: how deep, how tall, how wide.
pub(super) const SKULL: Vec3 = Vec3::new(0.0, 0.2, 0.0);
pub(super) const SKULL_HALF: Vec3 = Vec3::new(0.135, 0.165, 0.11);
/// Where its eyes are, on the face, in shadow under the hood's brow.
pub(super) const HUNTER_EYES: Vec3 = Vec3::new(0.128, 0.198, 0.043);

/// A point `out` off the skull, `theta` round it from the front (radians, toward +Z), `phi` up
/// from its middle: rounder behind, swelling at the back of the crown.
pub(super) fn on_skull(theta: f32, phi: f32, out: f32) -> Vec3 {
    let back = (-theta.cos()).max(0.0) * phi.cos();
    let half = SKULL_HALF + Vec3::new(0.012, 0.0, 0.004) * back;
    SKULL + Vec3::new((half.x + out) * phi.cos() * theta.cos(), (half.y + out) * phi.sin(), (half.z + out) * phi.cos() * theta.sin())
}

/// How far (radians either side of the front) the hood is open at `phi` up the skull: shut over
/// the crown, round the face from the brow down to the jaw, then closing in under the bandana.
pub(super) fn hood_open(phi: f32) -> f32 {
    let brow = 1.0 - ((phi - 0.32) / 0.22).clamp(0.0, 1.0);
    let chin = ((phi + 1.15) / 0.4).clamp(0.0, 1.0);
    0.95 * crate::ease::smooth(brow) * (0.55 + 0.45 * chin)
}

/// How far up the skull (`phi`) the hood is at `v` (0..1) down it: over the crown and down to the
/// jaw (`v` up to `HOOD_JAW`), below that tucked down into the coat's cowl.
pub(super) const HOOD_JAW: f32 = 0.78;
pub(super) const HOOD_PHI: (f32, f32) = (std::f32::consts::FRAC_PI_2, -0.8);

/// A point on the death hunter's hood, `u` (-1..1) round the head from the left side of its
/// opening, round the back, to the right side, `v` (0..1) from the crown down into the coat's
/// cowl: close over the skull, softly creased, a short point at the back of the crown, its brow
/// pulled forward over the eyes.
pub(super) fn hood_point(u: f32, v: f32) -> Vec3 {
    let jaw = (v / HOOD_JAW).min(1.0);
    let phi = HOOD_PHI.0 + (HOOD_PHI.1 - HOOD_PHI.0) * jaw;
    let open = hood_open(phi);
    let theta = std::f32::consts::PI + u * (std::f32::consts::PI - open);
    let edge = ((u.abs() - 0.8) / 0.2).clamp(0.0, 1.0);
    let brow = (-((phi - 0.3) / 0.22).powi(2)).exp();
    let folds = 0.008 * (theta * 4.0 + phi * 2.0).sin() * (0.4 + 0.6 * v);
    let point = 0.03 * (-((theta.cos() + 1.0) / 0.08)).exp() * (-((phi - 0.75) / 0.25).powi(2)).exp();
    let mut at = on_skull(theta, phi, 0.032 + 0.018 * edge + folds + point);
    // The brow: pulled forward and down over the face, dipping to a point at its middle.
    let middle = (-(theta.sin() / 0.3).powi(2)).exp();
    at += (Vec3::new(0.03, -0.012, 0.0) + Vec3::new(0.012, -0.03, 0.0) * middle) * brow * edge * theta.cos().max(0.0);
    if v <= HOOD_JAW {
        return at;
    }
    // Below the jaw: down into the coat's cowl round the neck (`javelinist_body`), tucked in.
    let s = (v - HOOD_JAW) / (1.0 - HOOD_JAW);
    let tucked = Vec3::new(-0.016 + 0.134 * theta.cos(), 0.05, 0.148 * theta.sin());
    at.lerp(tucked, s)
}

/// The death hunter's head: a real skull's shape, its face in shadow but for the eyes, under the
/// coat's hood, close, pulled forward over the brow, its edge rolled, rising out of the cowl
/// round the neck (`javelinist_body`); a bandana of pale, faded cloth (`hunter_bandana`) over the nose and mouth,
/// tucked in under the hood's sides.
pub(super) fn javelinist_hood() -> Mesh {
    const ROWS: usize = 28;
    const COLS: usize = 56;
    let cloth = palette::HUNTER;
    let face = palette::HUNTER_DARK.mix(&palette::BONE, 0.12);
    let mut head = vec![tinted(ball(1.0).scaled_by(SKULL_HALF).translated_by(SKULL), face)];
    // A hard brow, the bridge of the nose, and the eyes sunk in dark hollows under it.
    let brow = HUNTER_EYES + Vec3::new(-0.012, 0.022, 0.0);
    head.push(sweep(&[brow.with_z(-0.06), brow.with_z(0.0) + Vec3::X * 0.008, brow.with_z(0.06)], &[0.012, 0.016, 0.012], 8, face));
    head.push(tinted(ball(1.0).scaled_by(Vec3::new(0.02, 0.035, 0.014)).translated_by(Vec3::new(0.133, 0.17, 0.0)), face));
    for side in [1.0, -1.0] {
        let hollow = ball(1.0).scaled_by(Vec3::new(0.012, 0.016, 0.026)).translated_by(HUNTER_EYES * Vec3::new(1.0, 1.0, side) - Vec3::X * 0.003);
        head.push(tinted(hollow, palette::INK.darker(0.05)));
    }
    let head = sculpted(head);
    let neck = tube(&[ring(-0.01, -0.06, (0.06, 0.058), face), ring(0.0, 0.08, (0.066, 0.062), face)], 12, (true, true));
    let colors = hemmed(ROWS, 3, cloth, palette::HUNTER_DARK);
    let hood = sheet(&grid(ROWS, COLS, (0.0, 1.0), |u, v| hood_point(u * 2.0 - 1.0, v)), &colors, 0.026);
    // The rolled edge round the face: up one side, over the brow, down the other.
    let rows = 60;
    let side = |u: f32| -> Vec<Vec3> {
        (0..=rows)
            .map(|i| HOOD_JAW * 1.08 * i as f32 / rows as f32)
            .filter(|&v| hood_open(HOOD_PHI.0 + (HOOD_PHI.1 - HOOD_PHI.0) * (v / HOOD_JAW).min(1.0)) > 0.02)
            .map(|v| hood_point(u, v))
            .collect()
    };
    let mut edge: Vec<Vec3> = side(-1.0).into_iter().rev().collect();
    edge.extend(side(1.0));
    let hem = sweep(&edge, &vec![0.018; edge.len()], 8, palette::HUNTER_DARK);
    sculpted(vec![head, neck, hood, hem, hunter_bandana()])
}

/// The bandana's front (in the head's space): its middle, how far it reaches forward and out to
/// either side round the face, and the height of its top edge, just under the eyes.
pub(super) const BANDANA: (f32, f32, f32, f32) = (0.02, 0.13, 0.105, 0.162);

/// How far down the bandana hangs at `u` (-1..1, across the face): a short point in the middle.
pub(super) fn bandana_drop(u: f32) -> f32 {
    0.07 + 0.025 * (1.0 - u.abs()).powf(1.8)
}

/// A point on the bandana, `u` (-1..1) across the face from its left to its right, `v` (0..1) down
/// from its top edge: drawn up over the bridge of the nose, round the face, falling from the nose
/// in soft folds that deepen as they hang, its lower edge drawn back under the chin, tucked in
/// behind the hood's cowl.
pub(super) fn bandana_point(u: f32, v: f32) -> Vec3 {
    let (middle, forward, out, top) = BANDANA;
    let a = u * 1.35;
    let nose = (-(u / 0.3).powi(2)).exp();
    let point = (1.0 - u.abs()).powf(1.8);
    let folds = 0.005 * v * (u * 11.0 + 1.3).sin() * (0.6 + 0.4 * (u * 4.0).cos()) + 0.004 * v.powi(3) * (u * 23.0).sin() * point;
    let reach = forward + 0.016 * nose * (1.0 - 0.4 * v) - 0.04 * v * v + folds;
    let y = top + 0.01 * nose - bandana_drop(u) * v - 0.008 * v.powi(4) * (u * 17.0).sin().abs();
    Vec3::new(middle + reach * a.cos(), y, out * a.sin() * (1.0 + folds))
}

/// Just off the front of the bandana at `u` across it and height `y`: where its stitches go.
pub(super) fn on_bandana(u: f32, y: f32) -> Vec3 {
    let (middle, _, _, top) = BANDANA;
    let v = ((top - y) / bandana_drop(u)).clamp(0.0, 1.0);
    let p = bandana_point(u, v);
    p + Vec3::new(p.x - middle, 0.0, p.z).normalize() * 0.007
}

/// The bandana over the nose and mouth, in pale, dirty cloth going greyer and darker toward its
/// lower edge, its top edge rolled; the mouth under it sewn shut: one crude seam of dark thread
/// stretched almost from ear to ear, its corners pulled up into a smile too wide, crossed by
/// uneven slanting stitches, old dark stains run down from it in drips; over it the nose two
/// small dark slits.
pub(super) fn hunter_bandana() -> Mesh {
    const ROWS: usize = 16;
    const COLS: usize = 40;
    let cloth = palette::BONE.darker(0.3).mix(&palette::ASH, 0.25);
    let colors: Vec<Color> = (0..ROWS).map(|i| cloth.darker(0.12 * (i as f32 / (ROWS - 1) as f32).powi(2))).collect();
    let mut parts = vec![sheet(&grid(ROWS, COLS, (0.0, 1.0), |u, v| bandana_point(u * 2.0 - 1.0, v)), &colors, 0.007)];
    // The rolled top edge.
    let hem: Vec<Vec3> = (0..=COLS).map(|i| bandana_point(i as f32 / COLS as f32 * 2.0 - 1.0, 0.0) + Vec3::Y * 0.002).collect();
    parts.push(sweep(&hem, &vec![0.0055; hem.len()], 6, cloth.darker(0.06)));
    let (thread, stain) = (palette::TATTERS.darker(0.2), palette::OXBLOOD);
    // Uneven, but the same every time: how far through the stitching (0..1) by `i`.
    let jitter = |i: f32| (i * 12.9898).sin() * 0.5 + 0.5;
    // The seam: almost ear to ear, its corners pulled up.
    let mouth = 0.112;
    let seam = |u: f32| on_bandana(u, mouth + 0.02 * (u / 0.72).powi(2) - 0.002 * (u * 9.0).sin());
    let line: Vec<Vec3> = (0..=24).map(|i| seam(-0.72 + 1.44 * i as f32 / 24.0)).collect();
    parts.push(sweep(&line, &[0.0032; 25], 4, thread));
    // Stitches across it, crude: uneven lengths, slanting this way and that.
    for i in 0..15 {
        let u = -0.66 + 1.32 * i as f32 / 14.0 + 0.015 * (jitter(i as f32) - 0.5);
        let (up, down) = (0.01 + 0.012 * jitter(i as f32 + 3.1), 0.01 + 0.012 * jitter(i as f32 + 7.7));
        let slant = 0.05 * (jitter(i as f32 + 1.3) - 0.5);
        let y = |u: f32| mouth + 0.02 * (u / 0.72).powi(2);
        parts.push(sweep(&[on_bandana(u - slant, y(u) + up), on_bandana(u + slant, y(u) - down)], &[0.0026, 0.0026], 4, thread));
    }
    // Old stains run down from it in drips, of different lengths.
    for (u, length) in [(-0.42, 0.035), (-0.12, 0.06), (0.05, 0.028), (0.3, 0.05), (0.55, 0.03)] {
        let top = mouth + 0.02 * (u / 0.72f32).powi(2) - 0.004;
        let drip: Vec<Vec3> = (0..=4).map(|k| on_bandana(u + 0.006 * (k as f32 * 1.7).sin(), top - length * k as f32 / 4.0)).collect();
        parts.push(sweep(&drip, &[0.007, 0.006, 0.005, 0.004, 0.0], 5, stain));
    }
    // The nose: two small dark slits.
    for side in [1.0, -1.0] {
        parts.push(sweep(&[on_bandana(side * 0.05, mouth + 0.046), on_bandana(side * 0.07, mouth + 0.036)], &[0.004, 0.003], 4, thread));
    }
    sculpted(parts)
}

/// How long the executioner's spear is, from its point to the end of its butt spike.
pub const SPEAR_LENGTH: f32 = 2.05;
