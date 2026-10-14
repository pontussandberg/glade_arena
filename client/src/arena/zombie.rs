//! The zombie's look: a dead villager back on its feet, rotting grey-green skin under a torn,
//! filthy shirt, bare arms reaching out ahead and hands gone to bony claws.

use super::*;

use crate::sculpt::{smoothed, sweep, tube};

/// The zombie's body: a shirt of dirty linen hanging loose and torn below the waist, a frayed
/// rope for a belt, and rotting skin showing at the open neck.
pub(super) fn zombie_body() -> Mesh {
    let (rags, skin) = (palette::RAGS, palette::ROT);
    let shirt = tube(
        &smoothed(
            &[
                ring(0.01, 0.66, (0.17, 0.2), rags).torn(0.08).folded(0.04, 6),
                ring(0.01, 0.8, (0.16, 0.19), rags).folded(0.03, 6),
                ring(0.02, 1.0, (0.15, 0.2), rags).folded(0.02, 5),
                ring(0.02, 1.18, (0.15, 0.23), rags).squared(2.2),
                ring(0.0, 1.3, (0.14, 0.28), rags).squared(2.8),
                ring(-0.01, 1.37, (0.12, 0.26), rags).squared(2.4),
            ],
            2,
        ),
        32,
        (true, true),
    );
    // The open neck: the skin of the chest and throat, up into the head.
    let neck = tube(&[ring(0.0, 1.34, (0.11, 0.15), skin), ring(0.0, 1.41, (0.085, 0.1), skin), ring(0.0, 1.47, (0.06, 0.065), skin)], 18, (true, true));
    // Dark rot and old wounds on the chest, where the shirt's torn open.
    let wound = tinted(ball(1.0).scaled_by(Vec3::new(0.03, 0.07, 0.05)).translated_by(Vec3::new(0.15, 1.1, -0.06)), palette::ROT_DARK);
    let rope = belt(0.01, (0.83, 0.87), (0.162, 0.192), palette::ROPE);
    sculpted(vec![shirt, neck, wound, rope])
}

/// A bald, lumpy head on a thin neck, its jaw hanging slack and open, the eyes sunk in dark
/// sockets.
pub(super) fn zombie_head() -> Mesh {
    let (skin, rot) = (palette::ROT, palette::ROT_DARK);
    let skull = tube(
        &smoothed(
            &[
                ring(0.0, -0.04, (0.055, 0.055), skin),
                ring(0.01, 0.03, (0.07, 0.07), skin),
                // The slack jaw, hanging forward.
                ring(0.04, 0.08, (0.1, 0.085), rot),
                ring(0.02, 0.14, (0.115, 0.095), skin),
                ring(0.0, 0.21, (0.125, 0.105), skin),
                ring(-0.01, 0.28, (0.115, 0.1), skin),
                ring(-0.02, 0.33, (0.075, 0.065), skin),
                ring(-0.02, 0.355, (0.0, 0.0), skin),
            ],
            2,
        ),
        24,
        (true, true),
    );
    let mut parts = vec![skull];
    // The open mouth.
    parts.push(tinted(ball(1.0).scaled_by(Vec3::new(0.03, 0.035, 0.05)).translated_by(Vec3::new(0.125, 0.1, 0.0)), palette::DARK_LEATHER));
    // The sockets, and lumps of rot on the scalp.
    for side in [1.0, -1.0] {
        parts.push(tinted(ball(1.0).scaled_by(Vec3::new(0.02, 0.026, 0.032)).translated_by(Vec3::new(0.105, 0.19, side * 0.045)), palette::DARK_LEATHER));
    }
    for (at, size) in [(Vec3::new(-0.02, 0.3, 0.05), 0.035), (Vec3::new(-0.08, 0.24, -0.06), 0.03)] {
        parts.push(tinted(ball(size).translated_by(at), rot));
    }
    sculpted(parts)
}

/// Where the zombie's eyes are, deep in their sockets.
pub(super) const ZOMBIE_EYES: Vec3 = Vec3::new(0.112, 0.19, 0.045);

/// A bare, rotting arm under the torn stump of a sleeve, into a bony hand.
pub(super) fn zombie_arm() -> Mesh {
    let (skin, rot) = (palette::ROT, palette::ROT_DARK);
    let sleeve = tube(
        &[ring(0.0, 0.03, (0.072, 0.072), palette::RAGS), ring(0.0, -0.06, (0.076, 0.076), palette::RAGS), ring(0.0, -0.13, (0.08, 0.08), palette::RAGS).torn(0.05)],
        16,
        (true, true),
    );
    let arm = tube(
        &smoothed(
            &[
                ring(0.0, 0.0, (0.05, 0.05), skin),
                ring(0.0, -0.18, (0.043, 0.045), skin),
                // A dark rotting patch at the elbow.
                ring(0.0, -0.26, (0.047, 0.047), rot),
                ring(0.0, -0.32, (0.04, 0.042), skin),
                ring(0.0, -0.42, (0.034, 0.038), skin),
            ],
            2,
        ),
        14,
        (true, false),
    );
    sculpted(vec![sleeve, arm, fist(skin, -0.42, 0.045)])
}

/// The zombie's claws, from the hand (the grip, at the origin) out along +Y: three long, hooked,
/// yellowed nails.
pub(super) fn zombie_claws() -> Mesh {
    let nail = palette::BONE.darker(0.2);
    sculpted(
        [-1.0, 0.0, 1.0]
            .map(|spread| {
                let points = [Vec3::new(0.0, 0.0, spread * 0.024), Vec3::new(0.008, 0.07, spread * 0.034), Vec3::new(0.035, 0.13, spread * 0.04)];
                sweep(&points, &[0.013, 0.009, 0.0], 6, nail)
            })
            .to_vec(),
    )
}
