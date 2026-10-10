//! Shapes for a simple but finished look, beyond stock primitives: lofted tubes (a smooth skin
//! through shaped cross-sections, so a robe runs as one piece from collar to hem, a hood wraps a
//! head and a blade has an edge) and bevelled boxes (stone and timber whose edges catch the light).
//! Meshes have the same attributes as Bevy's primitives (position, normal, uv, u32 indices), so
//! they merge with them; tubes carry vertex colors like `arena::tinted` parts.

use std::f32::consts::TAU;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

/// One cross-section of a `tube`: a rounded shape around a point on its path. The shape lies
/// across the path: `half.x` along the depth axis (+X when the path runs up +Y), `half.y` along
/// the width axis (+Z).
#[derive(Clone, Copy)]
pub struct Section {
    pub at: Vec3,
    pub half: Vec2,
    /// Superellipse exponent: 2 is an ellipse, higher squarer (a chest, a boot), 1 a diamond (a
    /// blade's edge).
    pub round: f32,
    /// Pushes the sides forward (+depth) by this much at the widest, so the section curves round
    /// like a cape across the back or a collar round the neck.
    pub bend: f32,
    /// Ragged edge: points this far back along the path on every other vertex (tatters at a hem).
    pub jag: f32,
    /// Soft cloth folds: the section ripples in and out this much (a fraction of its size),
    /// `fold_count` times round. Lined up from section to section, so they run down as creases.
    pub folds: f32,
    pub fold_count: u32,
    /// The color from this section to the next (and of the end cap, on the last).
    pub color: Color,
}

impl Section {
    /// An elliptical section `half` across (depth, width) at `at`.
    pub fn new(at: Vec3, half: Vec2, color: Color) -> Self {
        Section { at, half, round: 2.0, bend: 0.0, jag: 0.0, folds: 0.0, fold_count: 0, color }
    }

    /// A round section of `radius` at `at`.
    pub fn round(at: Vec3, radius: f32, color: Color) -> Self {
        Self::new(at, Vec2::splat(radius), color)
    }

    pub fn squared(self, round: f32) -> Self {
        Section { round, ..self }
    }

    pub fn bent(self, bend: f32) -> Self {
        Section { bend, ..self }
    }

    pub fn jagged(self, jag: f32) -> Self {
        Section { jag, ..self }
    }

    pub fn folded(self, folds: f32, fold_count: u32) -> Self {
        Section { folds, fold_count, ..self }
    }
}

/// `sections` with `steps - 1` more between each pair, along a smooth curve through them (their
/// positions and sizes), so a tube's profile curves instead of running straight from section to
/// section. Inserted sections take the color of the span they're in; only the given ones are
/// ragged.
pub fn smoothed(sections: &[Section], steps: usize) -> Vec<Section> {
    let n = sections.len();
    let get = |i: isize| sections[i.clamp(0, n as isize - 1) as usize];
    let curve = |p0: Vec3, p1: Vec3, p2: Vec3, p3: Vec3, t: f32| {
        let (t2, t3) = (t * t, t * t * t);
        0.5 * (2.0 * p1 + (p2 - p0) * t + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2 + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t3)
    };
    let mut out = vec![sections[0]];
    for i in 0..n as isize - 1 {
        let [s0, s1, s2, s3] = [get(i - 1), get(i), get(i + 1), get(i + 2)];
        for k in 1..=steps {
            let t = k as f32 / steps as f32;
            if k == steps {
                out.push(s2);
                continue;
            }
            let half = curve(s0.half.extend(0.0), s1.half.extend(0.0), s2.half.extend(0.0), s3.half.extend(0.0), t);
            out.push(Section {
                at: curve(s0.at, s1.at, s2.at, s3.at, t),
                half: half.truncate().max(Vec2::ZERO),
                round: s1.round + (s2.round - s1.round) * t,
                bend: s1.bend + (s2.bend - s1.bend) * t,
                jag: 0.0,
                folds: s1.folds + (s2.folds - s1.folds) * t,
                fold_count: s1.fold_count.max(s2.fold_count),
                color: s1.color,
            });
        }
    }
    out
}

/// Signed power, keeping the sign: the superellipse's curve.
fn spow(x: f32, p: f32) -> f32 {
    x.signum() * x.abs().powf(p)
}

/// A smooth skin through `sections` (at least two), `sides` vertices around, each span between
/// two sections in the color of the first, with hard color changes between spans but smooth
/// shading across them. Sections lie across the path (its direction at each section), turning
/// with it without twisting. `caps` closes the first and last section (leave a tapered-to-a-point
/// end open).
pub fn tube(sections: &[Section], sides: u32, caps: (bool, bool)) -> Mesh {
    assert!(sections.len() >= 2, "a tube needs two sections");
    let n = sections.len();
    let sides = sides as usize;
    // The path's direction at each section, and a depth axis carried along it without twisting.
    let tangents: Vec<Vec3> = (0..n)
        .map(|i| (sections[(i + 1).min(n - 1)].at - sections[i.saturating_sub(1)].at).normalize_or(Vec3::Y))
        .collect();
    let mut depth = Vec::with_capacity(n);
    let first = Vec3::X.reject_from(tangents[0]);
    depth.push(if first.length_squared() > 1e-6 { first.normalize() } else { Vec3::Z.reject_from(tangents[0]).normalize() });
    for i in 1..n {
        let carried = Quat::from_rotation_arc(tangents[i - 1], tangents[i]) * depth[i - 1];
        depth.push(carried.reject_from(tangents[i]).normalize());
    }

    let ring: Vec<Vec<Vec3>> = (0..n)
        .map(|i| {
            let s = &sections[i];
            let (t, d) = (tangents[i], depth[i]);
            let w = d.cross(t);
            (0..sides)
                .map(|k| {
                    let a = k as f32 / sides as f32 * TAU;
                    let ripple = 1.0 + s.folds * (a * s.fold_count as f32).sin();
                    let z = spow(a.sin(), 2.0 / s.round) * ripple;
                    let x = spow(a.cos(), 2.0 / s.round) * s.half.x * ripple + s.bend * z * z;
                    // Tatters: every other vertex hangs back, by a varying amount.
                    let hang = if k % 2 == 0 { s.jag * (0.45 + 0.55 * ((k * 37 % 11) as f32 / 10.0)) } else { 0.0 };
                    s.at + d * x + w * (z * s.half.y) - t * hang
                })
                .collect()
        })
        .collect();

    // Smooth normals over the whole skin (shared across spans, so color bands don't crease).
    let mut normals = vec![vec![Vec3::ZERO; sides]; n];
    for i in 0..n - 1 {
        for k in 0..sides {
            let k1 = (k + 1) % sides;
            let (a, b, c, d) = (ring[i][k], ring[i][k1], ring[i + 1][k1], ring[i + 1][k]);
            let face = (c - a).cross(b - a) + (d - a).cross(c - a);
            for (r, kk) in [(i, k), (i, k1), (i + 1, k1), (i + 1, k)] {
                normals[r][kk] += face;
            }
        }
    }

    let mut b = Builder::default();
    for i in 0..n - 1 {
        let color = sections[i].color;
        let base = b.positions.len() as u32;
        for r in [i, i + 1] {
            for k in 0..sides {
                b.vertex(ring[r][k], normals[r][k].normalize_or(tangents[r]), color);
            }
        }
        let s = sides as u32;
        for k in 0..s {
            let k1 = (k + 1) % s;
            let (a, bb, c, d) = (base + k, base + k1, base + s + k1, base + s + k);
            b.indices.extend([a, c, bb, a, d, c]);
        }
    }
    let mut cap = |i: usize, outward: Vec3, color: Color| {
        let center = ring[i].iter().copied().sum::<Vec3>() / sides as f32;
        let base = b.positions.len() as u32;
        b.vertex(center, outward, color);
        for k in 0..sides {
            b.vertex(ring[i][k], outward, color);
        }
        for k in 0..sides as u32 {
            let (p, q) = (base + 1 + k, base + 1 + (k + 1) % sides as u32);
            // Wound to face `outward`.
            if (b.positions[p as usize] - center).cross(b.positions[q as usize] - center).dot(outward) > 0.0 {
                b.indices.extend([base, p, q]);
            } else {
                b.indices.extend([base, q, p]);
            }
        }
    };
    if caps.0 {
        cap(0, -tangents[0], sections[0].color);
    }
    if caps.1 {
        cap(n - 1, tangents[n - 1], sections[n - 1].color);
    }
    b.build()
}

/// A round tube along a smooth curve through `points`, its radius running through `radii`: rods,
/// shafts, grips, beaks, antlers, tapering to a point if the last radius is 0.
pub fn sweep(points: &[Vec3], radii: &[f32], sides: u32, color: Color) -> Mesh {
    assert_eq!(points.len(), radii.len(), "a radius for every point");
    // Catmull-Rom through the points, a few steps per span, so bends are smooth.
    const STEPS: usize = 4;
    let n = points.len();
    let at = |i: isize| points[i.clamp(0, n as isize - 1) as usize];
    let mut sections = vec![Section::round(points[0], radii[0], color)];
    for i in 0..n - 1 {
        let (p0, p1, p2, p3) = (at(i as isize - 1), at(i as isize), at(i as isize + 1), at(i as isize + 2));
        let steps = if n > 2 { STEPS } else { 1 };
        for k in 1..=steps {
            let t = k as f32 / steps as f32;
            let (t2, t3) = (t * t, t * t * t);
            let point = 0.5
                * (2.0 * p1 + (p2 - p0) * t + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2 + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t3);
            sections.push(Section::round(point, radii[i] + (radii[i + 1] - radii[i]) * t, color));
        }
    }
    let pointed = radii.last().is_some_and(|r| *r < 1e-3);
    tube(&sections, sides, (true, !pointed))
}

/// A box of `size`, centered on the origin, its edges and corners cut off `bevel` deep, flat
/// shaded so each bevel catches the light as a thin highlight. No vertex colors.
pub fn bevel_box(size: Vec3, bevel: f32) -> Mesh {
    let h = size / 2.0;
    let bevel = bevel.min(h.min_element() * 0.45);
    let inset = h - Vec3::splat(bevel);
    // A corner's three points, one per axis: on that axis' face, inset along the other two.
    let point = |corner: Vec3, axis: usize| {
        let mut p = corner * inset;
        p[axis] = corner[axis] * h[axis];
        p
    };
    let signs = [-1.0, 1.0];
    let corners: Vec<Vec3> = (0..8).map(|i| Vec3::new(signs[i & 1], signs[(i >> 1) & 1], signs[(i >> 2) & 1])).collect();
    let mut b = Builder::default();
    for axis in 0..3 {
        for &s in &signs {
            // The face itself.
            let face: Vec<Vec3> = corners.iter().filter(|c| c[axis] == s).map(|&c| point(c, axis)).collect();
            let mut normal = Vec3::ZERO;
            normal[axis] = s;
            b.polygon(&face, normal);
            // The bevel along each of its edges with the next axis round.
            let other = (axis + 1) % 3;
            for &t in &signs {
                let edge: Vec<Vec3> = corners
                    .iter()
                    .filter(|c| c[axis] == s && c[other] == t)
                    .flat_map(|&c| [point(c, axis), point(c, other)])
                    .collect();
                let mut normal = Vec3::ZERO;
                normal[axis] = s;
                normal[other] = t;
                b.polygon(&edge, normal.normalize());
            }
        }
    }
    for &c in &corners {
        b.polygon(&[point(c, 0), point(c, 1), point(c, 2)], c.normalize());
    }
    let mut mesh = b.build();
    mesh.remove_attribute(Mesh::ATTRIBUTE_COLOR);
    mesh
}

#[derive(Default)]
struct Builder {
    positions: Vec<Vec3>,
    normals: Vec<Vec3>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

impl Builder {
    fn vertex(&mut self, at: Vec3, normal: Vec3, color: Color) {
        self.positions.push(at);
        self.normals.push(normal);
        self.colors.push(color.to_linear().to_f32_array());
    }

    /// A flat convex polygon facing `normal`, its points in any order.
    fn polygon(&mut self, points: &[Vec3], normal: Vec3) {
        let center = points.iter().copied().sum::<Vec3>() / points.len() as f32;
        let u = (points[0] - center).normalize();
        let v = normal.cross(u);
        let mut sorted = points.to_vec();
        sorted.sort_by(|a, b| {
            let angle = |p: &Vec3| (*p - center).dot(v).atan2((*p - center).dot(u));
            angle(a).total_cmp(&angle(b))
        });
        let base = self.positions.len() as u32;
        for p in &sorted {
            self.vertex(*p, normal, Color::WHITE);
        }
        for i in 1..sorted.len() as u32 - 1 {
            self.indices.extend([base, base + i, base + i + 1]);
        }
    }

    fn build(self) -> Mesh {
        let count = self.positions.len();
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32, 0.0]; count])
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colors)
            .with_inserted_indices(Indices::U32(self.indices))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every triangle faces away from the shape's center: none is wound inside out.
    fn faces_outward(mesh: &Mesh, center: Vec3) {
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(p)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else { panic!() };
        let Some(Indices::U32(idx)) = mesh.indices() else { panic!() };
        for t in idx.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(p[t[i] as usize]));
            let normal = (b - a).cross(c - a);
            assert!(normal.dot((a + b + c) / 3.0 - center) > -1e-6, "triangle {t:?} faces inward");
        }
    }

    #[test]
    fn bevelled_boxes_face_outward() {
        faces_outward(&bevel_box(Vec3::new(1.0, 0.4, 2.0), 0.08), Vec3::ZERO);
    }

    #[test]
    fn tubes_face_outward() {
        let sections = [0.0, 0.5, 1.0].map(|y| Section::new(Vec3::Y * y, Vec2::new(0.3, 0.2), Color::WHITE));
        faces_outward(&tube(&sections, 12, (true, true)), Vec3::Y * 0.5);
    }
}
