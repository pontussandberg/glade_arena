//! Building blocks the fighters' (and the world's) meshes are made of, whoever wears them:
//! simple shapes, tinting and merging, rings and bands, hoods, eyes, legs, fists, belts, chain
//! links, cloth panels, glints, and flat ground-hugging meshes.

use super::*;

use bevy::asset::RenderAssetUsages;
use bevy::color::Mix;
use bevy::mesh::PrimitiveTopology;

use crate::cloth::{Drape, Weave};
use crate::sculpt::{Section, grid, sheet, smoothed, sweep, tube};

/// `part` merged into `all`, either of them indexed or not: `Mesh::merge` alone drops or
/// scrambles triangles when one side is indexed and the other isn't.
pub(super) fn merge_into(all: &mut Mesh, part: &Mesh) {
    fn indexed(mesh: &mut Mesh) {
        if mesh.indices().is_none() {
            let count = mesh.count_vertices() as u32;
            mesh.insert_indices(bevy::mesh::Indices::U32((0..count).collect()));
        }
    }
    indexed(all);
    let mut part = part.clone();
    indexed(&mut part);
    all.merge(&part).expect("merged meshes share attributes");
}

pub(super) fn cylinder(r: f32, h: f32, sides: u32) -> Mesh {
    Cylinder::new(r, h).mesh().resolution(sides).build()
}

pub(super) fn cone(r: f32, h: f32, sides: u32) -> Mesh {
    Cone::new(r, h).mesh().resolution(sides).build()
}

/// Faceted parts merged into one mesh.
pub(super) fn merge_parts(parts: Vec<Mesh>) -> Mesh {
    sculpted(parts.into_iter().map(faceted).collect())
}

/// Parts merged into one smoothly shaded mesh (each part keeps its own smooth normals instead of
/// being faceted): the modern, simple look of fighters.
pub(super) fn sculpted(parts: Vec<Mesh>) -> Mesh {
    parts
        .into_iter()
        .reduce(|mut all, p| {
            merge_into(&mut all, &p);
            all
        })
        .expect("at least one part")
}

/// A straight band (a binding, a grip wrap, a ferrule) round the Y axis from `from` to `to`.
pub(super) fn band(from: f32, to: f32, radius: f32, sides: u32, color: Color) -> Mesh {
    sweep(&[Vec3::Y * from, Vec3::Y * to], &[radius, radius], sides, color)
}

/// A smooth ball: heads, knuckles, bone knobs.
pub(super) fn ball(radius: f32) -> Mesh {
    Sphere::new(radius).mesh().ico(2).unwrap()
}

/// Gives every vertex `color`, so one material can carry several colors: vertex colors multiply
/// the material's base color (white on a body, tinted while it's chilled). All parts merged
/// together need it, or none.
pub(super) fn tinted(mut mesh: Mesh, color: Color) -> Mesh {
    let count = mesh.count_vertices();
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, vec![color.to_linear().to_f32_array(); count]);
    mesh
}

/// Where the eyes sit in a `deep_hood`'s opening (and ±z, the other one).
const HOOD_EYES: Vec3 = Vec3::new(0.165, 0.2, 0.055);

/// Small, narrow, slanted eyes burning deep in a `deep_hood` drawn `hood` times its size (width,
/// height).
pub(super) fn spectral_eyes(hood: Vec2) -> Mesh {
    let h = HOOD_EYES;
    slit_eyes(Vec3::new(h.x * hood.x - 0.012, h.y * hood.y - 0.01, h.z * hood.x * 0.95))
}

/// Small, narrow, slanted eyes at `at` and its mirror (-z) in the head's space. Drawn in a glow
/// material (white here).
pub(super) fn slit_eyes(at: Vec3) -> Mesh {
    merge_parts(
        [1.0, -1.0]
            .map(|side| {
                let slit = Sphere::new(0.02).mesh().ico(1).unwrap().scaled_by(Vec3::new(0.5, 0.42, 1.5));
                let slant = Quat::from_rotation_x(side * 0.35);
                tinted(slit.rotated_by(slant).translated_by(at * Vec3::new(1.0, 1.0, side)), Color::WHITE)
            })
            .to_vec(),
    )
}

/// A section of a fighter part: `half` (depth, width) across at height `y`, offset `x` forward.
pub(super) fn ring(x: f32, y: f32, half: (f32, f32), color: Color) -> Section {
    Section::new(Vec3::new(x, y, 0.0), Vec2::new(half.0, half.1), color)
}

/// A belt (or a sash) round the waist from `y.0` up to `y.1`, `half` (depth, width) across at its
/// edge, `x` forward: a band standing a little proud of its rolled edges.
pub(super) fn belt(x: f32, y: (f32, f32), half: (f32, f32), color: Color) -> Mesh {
    let at = |y: f32, grow: f32| ring(x, y, (half.0 + grow, half.1 + grow), color).exact();
    tube(&[at(y.0, 0.0), at(y.0 + 0.012, 0.005), at(y.1 - 0.012, 0.004), at(y.1, -0.001)], 40, (true, true))
}

/// How a tabard moves: heavier than the rest, held close.
const TABARD_WEAVE: Weave = Weave { hold: (9.0, 2.0), damping: 5.0, air: 0.3, inertia: 0.3, gravity: 8.0, flutter: 0.4, joined: false, wisps: 0.0 };

/// A tabard: a narrow panel hanging from `top` (at the belt, in front) down by `drop`, `half` wide
/// at the top and at the bottom, curving back round the legs (`curve`), its end torn up to
/// `torn.0` deep in strips `torn.1` apart; in `colors` from the top down (the last for the rest).
pub(super) fn tabard(top: Vec3, drop: Vec3, half: (f32, f32), curve: f32, torn: (f32, f32), colors: &[Color]) -> Drape {
    const ROWS: usize = 12;
    const COLS: usize = 8;
    let at = |v: f32| top + drop * v;
    let rows = grid(ROWS, COLS, (0.0, 1.0), |u, v| {
        let z = (u * 2.0 - 1.0) * (half.0 + (half.1 - half.0) * v);
        let tear = if v >= 1.0 { torn.0 * (u * COLS as f32 * torn.1).sin().abs() } else { 0.0 };
        at(v) + Vec3::new(-curve * z * z, -tear, z)
    });
    Drape::new(sheet(&rows, colors, 0.008), None, vec![[0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0].map(at).to_vec()], TABARD_WEAVE)
}

/// A deep hood, its rim (in `rim`) sunk over the collar, rising through `top` (the sections
/// above the brow), its opening dark (`void`) and framed by a heavy lip of cloth, in the head's
/// space (the neck at the origin, facing +X). The eyes go at `HOOD_EYES`.
pub(super) fn deep_hood(cloth: Color, rim: Color, void: Color, top: &[Section]) -> Vec<Mesh> {
    let mut sections = vec![
        ring(-0.03, -0.05, (0.17, 0.19), rim),
        ring(-0.035, 0.01, (0.18, 0.195), cloth),
        ring(-0.04, 0.08, (0.19, 0.2), cloth),
    ];
    sections.extend_from_slice(top);
    let hood = tube(&smoothed(&sections, 3), 26, (true, false));
    let dark = tinted(ball(1.0).scaled_by(Vec3::new(0.07, 0.13, 0.12)).translated_by(Vec3::new(0.1, 0.19, 0.0)), void);
    vec![hood, dark, hood_brim(cloth, void.mix(&cloth, 0.45), 1.0), hood_brim(void.mix(&cloth, 0.45), void, -1.0)]
}

/// The cloth framing a `deep_hood`'s opening: a band folded back over the hood round it, deeper
/// over the brow, where it dips to a point hanging over the face, gathered in uneven creases,
/// rolled along its edge in `edge`. `layer` 1 is the outer fold; -1 the lining just inside it, narrower and set back.
fn hood_brim(cloth: Color, edge: Color, layer: f32) -> Mesh {
    const STEPS: usize = 28;
    let outer = layer > 0.0;
    let sections: Vec<Section> = (0..=STEPS)
        .map(|i| {
            let a = -1.95 + 3.9 * i as f32 / STEPS as f32;
            let brow = a.cos().max(0.0);
            // The point over the face: a dip at the top of the arch.
            let peak = (-(a / 0.32).powi(2)).exp();
            let creases = 1.0 + 0.22 * (a * 7.0 + 0.6).sin() * (0.6 + 0.4 * (a * 3.3).cos());
            // Tapering away at its ends, down at the jaw, into the hood's sides.
            let end = ((1.95 - a.abs()) / 0.6).clamp(0.0, 1.0);
            // The fold: how far it lies back over the hood, and how thick it is.
            let fold = if outer { (0.014 + 0.02 * brow + 0.012 * peak) * creases * (0.4 + 0.6 * end) } else { 0.008 };
            let thick = if outer { (0.01 + 0.007 * brow) * (0.5 + 0.5 * end) } else { 0.01 + 0.004 * brow };
            // Round the opening (curving back round the sides of the head, and leaning back with
            // the hood above the brow), the fold lying
            // back from its edge over the hood; the lining just inside the edge.
            let out = Vec3::new(0.0, a.cos() / 0.155, a.sin() / 0.135).normalize();
            let y = 0.17 + 0.15 * a.cos() - 0.045 * peak;
            let rim = Vec3::new(0.075 + 0.08 * a.cos() + 0.028 * peak - 0.3 * (y - 0.24).max(0.0), y, 0.135 * a.sin());
            let at = if outer { rim + out * thick * 0.6 - Vec3::X * fold * 0.5 } else { rim - out * 0.012 - Vec3::X * 0.014 };
            let color = if outer && (i == 0 || i == STEPS) { edge } else { cloth };
            Section::new(at, Vec2::new(fold, thick), color)
        })
        .collect();
    let brim = tube(&sections, 10, (true, true));
    if !outer {
        return brim;
    }
    // A rolled edge along the front of the fold, darker, so it reads as turned back cloth.
    let roll: Vec<Section> = sections
        .iter()
        .map(|s| Section::round(s.at + Vec3::X * s.half.x * 0.85, s.half.y * 0.75, edge))
        .collect();
    sculpted(vec![brim, tube(&roll, 8, (true, true))])
}


/// A limb that bends at `joint` (in the limb's space: hanging down from its top at the origin),
/// in two parts. `tubes` (each its sections, running down from the top, its sides and its caps)
/// are cut at the joint's height: what's above goes to the upper part, with a rounded knob at the
/// joint (the first, outermost tube's size and color there) so the bend shows no gap; what's
/// below goes to the lower part, built in its own space (the joint at its origin), along with
/// `below` (parts in the limb's space that hang below the joint: a fist, a bracer, a shield).
/// Returns (upper, lower).
pub(super) fn jointed(joint: Vec3, tubes: Vec<(Vec<Section>, u32, (bool, bool))>, below: Vec<Mesh>) -> (Mesh, Mesh) {
    let (mut upper, mut lower) = (Vec::new(), Vec::new());
    let mut knob = None;
    for (sections, sides, caps) in tubes {
        // The span the joint cuts through, and how far down it.
        let Some(i) = sections.windows(2).position(|s| s[0].at.y >= joint.y && joint.y >= s[1].at.y) else {
            if sections.iter().all(|s| s.at.y >= joint.y) {
                upper.push(tube(&sections, sides, caps));
            } else {
                lower.push(tube(&sections, sides, caps).translated_by(-joint));
            }
            continue;
        };
        let (a, b) = (sections[i], sections[i + 1]);
        let t = ((a.at.y - joint.y) / (a.at.y - b.at.y).max(1e-6)).clamp(0.0, 1.0);
        let between = Section {
            at: a.at.lerp(b.at, t),
            half: a.half.lerp(b.half, t),
            round: a.round + (b.round - a.round) * t,
            jag: 0.0,
            torn: 0.0,
            trail: a.trail + (b.trail - a.trail) * t,
            folds: a.folds + (b.folds - a.folds) * t,
            fold_count: a.fold_count.max(b.fold_count),
            rough: a.rough + (b.rough - a.rough) * t,
            color: a.color,
        };
        let (mut top, mut bottom) = (sections[..=i].to_vec(), sections[i + 1..].to_vec());
        if t > 1e-3 {
            top.push(between);
        }
        if t < 1.0 - 1e-3 {
            bottom.insert(0, between);
        }
        if top.len() >= 2 {
            upper.push(tube(&top, sides, (caps.0, true)));
        }
        if bottom.len() >= 2 {
            lower.push(tube(&bottom, sides, (true, caps.1)).translated_by(-joint));
        }
        knob.get_or_insert(between);
    }
    // The knob, a touch smaller than the tube it sits in, so their skins don't fight.
    if let Some(at) = knob {
        upper.push(tinted(ball((at.half.x + at.half.y) / 2.0 * 0.98).translated_by(at.at), at.color));
    }
    lower.extend(below.into_iter().map(|part| part.translated_by(-joint)));
    (sculpted(upper), sculpted(lower))
}

/// A leg hanging from the hip in `cloth`, `thigh` and `knee` thick (wide for baggy trousers),
/// into a soft `boot` with a toe: the thigh, and the shin from the knee (`RIG_KNEE`).
pub(super) fn booted_leg(cloth: Color, boot: Color, thigh: f32, knee: f32) -> (Mesh, Mesh) {
    let leg = vec![
        ring(0.0, 0.05, (thigh * 0.95, thigh * 0.95), cloth),
        ring(0.0, -0.2, (thigh, thigh * 0.95), cloth),
        ring(0.01, RIG_KNEE.y, (knee, knee * 0.96), cloth),
        ring(0.0, -0.55, (knee * 0.92, knee * 0.88), cloth),
        ring(0.0, -0.6, (0.078, 0.076), boot),
        ring(0.015, -0.7, (0.085, 0.078), boot),
        ring(0.04, -0.76, (0.11, 0.082), boot).squared(3.0),
        ring(0.05, -0.8, (0.115, 0.08), boot).squared(3.0),
    ];
    jointed(RIG_KNEE, vec![(leg, 16, (true, true))], vec![])
}

/// A fist closing below a cuff: in `color`, the hand at `RIG_HAND`, from `top` down.
pub(super) fn fist(color: Color, top: f32, width: f32) -> Mesh {
    tube(
        &[
            ring(0.0, top, (width * 0.9, width * 0.9), color),
            ring(0.0, -0.45, (width, width * 0.96), color),
            ring(0.01, -0.48, (width * 1.04, width * 0.94), color).squared(2.6),
            ring(0.015, -0.55, (width * 1.08, width * 0.95), color).squared(2.6),
            ring(0.01, -0.6, (width * 0.68, width * 0.72), color),
            ring(0.0, -0.62, (0.0, 0.0), color),
        ],
        16,
        (true, false),
    )
}

/// One iron link of a chain, `at`, upright and turned across the last one when `turned`.
pub(super) fn chain_link(at: Vec3, turned: bool) -> Mesh {
    let loop_points: Vec<Vec3> = (0..=12)
        .map(|i| i as f32 / 12.0 * std::f32::consts::TAU)
        .map(|a| {
            let (side, up) = (0.013 * a.sin(), 0.022 * a.cos());
            at + if turned { Vec3::new(side, up, 0.0) } else { Vec3::new(0.0, up, side) }
        })
        .collect();
    sweep(&loop_points, &[0.0055; 13], 6, palette::IRON)
}

/// A glint: a star of three long, thin crystals crossed through its middle, so it reads as a
/// sharp twinkle from any angle.
pub(super) fn glint() -> Mesh {
    let ray = || gem_mesh(0.2).scaled_by(Vec3::new(0.07, 1.0, 0.07));
    let x = ray().rotated_by(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2));
    let z = ray().rotated_by(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2));
    sculpted(vec![tinted(ray(), Color::WHITE), tinted(x, Color::WHITE), tinted(z, Color::WHITE)])
}

/// Deterministic, so the arena looks the same for every player and every run.
pub(super) struct Lcg(pub(super) u64);

impl Lcg {
    pub(super) fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
    pub(super) fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next()
    }
}

/// Collects flat-colored triangles into one mesh.
#[derive(Default)]
pub(crate) struct FlatMesh {
    pub(super) positions: Vec<[f32; 3]>,
    pub(super) colors: Vec<[f32; 4]>,
}

impl FlatMesh {
    /// One triangle, counter-clockwise seen from its front.
    pub(super) fn tri(&mut self, corners: [Vec3; 3], color: Color) {
        let c = color.to_linear();
        for p in corners {
            self.positions.push(p.to_array());
            self.colors.push([c.red, c.green, c.blue, 1.0]);
        }
    }

    /// A white triangle whose opacity at each corner is `alphas` (for see-through materials).
    pub(crate) fn tri_faded(&mut self, corners: [Vec3; 3], alphas: [f32; 3]) {
        for (p, alpha) in corners.into_iter().zip(alphas) {
            self.positions.push(p.to_array());
            self.colors.push([1.0, 1.0, 1.0, alpha]);
        }
    }

    /// `tri_faded`, facing both ways (seen from either side).
    pub(crate) fn tri_faded_both(&mut self, [a, b, c]: [Vec3; 3], [x, y, z]: [f32; 3]) {
        self.tri_faded([a, b, c], [x, y, z]);
        self.tri_faded([a, c, b], [x, z, y]);
    }

    /// A quad from four corners in order (counter-clockwise seen from its front).
    pub(super) fn quad(&mut self, [a, b, c, d]: [Vec3; 4], color: Color) {
        self.tri([a, b, c], color);
        self.tri([a, c, d], color);
    }

    pub(crate) fn build(self) -> Mesh {
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colors)
            .with_computed_flat_normals()
    }
}
