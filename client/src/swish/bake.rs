//! A tear's shapes, as code: the cracks, the broken ground and the ink bleeding round it, each way
//! a tear breaks (`BREAKS`), baked once into the texture its layers are drawn from (`tears.png`,
//! `tear_look`): `cargo test -p arena-client bake_tears -- --ignored`, then rebuild. The mist,
//! too, for the previews.

use super::*;

use std::f32::consts::TAU;

use crate::arena::FlatMesh;

/// Cracks through ground broken over a rectangle `length` by `width`, flat, along +X from the
/// origin (centered across it): a jagged rift down its middle, widest along its middle and
/// narrowing to its ends, and cracks running off it out to its sides, some forking. Each a line
/// of points, as wide as each says (from its spine out to either edge). Broken by `seed`.
fn rift_cracks(length: f32, width: f32, seed: u32) -> Vec<Vec<(Vec3, f32)>> {
    let mut cracks = Vec::new();
    let mut n = 0;
    let mut next = || {
        n += 1;
        noise(seed, n)
    };
    // The rift: in short jagged steps, wobbling either side of the middle.
    let steps = (length / 0.12).ceil().max(4.0) as usize;
    let rift: Vec<(Vec3, f32)> = (0..=steps)
        .map(|i| {
            let t = i as f32 / steps as f32;
            let wobble = if i == 0 || i == steps { 0.0 } else { (next() - 0.5) * width * 0.25 };
            let wide = 0.09 * (t * (1.0 - t) * 4.0).sqrt().max(0.2) * (0.7 + 0.5 * next());
            (Vec3::new(t * length, 0.0, wobble), wide)
        })
        .collect();
    // Cracks off it out toward the sides (a little past them), by turns, in jagged steps.
    let forks = (length * 4.5).round().max(4.0) as usize;
    for k in 0..forks {
        let at = rift[((k as f32 + 0.2 + 0.6 * next()) / forks as f32 * steps as f32) as usize % (steps + 1)].0;
        let side = if k % 2 == 0 { 1.0 } else { -1.0 };
        let out = width / 2.0 * (0.8 + 0.5 * next()) - at.z * side;
        let slant = (next() - 0.5) * 0.9;
        let to = at + Vec3::new(slant * out, 0.0, -side * out);
        let mut bend = |t: f32| at.lerp(to, t) + Vec3::new((next() - 0.5) * 0.12, 0.0, 0.0);
        let (a, c) = (bend(0.35), bend(0.7));
        cracks.push(vec![(at, 0.04), (a, 0.03), (c, 0.018), (to, 0.0)]);
        if next() < 0.5 {
            let twig = a + Vec3::new((next() - 0.5) * 0.5, 0.0, -side * out * 0.3);
            cracks.push(vec![(a, 0.018), (twig, 0.0)]);
        }
    }
    cracks.insert(0, rift);
    cracks
}

/// The cracks (`rift_cracks`) as triangles: each solid along its spine and clear at its edges,
/// `thickness` times as thick (the same cracks: their ink, or the light in them).
pub(crate) fn rift_shape(length: f32, width: f32, seed: u32, thickness: f32) -> FlatMesh {
    let mut b = FlatMesh::default();
    for crack in rift_cracks(length, width, seed) {
        for pair in crack.windows(2) {
            let ((p0, w0), (p1, w1)) = (pair[0], pair[1]);
            let (w0, w1) = (w0 * thickness, w1 * thickness);
            let along = (p1 - p0).normalize_or(Vec3::X);
            let side = Vec3::new(along.z, 0.0, -along.x);
            for sign in [1.0, -1.0] {
                b.tri_faded([p0, p0 + side * w0 * sign, p1], [1.0, 0.0, 1.0]);
                b.tri_faded([p1, p0 + side * w0 * sign, p1 + side * w1 * sign], [1.0, 0.0, 0.0]);
            }
        }
    }
    b
}

/// The ground broken over a rectangle `length` by `width` (as `rift_shape`): solid over it, its
/// edges ragged and fading out a little past it. Broken by `seed`.
pub(crate) fn rubble_shape(length: f32, width: f32, seed: u32) -> FlatMesh {
    let mut b = FlatMesh::default();
    // Its outline, all round, each point a little in or out: inner (solid) and outer (clear).
    let edge = |i: usize, out: f32| ragged_edge(length, width, seed, i, out);
    let around = RAGGED_POINTS;
    let middle = Vec3::X * length / 2.0;
    for i in 0..around {
        let j = (i + 1) % around;
        let (a, z, a_out, z_out) = (edge(i, 0.0), edge(j, 0.0), edge(i, RAGGED_SOFT), edge(j, RAGGED_SOFT));
        b.tri_faded([middle, z, a], [1.0, 1.0, 1.0]);
        b.tri_faded([a, z, z_out], [1.0, 1.0, 0.0]);
        b.tri_faded([a, z_out, a_out], [1.0, 0.0, 0.0]);
    }
    b
}

/// How many points the ragged outline of broken ground has, and how far past it (meters) it
/// fades out: a crisp edge.
const RAGGED_POINTS: usize = 48;
const RAGGED_SOFT: f32 = 0.012;
/// The ink bleeding out round it: how far (meters, shortest and longest) and how wide at the edge.
const BLEED_REACH: (f32, f32) = (0.04, 0.32);
const BLEED_WIDTH: f32 = 0.035;

/// The `i`th point round the ragged outline of ground broken over a rectangle `length` by
/// `width` (as `rubble_shape`), `out` further out. Broken by `seed`.
fn ragged_edge(length: f32, width: f32, seed: u32, i: usize, out: f32) -> Vec3 {
    let t = (i % RAGGED_POINTS) as f32 / RAGGED_POINTS as f32 * TAU;
    // A rounded rectangle's outline, by angle from its middle.
    let (c, s) = (t.cos(), t.sin());
    let r = |half: f32, v: f32| half * v.signum() * v.abs().powf(0.35);
    // Broken, not worn: every other point a shard sticking out, the rest bitten in.
    let shard = if i % 2 == 0 { 0.12 + 0.18 * noise(seed, 900 + (i % RAGGED_POINTS) as u32) } else { 0.0 };
    let ragged = 0.8 + 0.22 * noise(seed, 500 + (i % RAGGED_POINTS) as u32) + shard;
    Vec3::new(length / 2.0 + r(length / 2.0 * ragged + out, c), 0.0, r(width / 2.0 * ragged + out, s))
}

/// Ink bleeding out round the ragged outline of broken ground (`rubble_shape`'s, the same
/// `length`, `width` and `seed`): from each shard and notch a dark tendril seeping out into the
/// ground, of its own length, solid at the edge and thinning to nothing, some splitting in two.
pub(crate) fn bleed_shape(length: f32, width: f32, seed: u32) -> FlatMesh {
    let mut b = FlatMesh::default();
    let middle = Vec3::X * length / 2.0;
    for i in 0..RAGGED_POINTS {
        let r = |k: u32| noise(seed, 3000 + i as u32 * 8 + k);
        let from = ragged_edge(length, width, seed, i, -0.01);
        let out = (from - middle).with_y(0.0).normalize_or(Vec3::X);
        let reach = BLEED_REACH.0 + (BLEED_REACH.1 - BLEED_REACH.0) * r(0) * r(0);
        let bend = Vec3::new(out.z, 0.0, -out.x) * (r(1) - 0.5) * reach * 0.6;
        let tendrils = 1 + (r(2) < 0.3) as usize;
        for t in 0..tendrils {
            let fork = Vec3::new(out.z, 0.0, -out.x) * (t as f32 - 0.5) * 0.04 * (tendrils - 1) as f32;
            let to = from + out * reach + bend + fork * 4.0;
            let side = Vec3::new(out.z, 0.0, -out.x) * BLEED_WIDTH * (0.6 + 0.8 * r(3 + t as u32));
            let mid = from.lerp(to, 0.5) + bend * 0.3 + fork;
            b.tri_faded([from - side, mid, from + side], [1.0, 0.75, 1.0]);
            b.tri_faded([from - side, mid - side * 0.5, mid], [1.0, 0.75, 0.75]);
            b.tri_faded([from + side, mid, mid + side * 0.5], [1.0, 0.75, 0.75]);
            b.tri_faded([mid - side * 0.5, to, mid + side * 0.5], [0.75, 0.0, 0.75]);
        }
    }
    b
}

/// The dust the blow throws up: a flat ring 1 across, soft inside, clear at its edge (as
/// `lying.wgsl` draws it).
pub(crate) fn dust_shape() -> FlatMesh {
    let mut b = FlatMesh::default();
    let at = |angle: f32, r: f32| Vec3::new(angle.cos() * r, 0.0, -angle.sin() * r);
    let steps = 32;
    let (inner, middle, outer) = (0.3, 0.42, 0.5);
    for i in 0..steps {
        let (a, z) = (i as f32 / steps as f32 * TAU, (i + 1) as f32 / steps as f32 * TAU);
        b.tri_faded([at(a, inner), at(a, middle), at(z, middle)], [0.0, 1.0, 1.0]);
        b.tri_faded([at(a, inner), at(z, middle), at(z, inner)], [0.0, 1.0, 0.0]);
        b.tri_faded([at(a, middle), at(a, outer), at(z, outer)], [1.0, 0.0, 0.0]);
        b.tri_faded([at(a, middle), at(z, outer), at(z, middle)], [1.0, 0.0, 1.0]);
    }
    b
}

/// How many texels (along, down) each tile of a tear's texture has (`TEAR_TILES`).
pub(crate) const TEAR_TILE: UVec2 = UVec2::new(460, 160);

/// One way a tear breaks over a lane `length` by `width` (`seed`), as its texture has it
/// (`tear_look`), a texel's worth at a time over `TEAR_BOX`, a row at a time: how opaque the
/// broken ground and the ink bleeding round it are, and how far from the nearest crack's spine
/// each texel is, in that crack's widths there (as far as `CLOSING_REACH`).
fn bake_tear(length: f32, width: f32, seed: u32) -> [Vec<f32>; 3] {
    let (lo, hi) = (TEAR_BOX.0 * Vec2::new(length, width), TEAR_BOX.1 * Vec2::new(length, width));
    let texel = (hi - lo) / TEAR_TILE.as_vec2();
    let size = TEAR_TILE.x as usize * TEAR_TILE.y as usize;
    // The texels over a box (in the lane's plane), and where each one's middle is.
    let over = move |a: Vec2, b: Vec2| {
        let (from, to) = (((a - lo) / texel).floor().as_ivec2().max(IVec2::ZERO), ((b - lo) / texel).ceil().as_ivec2().min(TEAR_TILE.as_ivec2()));
        (from.y..to.y).flat_map(move |y| (from.x..to.x).map(move |x| (y as usize * TEAR_TILE.x as usize + x as usize, lo + (Vec2::new(x as f32, y as f32) + 0.5) * texel)))
    };
    // A shape's opacity over each texel, as its triangles have it there (the most of any).
    let paint = |shape: &FlatMesh| {
        let mut opaque = vec![0.0f32; size];
        for (corners, alphas) in shape.faded_triangles() {
            let [p, q, r] = corners.map(|c| c.xz());
            let (e1, e2) = (q - p, r - p);
            let area = e1.perp_dot(e2);
            if area.abs() < 1e-12 {
                continue;
            }
            for (k, at) in over(p.min(q).min(r), p.max(q).max(r)) {
                let (s, t) = ((at - p).perp_dot(e2) / area, e1.perp_dot(at - p) / area);
                if s >= -1e-4 && t >= -1e-4 && s + t <= 1.0 + 1e-4 {
                    opaque[k] = opaque[k].max(alphas[0] * (1.0 - s - t) + alphas[1] * s + alphas[2] * t);
                }
            }
        }
        opaque
    };
    let mut spine = vec![CLOSING_REACH; size];
    for crack in rift_cracks(length, width, seed) {
        for pair in crack.windows(2) {
            let ((a, w0), (b, w1)) = ((pair[0].0.xz(), pair[0].1), (pair[1].0.xz(), pair[1].1));
            let reach = Vec2::splat(w0.max(w1) * CLOSING_REACH);
            for (k, at) in over(a.min(b) - reach, a.max(b) + reach) {
                let u = ((at - a).dot(b - a) / (b - a).length_squared().max(1e-12)).clamp(0.0, 1.0);
                let wide = w0 + (w1 - w0) * u;
                if wide > 1e-6 {
                    spine[k] = spine[k].min(at.distance(a.lerp(b, u)) / wide);
                }
            }
        }
    }
    [paint(&rubble_shape(length, width, seed)), paint(&bleed_shape(length, width, seed)), spine]
}

/// Bakes the revenant's tear, every way it breaks, into `tears.png`: a tile each (`TEAR_TILES`),
/// its channels the broken ground, the ink bleeding round it and how far from the cracks' spines
/// (`bake_tear`).
#[test]
#[ignore = "writes tears.png; run by hand when the tear's shapes change"]
fn bake_tears() {
    let (from, range, width) = ClassId::by_key("revenant").unwrap().def().attack.lane(false).unwrap();
    let along = (TEAR_TILE.x * TEAR_TILES.x) as usize;
    let mut texels = vec![0u8; along * (TEAR_TILE.y * TEAR_TILES.y) as usize * 3];
    for seed in 0..BREAKS {
        let [ground, bleed, spine] = bake_tear(range - from, width, seed);
        let corner = UVec2::new(seed % TEAR_TILES.x, seed / TEAR_TILES.x) * TEAR_TILE;
        for (k, ((ground, bleed), spine)) in ground.into_iter().zip(bleed).zip(spine).enumerate() {
            let (x, y) = (k % TEAR_TILE.x as usize, k / TEAR_TILE.x as usize);
            let edge = x == 0 || y == 0 || x + 1 == TEAR_TILE.x as usize || y + 1 == TEAR_TILE.y as usize;
            let rgb = [ground, bleed, spine / CLOSING_REACH].map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
            // Nothing reaches its tile's edge: it all fits the box, and none of it is drawn into
            // its neighbour's.
            assert!(!edge || rgb == [0, 0, 255], "break {seed}: {rgb:?} at its tile's edge ({x}, {y})");
            let at = ((corner.y as usize + y) * along + corner.x as usize + x) * 3;
            texels[at..at + 3].copy_from_slice(&rgb);
        }
    }
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/swish/tears.png");
    let mut encoder = png::Encoder::new(std::fs::File::create(path).unwrap(), along as u32, texels.len() as u32 / 3 / along as u32);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::High);
    encoder.write_header().unwrap().write_image_data(&texels).unwrap();
    println!("wrote {path}");
}
