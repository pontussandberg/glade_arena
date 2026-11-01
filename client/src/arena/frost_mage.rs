//! The frost mage's look: robe, rime mantle and crowned hood, the ice staff, and its spells'
//! meshes (frostbolt, nova, ice prison, frost rune).

use super::*;

use crate::cloth::{Column, Drape, Wardrobe, Weave};
use crate::sculpt::{Section, grid, hemmed, sheet, smoothed, sweep, tube};

/// A long crystal standing on Y, base at the origin: a narrow prism ending in a point.
fn ice_shard(radius: f32, height: f32) -> Mesh {
    let body = cylinder(radius, height * 0.7, 5).translated_by(Vec3::Y * height * 0.35);
    let point = cone(radius, height * 0.3, 5).translated_by(Vec3::Y * height * 0.85);
    faceted(sculpted(vec![body, point]))
}

/// The middle of the staff's head (above the grip), and how far its crescents reach out.
const STAFF_HEAD: f32 = 1.32;
const STAFF_REACH: f32 = 0.17;
/// How far around (radians from the bottom of the head) each crescent half curls up.
const STAFF_CURL: f32 = 2.5;
/// The sides the staff's crescent halves curl up on.
const CRESCENTS: [Vec3; 4] = [Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z];

/// A point `t` (0..1) of the way along a crescent half, from the bottom of the staff's head up
/// around `side` to its tip.
fn crescent(side: Vec3, t: f32) -> Vec3 {
    let angle = STAFF_CURL * t;
    Vec3::Y * STAFF_HEAD + (side * angle.sin() - Vec3::Y * angle.cos()) * STAFF_REACH
}

/// The staff's crystals, drawn glowing: a long one floating in the middle of its head, a small
/// one above it, and a sliver growing out of each crescent's tip.
pub(super) fn staff_crystals() -> Mesh {
    let crystal = gem_mesh(0.07).scaled_by(Vec3::new(0.85, 2.7, 0.85));
    let mut parts = vec![
        tinted(crystal.translated_by(Vec3::Y * (STAFF_HEAD + 0.02)), Color::WHITE),
        tinted(ice_shard(0.022, 0.1).translated_by(Vec3::Y * (STAFF_HEAD + 0.25)), Color::WHITE),
    ];
    for side in CRESCENTS {
        let tip = crescent(side, 1.0);
        let out = (side + Vec3::Y).normalize();
        let sliver = ice_shard(0.016, 0.11).rotated_by(Quat::from_rotation_arc(Vec3::Y, out));
        parts.push(tinted(sliver.translated_by(tip), Color::WHITE));
    }
    sculpted(parts)
}

/// What glows on the frost mage's body: ice shards growing out of its mantle, a crystal at its
/// sash, and runes circling the hem of its robe.
pub(super) fn frost_mage_glow() -> Mesh {
    let mut parts = Vec::new();
    for side in [1.0, -1.0] {
        for (i, (out, height)) in [(0.25, 0.22), (0.6, 0.16), (0.95, 0.12)].into_iter().enumerate() {
            let shard = ice_shard(0.03, height).rotated_by(Quat::from_rotation_x(side * out));
            let at = Vec3::new(-0.06 + 0.06 * i as f32, 1.42, side * (0.2 + 0.03 * i as f32));
            parts.push(tinted(shard.translated_by(at), Color::WHITE));
        }
    }
    let gem = gem_mesh(0.045).scaled_by(Vec3::new(0.6, 1.4, 1.0));
    parts.push(tinted(gem.translated_by(Vec3::new(0.225, 0.868, 0.0)), Color::WHITE));
    sculpted(parts)
}

/// Ice locking a rooted fighter in place, around its feet at the origin: a ring of crystals
/// leaning in up to the knees, a few taller ones climbing the robe. Drawn see-through.
pub fn ice_prison_mesh() -> Mesh {
    let mut parts = Vec::new();
    for i in 0..9 {
        let around = i as f32 / 9.0 * std::f32::consts::TAU + 0.3;
        let height = 0.55 + 0.35 * ((i * 5) % 3) as f32 / 2.0;
        let shard = ice_shard(0.09, height).rotated_by(Quat::from_rotation_z(0.28));
        let placed = shard.translated_by(Vec3::new(-0.42, 0.0, 0.0)).rotated_by(Quat::from_rotation_y(around));
        parts.push(placed);
    }
    sculpted(parts)
}

/// A nova's parts, all around its center at the origin (see `render::show_novas`).
#[derive(Clone)]
pub struct NovaMeshes<M = Mesh> {
    /// Frost over the ground it reaches, thickest at the edge.
    pub disc: M,
    /// The shockwave: a band at its edge, fading inward, that races out from the center.
    pub ring: M,
    /// One shard bursting out of the ground, leaning out along +X, 1 m tall.
    pub shard: M,
    /// Ice erupting from where the staff strikes the ground: a spray of tall shards.
    pub eruption: M,
}

pub fn nova_meshes(radius: f32) -> NovaMeshes {
    let steps = 48;
    let point = |i: u32, r: f32| {
        let t = i as f32 / steps as f32 * std::f32::consts::TAU;
        Vec3::new(t.cos() * r, 0.0, -t.sin() * r)
    };
    let mut disc = FlatMesh::default();
    let mut ring = FlatMesh::default();
    for i in 0..steps {
        disc.tri_faded([Vec3::ZERO, point(i, radius), point(i + 1, radius)], [0.25, 1.0, 1.0]);
        let inner = radius * 0.72;
        ring.tri_faded([point(i, inner), point(i, radius), point(i + 1, radius)], [0.0, 1.0, 1.0]);
        ring.tri_faded([point(i, inner), point(i + 1, radius), point(i + 1, inner)], [0.0, 1.0, 0.0]);
    }
    let mut eruption = Vec::new();
    for i in 0..9 {
        let around = i as f32 / 9.0 * std::f32::consts::TAU;
        let height = 0.7 + 0.5 * ((i * 4) % 5) as f32 / 4.0;
        let lean = 0.35 + 0.25 * ((i * 2) % 3) as f32 / 2.0;
        let shard = ice_shard(0.07, height).rotated_by(Quat::from_rotation_z(-lean));
        eruption.push(shard.translated_by(Vec3::X * 0.15).rotated_by(Quat::from_rotation_y(around)));
    }
    NovaMeshes {
        disc: disc.build(),
        ring: ring.build(),
        shard: ice_shard(0.11, 1.0).rotated_by(Quat::from_rotation_z(-0.35)),
        eruption: sculpted(eruption),
    }
}

/// Frost under a slowed fighter's feet, flat on the ground: a thin ring and a six-armed
/// snowflake inside it (each arm branching twice), fading toward the middle.
pub fn frost_rune_mesh() -> Mesh {
    let mut b = FlatMesh::default();
    let at = |angle: f32, r: f32| Vec3::new(angle.cos() * r, 0.0, -angle.sin() * r);
    // The ring.
    let steps = 36;
    let (inner, outer) = (0.56, 0.64);
    for i in 0..steps {
        let (a, z) = (i as f32 / steps as f32 * std::f32::consts::TAU, (i + 1) as f32 / steps as f32 * std::f32::consts::TAU);
        b.tri_faded([at(a, inner), at(a, outer), at(z, outer)], [0.7, 1.0, 1.0]);
        b.tri_faded([at(a, inner), at(z, outer), at(z, inner)], [0.7, 1.0, 0.7]);
    }
    // A thin bar from `from` to `to`, `width` wide, opaque at `to`.
    let mut bar = |from: Vec3, to: Vec3, width: f32, alpha: [f32; 2]| {
        let along = (to - from).normalize();
        let side = Vec3::new(along.z, 0.0, -along.x) * width / 2.0;
        let corners = [from - side, to - side, to + side, from + side];
        b.tri_faded([corners[0], corners[1], corners[2]], [alpha[0], alpha[1], alpha[1]]);
        b.tri_faded([corners[0], corners[2], corners[3]], [alpha[0], alpha[1], alpha[0]]);
    };
    for i in 0..6 {
        let angle = i as f32 / 6.0 * std::f32::consts::TAU;
        bar(at(angle, 0.12), at(angle, 0.5), 0.05, [0.2, 1.0]);
        for (r, length) in [(0.26, 0.1), (0.38, 0.08)] {
            for turn in [0.7, -0.7] {
                let from = at(angle, r);
                let to = from + at(angle + turn, length);
                bar(from, to, 0.035, [0.6, 1.0]);
            }
        }
    }
    b.build()
}

/// The frost mage's body, under its moving cloth (`frost_mage_wardrobe`): a robe from the waist
/// up to a high collar, a blue sash over the skirt's top, and a heavy rime mantle over the
/// shoulders, its edge hanging in short icicles.
pub(super) fn frost_mage_body() -> Mesh {
    let robe = tube(
        &[
            ring(0.01, 0.7, (0.18, 0.22), palette::FROST_ROBE),
            ring(0.01, 0.92, (0.19, 0.23), palette::FROST_ROBE),
            ring(0.01, 1.12, (0.19, 0.26), palette::FROST_ROBE).squared(2.4),
            ring(0.0, 1.32, (0.17, 0.27), palette::FROST_ROBE).squared(2.4),
            ring(-0.01, 1.42, (0.13, 0.2), palette::FROST_ROBE),
            ring(-0.01, 1.5, (0.09, 0.11), palette::FROST_DARK),
            ring(-0.01, 1.54, (0.08, 0.1), palette::FROST_DARK),
        ],
        28,
        (true, true),
    );
    let sash = belt(0.01, (0.82, 0.915), (0.208, 0.248), palette::FROST_BLUE);
    let mantle = tube(
        &[
            ring(-0.01, 1.53, (0.1, 0.13), palette::RIME),
            ring(-0.01, 1.47, (0.2, 0.28), palette::RIME),
            ring(-0.01, 1.38, (0.25, 0.36), palette::RIME),
            ring(-0.01, 1.3, (0.255, 0.37), palette::RIME).jagged(0.06),
        ],
        28,
        (true, true),
    );
    sculpted(vec![robe, sash, mantle])
}

/// Where the frost mage's skirt is at `a` (radians round from the front) and `v` (0..1) from the
/// sash down to its hem on the ground, flaring as it falls.
fn frost_skirt_point(a: f32, v: f32) -> Vec3 {
    let flare = v.powf(1.4);
    let (depth, width) = (0.192 + 0.16 * flare, 0.232 + 0.18 * flare);
    Vec3::new(a.cos() * depth, 0.88 - 0.85 * v, a.sin() * width)
}

/// The frost mage's moving cloth: the robe's skirt from the sash to the ground in deepening
/// folds, a blue panel down its front, a blue band above its rimed hem and a ring of glowing runes
/// just above that; and a long, narrow cape from under the mantle, rimed along its hem.
pub(super) fn frost_mage_wardrobe() -> Wardrobe {
    const FOLDS: u32 = 11;
    let skirt_sections: Vec<Section> = [
        (0.0, palette::FROST_ROBE, 0.0),
        (0.45, palette::FROST_ROBE, 0.025),
        (0.9, palette::FROST_BLUE, 0.045),
        (0.95, palette::RIME, 0.05),
        (1.0, palette::RIME, 0.05),
    ]
    .iter()
    .map(|&(v, color, folds)| {
        let half = Vec2::new(frost_skirt_point(0.0, v).x, frost_skirt_point(std::f32::consts::FRAC_PI_2, v).z);
        Section::new(Vec3::Y * frost_skirt_point(0.0, v).y, half, color).folded(folds, FOLDS)
    })
    .collect();
    let skirt = tube(&smoothed(&skirt_sections, 3), 56, (true, true));
    // The panel down the front, just over the skirt.
    let panel = tube(
        &[0.04, 0.45, 0.88]
            .map(|v| Section::new(frost_skirt_point(0.0, v) + Vec3::X * 0.012, Vec2::new(0.008, 0.05 - 0.016 * (1.0 - v)), palette::FROST_BLUE)),
        8,
        (true, true),
    );
    // Runes: small upright diamonds just above the hem, following the skirt's curve.
    let runes = sculpted(
        (0..10)
            .map(|i| {
                let around = (i as f32 + 0.5) / 10.0 * std::f32::consts::TAU;
                let rune = gem_mesh(0.03).scaled_by(Vec3::new(0.35, 1.5, 1.0)).rotated_by(Quat::from_rotation_y(-around));
                tinted(rune.translated_by(frost_skirt_point(around, 0.83) * Vec3::new(1.03, 1.0, 1.03)), Color::WHITE)
            })
            .collect(),
    );
    let skirt_chains: Vec<Vec<Vec3>> = (0..12)
        .map(|c| c as f32 / 12.0 * std::f32::consts::TAU)
        .map(|a| [0.04, 0.35, 0.68, 1.0].map(|v| frost_skirt_point(a, v)).to_vec())
        .collect();
    let skirt = Drape::new(
        sculpted(vec![skirt, panel]),
        Some(runes),
        skirt_chains,
        Weave { hold: (8.0, 2.2), damping: 6.0, air: 0.3, inertia: 0.3, gravity: 7.0, flutter: 0.35, joined: true, wisps: 4.0 },
    );

    // The cape: from under the back of the mantle, falling in soft waves and widening a little to
    // a rimed hem just off the ground.
    const ROWS: usize = 16;
    const COLS: usize = 16;
    let cape_point = |u: f32, v: f32, ragged: bool| {
        let half = 0.13 + 0.08 * v;
        let z = (u * 2.0 - 1.0) * half;
        let mut x = -0.24 - 0.1 * v + 1.5 * (1.0 - 0.6 * v) * z * z;
        if ragged {
            x += 0.03 * v * (z * 32.0 + v * 2.0).sin();
        }
        Vec3::new(x, 1.36 - 1.28 * v, z)
    };
    let cape = Drape::new(
        sheet(&grid(ROWS, COLS, (0.0, 1.0), |u, v| cape_point(u, v, true)), &hemmed(ROWS, 2, palette::FROST_DARK, palette::RIME), 0.012),
        None,
        [0.1, 0.5, 0.9].map(|u| [0.02, 0.27, 0.51, 0.75, 1.0].map(|v| cape_point(u, v, false)).to_vec()).to_vec(),
        Weave { hold: (5.0, 1.3), damping: 6.0, air: 0.4, inertia: 0.4, gravity: 6.0, flutter: 0.6, joined: true, wisps: 0.0 },
    );

    Wardrobe {
        drapes: vec![skirt, cape],
        trim: palette::FROST_GLOW,
        body: vec![
            Column { center: Vec2::ZERO, half: Vec2::new(0.175, 0.215), from: 0.3, to: 1.0, round: 2.0 },
            Column { center: Vec2::new(-0.01, 0.0), half: Vec2::new(0.21, 0.28), from: 1.0, to: 1.3, round: 2.0 },
        ],
        leg_radius: 0.12,
        arm: None,
    }
}

/// The frost mage's head: a deep hood rimed at the rim, empty but for the eyes, with a crown of
/// ice shards rising around it (so it reads from above).
pub(super) fn frost_crowned_hood() -> Mesh {
    let cloth = palette::FROST_ROBE;
    let top = [
        ring(-0.05, 0.22, (0.19, 0.19), cloth),
        ring(-0.08, 0.36, (0.15, 0.14), cloth),
        ring(-0.13, 0.48, (0.08, 0.07), cloth),
        ring(-0.2, 0.56, (0.025, 0.022), cloth),
        ring(-0.25, 0.58, (0.0, 0.0), cloth),
    ];
    let mut parts = deep_hood(cloth, palette::RIME, palette::FROST_DARK, &top);
    // Shards fanning up and out from the crown, tallest at the front.
    for i in 0..7 {
        let around = i as f32 / 7.0 * std::f32::consts::TAU;
        let height = 0.22 + 0.1 * (around.cos() * 0.5 + 0.5);
        let shard = ice_shard(0.035, height).rotated_by(Quat::from_rotation_z(-0.45));
        let turned = shard.rotated_by(Quat::from_rotation_y(around));
        parts.push(tinted(turned.translated_by(Vec3::new(-0.06, 0.32, 0.0)), palette::ICE));
    }
    sculpted(parts)
}

/// Wide bell sleeves, banded in blue and rimed at the cuff (dark inside), over a slim dark glove.
pub(super) fn frost_mage_arm() -> (Mesh, Mesh) {
    let sleeve = vec![
        ring(0.0, 0.08, (0.085, 0.085), palette::FROST_ROBE),
        ring(0.0, -0.1, (0.09, 0.09), palette::FROST_ROBE),
        ring(0.0, -0.22, (0.1, 0.1), palette::FROST_ROBE),
        ring(-0.01, -0.36, (0.148, 0.148), palette::FROST_ROBE),
        ring(-0.01, -0.385, (0.15, 0.15), palette::FROST_BLUE),
        ring(-0.01, -0.41, (0.152, 0.152), palette::FROST_ROBE),
        ring(-0.01, -0.43, (0.155, 0.155), palette::RIME),
        ring(-0.01, -0.47, (0.158, 0.158), palette::FROST_DARK),
    ];
    jointed(RIG_ELBOW, vec![(sleeve, 18, (true, true))], vec![fist(palette::DARK_LEATHER, -0.36, 0.056)])
}

/// The frost mage's staff, gripped at the origin, standing up (+Y): a dark shaft swelling a little
/// toward the head, bound in blue and shod in a steel spike, and at the top two steel crescents
/// curving up like an open cage around its crystals (`staff_crystals`, drawn glowing).
pub(super) fn ice_staff() -> Mesh {
    let wood = palette::BARK.darker(0.15);
    let mut parts = vec![
        sweep(&[Vec3::Y * -0.62, Vec3::Y * 0.2, Vec3::Y * 1.04], &[0.022, 0.024, 0.027], 12, wood),
        // The steel butt spike and its collar.
        sweep(&[Vec3::Y * -0.6, Vec3::Y * -0.66, Vec3::Y * -0.8], &[0.034, 0.03, 0.0], 12, palette::STEEL),
        // Blue bindings either side of the grip and up the shaft.
        band(0.09, 0.15, 0.031, 12, palette::FROST_BLUE),
        band(-0.15, -0.09, 0.03, 12, palette::FROST_BLUE),
        band(0.585, 0.615, 0.032, 12, palette::FROST_BLUE),
        band(0.665, 0.695, 0.032, 12, palette::FROST_BLUE),
        // The collar holding the head.
        sweep(&[Vec3::Y * 0.96, Vec3::Y * 1.02, Vec3::Y * 1.07], &[0.03, 0.04, 0.052], 12, palette::STEEL),
        sweep(&[Vec3::Y * 1.07, Vec3::Y * 1.095], &[0.054, 0.05], 12, palette::FROST_BLUE),
    ];
    for side in CRESCENTS {
        let points: Vec<Vec3> = (0..=6).map(|i| crescent(side, i as f32 / 6.0)).collect();
        let radii: Vec<f32> = (0..=6).map(|i| 0.017 - 0.007 * i as f32 / 6.0).collect();
        parts.push(sweep(&points, &radii, 10, palette::STEEL));
        parts.push(tinted(ball(0.02).translated_by(crescent(side, 1.0)), palette::STEEL));
    }
    sculpted(parts)
}

/// A frostbolt's crystal is this many times as long as the shot's radius, and turns this fast
/// (radians per second) as it flies.
const FROSTBOLT_LENGTH: f32 = 3.6;
pub(super) const FROSTBOLT_SPIN: f32 = 5.0;

/// A frostbolt, point first along +X (its point at the origin), sized by the shot's radius: a long
/// six-sided ice crystal with a ring of smaller shards splaying back from its waist like a
/// frozen burst, the core pale ice, the shards a deeper frost glow.
pub(super) fn frostbolt(radius: f32) -> Mesh {
    let length = radius * FROSTBOLT_LENGTH;
    let point_up = |mesh: Mesh| mesh.rotated_by(ALONG_X);
    let core = tinted(ice_shard(radius * 0.4, length).translated_by(Vec3::Y * -length), palette::ICE);
    // A shard standing from its base `back` behind the point, leaning back and `out` (radians)
    // from the axis, turned `around` it.
    let splay = |shard: Mesh, back: f32, out: f32, around: f32| {
        shard
            .rotated_by(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2 + out))
            .translated_by(Vec3::new(-back, -radius * 0.1, 0.0))
            .rotated_by(Quat::from_rotation_x(around))
    };
    let mut parts = vec![point_up(core)];
    // Five shards round the waist, alternating long and short.
    for i in 0..5 {
        let long = if i % 2 == 0 { 1.0 } else { 0.7 };
        let shard = tinted(ice_shard(radius * 0.17, length * 0.42 * long), palette::FROST_GLOW);
        parts.push(splay(shard, length * 0.35, 0.5, i as f32 * std::f32::consts::TAU / 5.0 + 0.3));
    }
    // Two slivers riding just behind the point.
    for around in [std::f32::consts::FRAC_PI_2, -std::f32::consts::FRAC_PI_2] {
        parts.push(splay(tinted(ice_shard(radius * 0.1, length * 0.3), palette::ICE), length * 0.12, 0.25, around));
    }
    sculpted(parts)
}

/// The cold glow around a frostbolt: a faceted halo round the crystal, stretched back into a
/// trail about as long again, fading out toward its end (vertex alpha).
pub(super) fn frost_aura(radius: f32) -> Mesh {
    let length = radius * FROSTBOLT_LENGTH;
    let halo = Sphere::new(1.0).mesh().ico(1).unwrap();
    let halo = halo.scaled_by(Vec3::new(length * 0.55, radius, radius)).translated_by(Vec3::X * -length * 0.45);
    let trail = cone(radius * 0.8, length * 1.4, 8).rotated_by(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2));
    let trail = trail.translated_by(Vec3::X * -length * 1.3);
    let mut aura = merge_parts(vec![halo, trail]);
    let fade = |x: f32| (1.0 + x / (length * 2.0)).clamp(0.0, 1.0).powf(1.5);
    let Some(bevy::mesh::VertexAttributeValues::Float32x3(positions)) = aura.attribute(Mesh::ATTRIBUTE_POSITION) else {
        unreachable!("meshes have positions")
    };
    let colors: Vec<[f32; 4]> = positions.iter().map(|p| [1.0, 1.0, 1.0, fade(p[0])]).collect();
    aura.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    aura
}
