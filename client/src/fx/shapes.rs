//! The shapes effects are drawn with, flat on the ground (the XZ plane, y up) unless said: each a
//! white mesh whose vertices carry how opaque it is there (`FlatMesh::tri_faded`), for a
//! see-through material to color, so edges fade into the ground instead of ending in a line.
//! Most are 1 across their radius (scaled to the effect), and broken by a seed, so no two are
//! quite alike.

use std::f32::consts::TAU;

use bevy::prelude::*;

use crate::arena::FlatMesh;
use crate::swish::noise;

/// A point `r` out at `angle` on the ground.
fn at(angle: f32, r: f32) -> Vec3 {
    Vec3::new(angle.cos() * r, 0.0, -angle.sin() * r)
}

/// Across `along` on the ground, to its left.
fn across(along: Vec3) -> Vec3 {
    Vec3::new(along.z, 0.0, -along.x)
}

/// A line along `points`, each as wide as it says, solid along its spine and clear at its
/// edges: a crack, a vein of frost, a streak.
fn stroke(b: &mut FlatMesh, points: &[(Vec3, f32)]) {
    for pair in points.windows(2) {
        let ((p0, w0), (p1, w1)) = (pair[0], pair[1]);
        let side = across((p1 - p0).normalize_or(Vec3::X));
        for sign in [1.0, -1.0] {
            b.tri_faded_both([p0, p0 + side * w0 * sign, p1], [1.0, 0.0, 1.0]);
            b.tri_faded_both([p1, p0 + side * w0 * sign, p1 + side * w1 * sign], [1.0, 0.0, 0.0]);
        }
    }
}

/// A line from `from` toward `dir`, `length` long, wandering a little as it goes (by `seed` from
/// `k`), `width` wide at its root, thinning to nothing.
fn wander(seed: u32, k: u32, from: Vec3, dir: Vec3, length: f32, width: f32, steps: u32, wobble: f32) -> Vec<(Vec3, f32)> {
    let side = across(dir);
    (0..=steps)
        .map(|i| {
            let t = i as f32 / steps as f32;
            let off = if i == 0 { 0.0 } else { (noise(seed, k + i) - 0.5) * wobble };
            (from + dir * length * t + side * off, width * (1.0 - t).powf(0.8))
        })
        .collect()
}

/// A ragged blob 1 across its radius: solid inside, its rim wavering in and out and fading to
/// clear a little past it: scorched earth, a splash, a glaze of frost.
pub(super) fn splotch(seed: u32, ragged: f32) -> Mesh {
    let mut b = FlatMesh::default();
    let points = 40;
    let rim = |i: usize, out: f32| {
        let a = i as f32 / points as f32 * TAU;
        let wobble = 1.0 - ragged * 0.5 + ragged * (0.6 * noise(seed, i as u32 % points as u32) + 0.4 * noise(seed, 100 + (i as u32 / 3) % 40));
        at(a, wobble * (0.82 + out))
    };
    for i in 0..points {
        let j = (i + 1) % points;
        b.tri_faded_both([Vec3::ZERO, rim(i, 0.0), rim(j, 0.0)], [1.0, 0.9, 0.9]);
        b.tri_faded_both([rim(i, 0.0), rim(i, 0.18), rim(j, 0.18)], [0.9, 0.0, 0.0]);
        b.tri_faded_both([rim(i, 0.0), rim(j, 0.18), rim(j, 0.0)], [0.9, 0.0, 0.9]);
    }
    b.build()
}

/// Streaks running out from the middle, 1 long at most, like soot thrown out round a blast, or
/// thorny vines, or a splash's drips: `count` of them, each of its own length, some forking.
pub(super) fn streaks(seed: u32, count: u32, width: f32) -> Mesh {
    let mut b = FlatMesh::default();
    for k in 0..count {
        let r = |i: u32| noise(seed, 400 + k * 16 + i);
        let angle = (k as f32 + r(0) * 0.7) / count as f32 * TAU;
        let dir = at(angle, 1.0);
        let from = dir * (0.25 + 0.2 * r(1));
        let length = 0.35 + 0.6 * r(2);
        let line = wander(seed, 600 + k * 16, from, dir, length, width * (0.6 + 0.8 * r(3)), 5, 0.12);
        stroke(&mut b, &line);
        if r(4) < 0.4 {
            let (root, w) = line[2];
            let twig_dir = (dir + across(dir) * (r(5) - 0.5) * 1.6).normalize();
            stroke(&mut b, &wander(seed, 800 + k * 16, root, twig_dir, length * 0.4, w * 0.7, 3, 0.06));
        }
    }
    b.build()
}

/// Cracks breaking out from the middle, 1 long at most, jagged, forking as they go: the ground
/// split by a blast, its cracks to glow with what broke it.
pub(super) fn cracks(seed: u32, count: u32) -> Mesh {
    let mut b = FlatMesh::default();
    for k in 0..count {
        let r = |i: u32| noise(seed, 1200 + k * 16 + i);
        let angle = (k as f32 + 0.3 + 0.4 * r(0)) / count as f32 * TAU;
        let dir = at(angle, 1.0);
        let line = wander(seed, 1400 + k * 16, dir * 0.05, dir, 0.6 + 0.4 * r(1), 0.05, 7, 0.18);
        stroke(&mut b, &line);
        for fork in 0..2 {
            if r(2 + fork) < 0.55 {
                let (root, w) = line[2 + fork as usize * 2];
                let side = if fork == 0 { 1.0 } else { -1.0 };
                let twig = (dir + across(dir) * side * (0.6 + 0.5 * r(4 + fork))).normalize();
                stroke(&mut b, &wander(seed, 1600 + k * 16 + fork * 8, root, twig, 0.18 + 0.2 * r(6 + fork), w * 0.7, 4, 0.08));
            }
        }
    }
    b.build()
}

/// Frost: ice crystals grown out over the ground from the middle, 1 across at most: six-fold
/// branches like a fern of rime, side shoots off them at a sixth of a turn, finer shoots off
/// those, each frond tapering to nothing.
pub(super) fn frost(seed: u32) -> Mesh {
    let mut b = FlatMesh::default();
    // A frond from `from` along `dir`, `length` long, `width` at its root, shooting off sides
    // `depth` more times.
    fn frond(b: &mut FlatMesh, seed: u32, k: u32, from: Vec3, dir: Vec3, length: f32, width: f32, depth: u32) {
        let steps = 4;
        let line = wander(seed, k, from, dir, length, width, steps, length * 0.08);
        stroke(b, &line);
        if depth == 0 {
            return;
        }
        let shoots = 2 + (noise(seed, k + 50) * 3.0) as u32;
        for s in 0..shoots {
            let t = 0.25 + 0.6 * (s as f32 + noise(seed, k + 60 + s)) / shoots as f32;
            let (root, w) = line[((t * steps as f32) as usize).min(steps as usize)];
            let side = if (s + k) % 2 == 0 { 1.0 } else { -1.0 };
            let turn = side * (TAU / 6.0) * (0.85 + 0.3 * noise(seed, k + 70 + s));
            let shoot = Quat::from_rotation_y(turn) * dir;
            frond(b, seed, k * 7 + s * 13 + 100, root, shoot, length * (0.3 + 0.25 * noise(seed, k + 80 + s)) * (1.0 - t * 0.5), w * 0.8, depth - 1);
        }
    }
    let arms = 6 + (noise(seed, 1) * 3.0) as u32;
    for k in 0..arms {
        let angle = (k as f32 + 0.4 * noise(seed, 10 + k)) / arms as f32 * TAU;
        let dir = at(angle, 1.0);
        let length = 0.6 + 0.4 * noise(seed, 20 + k);
        frond(&mut b, seed, 1000 + k * 97, dir * 0.04, dir, length, 0.045, 2);
    }
    b.build()
}

/// A soft band round a ring, 1 out at its middle, `width` wide, clear at both edges: a
/// shockwave, a ripple.
pub(super) fn band(width: f32) -> Mesh {
    let mut b = FlatMesh::default();
    let steps = 40;
    let (inner, outer) = (1.0 - width / 2.0, 1.0 + width / 2.0);
    for i in 0..steps {
        let (a, z) = (i as f32 / steps as f32 * TAU, (i + 1) as f32 / steps as f32 * TAU);
        b.tri_faded_both([at(a, inner), at(a, 1.0), at(z, 1.0)], [0.0, 1.0, 1.0]);
        b.tri_faded_both([at(a, inner), at(z, 1.0), at(z, inner)], [0.0, 1.0, 0.0]);
        b.tri_faded_both([at(a, 1.0), at(a, outer), at(z, outer)], [1.0, 0.0, 0.0]);
        b.tri_faded_both([at(a, 1.0), at(z, outer), at(z, 1.0)], [1.0, 0.0, 1.0]);
    }
    b.build()
}

/// A ring of runes, 1 across: a thin circle broken into strokes, with glyphs (ticks, chevrons,
/// little crosses) standing on it at intervals, and a fainter circle inside: a sigil drawn on
/// the ground where something is about to happen.
pub(super) fn runes(seed: u32) -> Mesh {
    let mut b = FlatMesh::default();
    let segments = 18;
    for i in 0..segments {
        let a0 = i as f32 / segments as f32 * TAU;
        let gap = 0.035 + 0.03 * noise(seed, 2000 + i);
        let a1 = (i + 1) as f32 / segments as f32 * TAU - gap;
        let arc: Vec<(Vec3, f32)> = (0..=4).map(|s| (at(a0 + (a1 - a0) * s as f32 / 4.0, 1.0), 0.03)).collect();
        stroke(&mut b, &arc);
        let mid = (a0 + a1) / 2.0;
        let out = at(mid, 1.0);
        let up = out.normalize();
        match (noise(seed, 2100 + i) * 3.0) as u32 {
            0 => stroke(&mut b, &[(out * 0.88, 0.025), (out * 0.78, 0.0)]),
            1 => {
                let side = across(up) * 0.05;
                stroke(&mut b, &[(out * 0.9 + side, 0.02), (out * 0.82, 0.02), (out * 0.9 - side, 0.0)]);
            }
            _ => {
                let side = across(up) * 0.04;
                stroke(&mut b, &[(out * 0.86 - side, 0.02), (out * 0.86 + side, 0.0)]);
                stroke(&mut b, &[(out * 0.9, 0.02), (out * 0.82, 0.0)]);
            }
        }
    }
    let inner: Vec<(Vec3, f32)> = (0..=48).map(|s| (at(s as f32 / 48.0 * TAU, 0.72), 0.012)).collect();
    stroke(&mut b, &inner);
    b.build()
}

/// A tongue of flame lying along +X from the origin, 1 long: a teardrop, widest a third of the
/// way out, licking to a point; solid along its middle, clear at its edges and tip.
pub(super) fn tongue() -> Mesh {
    let mut b = FlatMesh::default();
    let steps = 6;
    let width = |t: f32| 0.32 * (t * 3.0).min(1.0) * (1.0 - t).powf(0.7);
    for i in 0..steps {
        let (t0, t1) = (i as f32 / steps as f32, (i + 1) as f32 / steps as f32);
        let (a0, a1) = (1.0 - t0 * 0.8, 1.0 - t1 * 0.8 - if i + 1 == steps { 0.2 } else { 0.0 });
        for side in [1.0, -1.0] {
            let (p0, p1) = (Vec3::X * t0, Vec3::X * t1);
            let (e0, e1) = (p0 + Vec3::Z * width(t0) * side, p1 + Vec3::Z * width(t1) * side);
            b.tri_faded_both([p0, e0, p1], [a0, 0.0, a1]);
            b.tri_faded_both([p1, e0, e1], [a1, 0.0, 0.0]);
        }
    }
    b.build()
}

/// Arms of a whirl, 1 across: `arms` curving strokes spiralling in to the middle, thick at their
/// outer ends and thinning in: a vortex seen from above.
pub(super) fn whirl(seed: u32, arms: u32) -> Mesh {
    let mut b = FlatMesh::default();
    for k in 0..arms {
        let start = k as f32 / arms as f32 * TAU + noise(seed, 3000 + k) * 0.4;
        let line: Vec<(Vec3, f32)> = (0..=14)
            .map(|i| {
                let t = i as f32 / 14.0;
                let r = 1.0 - t * 0.92;
                (at(start + t * TAU * 0.75, r), 0.09 * r.powf(0.6))
            })
            .collect();
        stroke(&mut b, &line);
    }
    b.build()
}

/// A glint of light: a four-pointed star, 1 across, its points fading.
pub(super) fn glint() -> Mesh {
    let mut b = FlatMesh::default();
    for k in 0..4 {
        let a = k as f32 / 4.0 * TAU;
        let (tip, l, r) = (at(a, 0.5), at(a + 0.6, 0.07), at(a - 0.6, 0.07));
        b.tri_faded_both([Vec3::ZERO, l, tip], [1.0, 0.6, 0.0]);
        b.tri_faded_both([Vec3::ZERO, tip, r], [1.0, 0.0, 0.6]);
    }
    b.build()
}

/// A jagged line of lightning from `from` to `to` (anywhere), forking: a ribbon facing up, and
/// crossed with one standing on edge, so it reads from any angle; `width` at its widest.
pub(super) fn lightning(seed: u32, from: Vec3, to: Vec3, width: f32) -> Mesh {
    let mut b = FlatMesh::default();
    let length = from.distance(to).max(0.01);
    let dir = (to - from) / length;
    // Perpendiculars to bend along and to stand the ribbon on.
    let side = dir.any_orthonormal_vector();
    let up = dir.cross(side);
    let steps = ((length / 0.35).ceil() as u32).clamp(3, 24);
    let bolt = |b: &mut FlatMesh, k: u32, from: Vec3, to: Vec3, width: f32, steps: u32| {
        let points: Vec<Vec3> = (0..=steps)
            .map(|i| {
                let t = i as f32 / steps as f32;
                let jag = if i == 0 || i == steps { 0.0 } else { 1.0 };
                from.lerp(to, t) + (side * (noise(seed, k + i) - 0.5) + up * (noise(seed, k + 50 + i) - 0.5)) * from.distance(to) * 0.18 * jag
            })
            .collect();
        for pair in points.windows(2) {
            let (p0, p1) = (pair[0], pair[1]);
            let along = (p1 - p0).normalize_or(dir);
            for across in [along.cross(up).normalize_or(side), along.cross(side).normalize_or(up)] {
                let w = across * width;
                b.tri_faded_both([p0 - w, p0, p1], [0.0, 1.0, 1.0]);
                b.tri_faded_both([p0 - w, p1, p1 - w], [0.0, 1.0, 0.0]);
                b.tri_faded_both([p0, p0 + w, p1 + w], [1.0, 0.0, 0.0]);
                b.tri_faded_both([p0, p1 + w, p1], [1.0, 0.0, 1.0]);
            }
        }
        points
    };
    let main = bolt(&mut b, 0, from, to, width, steps);
    // A fork or two off it.
    for f in 0..2 {
        let at = main[(main.len() as f32 * (0.3 + 0.35 * noise(seed, 900 + f))) as usize % main.len()];
        let off = (side * (noise(seed, 910 + f) - 0.5) + up * (noise(seed, 920 + f) - 0.5)).normalize_or(side);
        let end = at + (dir * 0.6 + off) * length * 0.25;
        bolt(&mut b, 1000 + f * 100, at, end, width * 0.5, (steps / 3).max(2));
    }
    b.build()
}
