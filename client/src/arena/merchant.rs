//! The merchant's look: a stout, travel-worn trader in a long burgundy coat trimmed in gold, a
//! heavy belt hung with coin purses, a wide-brimmed hat with a long feather, and a pack frame on
//! the back piled high with wares (a rolled rug, a crate, a pot, a pan): seen from above, the
//! hat and the pile say who it is at once. In hand, a crooked walking staff with a lantern
//! hanging lit from its crook (`merchant_lantern`, drawn glowing).

use super::*;

use crate::sculpt::{bevel_box, smoothed, sweep, tube};

/// The coat (to the knee, round over the belly), its gold trim, the belt and purses, and the pack.
pub(super) fn merchant_body() -> Mesh {
    let (coat, gold) = (palette::MERCHANT_COAT, palette::GOLD);
    let body = tube(
        &smoothed(
            &[
                ring(0.0, 0.42, (0.25, 0.28), coat).folded(0.02, 7),
                ring(0.01, 0.62, (0.23, 0.26), coat),
                // Round over the belly, out in front (+X).
                ring(0.04, 0.88, (0.25, 0.26), coat),
                ring(0.05, 1.02, (0.25, 0.27), coat),
                ring(0.02, 1.16, (0.22, 0.26), coat).squared(2.2),
                ring(0.0, 1.3, (0.17, 0.29), coat).squared(2.6),
                ring(-0.01, 1.4, (0.12, 0.2), coat),
                ring(-0.01, 1.47, (0.07, 0.08), palette::SKIN),
            ],
            2,
        ),
        30,
        (true, true),
    );
    let mut parts = vec![body];
    // Gold at the hem, and down the front where the coat closes.
    parts.push(belt(0.0, (0.42, 0.47), (0.255, 0.285), gold));
    let placket: Vec<Vec3> = [(0.45, 0.255), (0.7, 0.245), (0.9, 0.29), (1.05, 0.3), (1.2, 0.24), (1.32, 0.17)].iter().map(|&(y, x)| Vec3::new(x, y, 0.0)).collect();
    parts.push(sweep(&placket, &[0.014; 6], 8, gold));
    // A heavy belt with a gold buckle, round the belly.
    parts.push(belt(0.04, (0.86, 0.94), (0.258, 0.27), palette::LEATHER));
    parts.push(tinted(bevel_box(Vec3::new(0.03, 0.09, 0.11), 0.01, gold).translated_by(Vec3::new(0.3, 0.9, 0.0)), gold));
    // Coin purses hanging off the belt, each tied at the neck with gold.
    for at in [Vec3::new(0.16, 0.8, 0.21), Vec3::new(0.06, 0.78, -0.25), Vec3::new(-0.12, 0.8, 0.24)] {
        parts.push(tinted(ball(1.0).scaled_by(Vec3::new(0.06, 0.07, 0.06)).translated_by(at), palette::BARK));
        parts.push(tinted(ball(0.022).translated_by(at + Vec3::Y * 0.065), gold));
    }
    parts.extend(merchant_pack());
    sculpted(parts)
}

/// The pack on the merchant's back (-X): a wooden frame, a canvas bag, and on and round it the
/// wares: a rug rolled across the top, a crate on that, a pot, a frying pan hung off the side.
fn merchant_pack() -> Vec<Mesh> {
    let (wood, canvas) = (palette::BARK, palette::CANVAS);
    let mut parts = Vec::new();
    // The frame: two poles up the back, rising above the head, and cross bars.
    for z in [-0.17, 0.17] {
        parts.push(sweep(&[Vec3::new(-0.27, 0.72, z), Vec3::new(-0.33, 1.3, z), Vec3::new(-0.34, 1.86, z)], &[0.02, 0.02, 0.018], 8, wood));
    }
    for y in [0.86, 1.4] {
        parts.push(sweep(&[Vec3::new(-0.3, y, -0.19), Vec3::new(-0.3, y, 0.19)], &[0.016, 0.016], 8, wood));
    }
    // The bag, bulging, tied shut.
    parts.push(bevel_box(Vec3::new(0.24, 0.52, 0.38), 0.07, canvas).translated_by(Vec3::new(-0.42, 1.12, 0.0)));
    parts.push(belt(-0.42, (1.24, 1.28), (0.125, 0.195), palette::ROPE));
    // A rug rolled across the top, striped.
    let rug = |from: f32, to: f32, color: Color| sweep(&[Vec3::new(-0.4, 1.47, from), Vec3::new(-0.4, 1.47, to)], &[0.085, 0.085], 14, color);
    parts.push(rug(-0.3, 0.3, palette::MERCHANT_RUG));
    for z in [-0.2, 0.0, 0.2] {
        parts.push(rug(z - 0.025, z + 0.025, palette::GOLD.darker(0.25)));
    }
    // A crate on the rug, and a pot beside it.
    parts.push(bevel_box(Vec3::new(0.22, 0.17, 0.24), 0.015, wood.lighter(0.08)).translated_by(Vec3::new(-0.38, 1.64, 0.06)));
    parts.push(tinted(ball(1.0).scaled_by(Vec3::new(0.09, 0.08, 0.09)).translated_by(Vec3::new(-0.36, 1.62, -0.19)), palette::IRON));
    // A frying pan hanging off the frame's side.
    parts.push(tinted(cylinder(0.11, 0.025, 14).rotated_by(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)).translated_by(Vec3::new(-0.46, 1.02, 0.25)), palette::BLACK_STEEL));
    parts.push(sweep(&[Vec3::new(-0.46, 1.13, 0.25), Vec3::new(-0.46, 1.3, 0.23)], &[0.012, 0.01], 6, palette::BLACK_STEEL));
    parts
}

/// A round, ruddy face over a bushy beard, under a wide-brimmed hat with a long feather sweeping
/// back: from above, mostly hat.
pub(super) fn merchant_head() -> Mesh {
    let skin = palette::SKIN;
    let face = tube(
        &smoothed(
            &[
                ring(0.0, -0.04, (0.06, 0.06), skin),
                ring(0.01, 0.05, (0.095, 0.09), skin),
                ring(0.01, 0.15, (0.115, 0.105), skin),
                ring(0.0, 0.24, (0.115, 0.105), skin),
                ring(-0.01, 0.3, (0.1, 0.095), skin),
            ],
            2,
        ),
        22,
        (true, true),
    );
    let mut parts = vec![face];
    // The beard, full round the jaw and down onto the chest.
    parts.push(tinted(ball(1.0).scaled_by(Vec3::new(0.085, 0.12, 0.11)).translated_by(Vec3::new(0.06, 0.04, 0.0)), palette::MERCHANT_BEARD));
    // A big nose, and the eyes in shadow under the brim (`MERCHANT_EYES`).
    parts.push(tinted(ball(0.03).translated_by(Vec3::new(0.125, 0.15, 0.0)), skin.darker(0.08)));
    // The hat: a wide brim, a crown with a gold band, the feather sweeping up and back.
    let hat = palette::MERCHANT_HAT;
    parts.push(tinted(cylinder(0.3, 0.025, 28).translated_by(Vec3::new(0.0, 0.29, 0.0)), hat));
    parts.push(tube(&[ring(0.0, 0.29, (0.13, 0.13), hat), ring(-0.01, 0.42, (0.115, 0.11), hat), ring(-0.02, 0.47, (0.08, 0.075), hat), ring(-0.02, 0.48, (0.0, 0.0), hat)], 20, (true, true)));
    parts.push(belt(0.0, (0.31, 0.34), (0.132, 0.132), palette::GOLD));
    let feather = [Vec3::new(-0.04, 0.36, 0.12), Vec3::new(-0.14, 0.46, 0.15), Vec3::new(-0.28, 0.56, 0.17), Vec3::new(-0.42, 0.6, 0.18)];
    parts.push(sweep(&feather, &[0.02, 0.035, 0.028, 0.0], 8, palette::MERCHANT_FEATHER));
    sculpted(parts)
}

/// Where the merchant's eyes are, in the shade of the brim.
pub(super) const MERCHANT_EYES: Vec3 = Vec3::new(0.108, 0.2, 0.045);

/// A puffed coat sleeve, cuffed in gold, into a leather glove.
pub(super) fn merchant_arm() -> (Mesh, Mesh) {
    let coat = palette::MERCHANT_COAT;
    let sleeve = vec![
        ring(0.0, 0.08, (0.09, 0.09), coat),
        ring(0.0, -0.08, (0.105, 0.105), coat),
        ring(0.0, -0.22, (0.1, 0.1), coat),
        ring(0.0, -0.32, (0.09, 0.09), coat),
        ring(0.0, -0.34, (0.095, 0.095), palette::GOLD),
        ring(0.0, -0.37, (0.095, 0.095), palette::GOLD),
    ];
    jointed(RIG_ELBOW, vec![(sleeve, 16, (true, true))], vec![fist(palette::LEATHER, -0.36, 0.055)])
}

/// The merchant's walking staff, gripped at the origin, standing up (+Y): a knotted shaft curling
/// over at the top into a crook, a lantern hanging off it on a short chain (its glass is
/// `merchant_lantern`, drawn glowing).
pub(super) fn merchant_staff() -> Mesh {
    let wood = palette::BARK.darker(0.1);
    let crook = [Vec3::Y * -0.78, Vec3::Y * 0.4, Vec3::Y * 0.95, Vec3::new(0.05, 1.08, 0.0), Vec3::new(0.15, 1.1, 0.0), Vec3::new(0.21, 1.02, 0.0)];
    let mut parts = vec![sweep(&crook, &[0.024, 0.024, 0.024, 0.022, 0.02, 0.016], 10, wood)];
    // Knots up the shaft.
    for y in [-0.3, 0.15, 0.6] {
        parts.push(tinted(ball(0.03).translated_by(Vec3::Y * y), wood.darker(0.15)));
    }
    // The lantern's chain, its cap and its base (the glass glows: `merchant_lantern`).
    parts.push(sweep(&[Vec3::new(0.21, 1.01, 0.0), Vec3::new(0.21, 0.93, 0.0)], &[0.006, 0.006], 6, palette::IRON));
    parts.push(tinted(cone(0.065, 0.05, 8).translated_by(Vec3::new(0.21, 0.905, 0.0)), palette::BLACK_STEEL));
    parts.push(tinted(cylinder(0.06, 0.02, 8).translated_by(Vec3::new(0.21, 0.76, 0.0)), palette::BLACK_STEEL));
    for corner in 0..4 {
        let a = corner as f32 * std::f32::consts::FRAC_PI_2 + std::f32::consts::FRAC_PI_4;
        let at = Vec3::new(0.21 + a.cos() * 0.05, 0.0, a.sin() * 0.05);
        parts.push(sweep(&[at.with_y(0.77), at.with_y(0.89)], &[0.006, 0.006], 5, palette::BLACK_STEEL));
    }
    sculpted(parts)
}

/// The lantern's glass, lit: drawn glowing (the rig's `held_glow`).
pub(super) fn merchant_lantern() -> Mesh {
    tinted(cylinder(0.045, 0.11, 8).translated_by(Vec3::new(0.21, 0.83, 0.0)), Color::WHITE)
}
