//! A software preview of each fighter's figure, for working on models without running the game:
//! `cargo test -p arena-client --lib preview -- --ignored` writes `fighter-<class>.png` (front,
//! three-quarter, back three-quarter and game-camera views of the rest pose, and the same of the body and head closer up
//! in `body-` and `head-<class>.png`) into `PREVIEW_DIR` (default the
//! system temp folder). Flat z-buffered triangles in vertex colors, sunlit from the upper left.

use bevy::mesh::{Indices, VertexAttributeValues};
use bevy::prelude::*;

use crate::arena::{self, RIG_HAND, RIG_HIP, RIG_NECK, RIG_SHOULDER, RIG_TAIL};

const SIZE: usize = 360;

struct Tri {
    at: [Vec3; 3],
    normal: [Vec3; 3],
    color: Vec3,
    /// Drawn at full brightness, whatever the light.
    glow: bool,
}

fn triangles(mesh: &Mesh, place: Mat4, out: &mut Vec<Tri>, glow: bool) {
    let Some(VertexAttributeValues::Float32x3(p)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else { return };
    let Some(VertexAttributeValues::Float32x3(n)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL) else { return };
    let colors = match mesh.attribute(Mesh::ATTRIBUTE_COLOR) {
        Some(VertexAttributeValues::Float32x4(c)) => c.clone(),
        _ => vec![[1.0; 4]; p.len()],
    };
    let idx: Vec<u32> = match mesh.indices() {
        Some(Indices::U32(i)) => i.clone(),
        Some(Indices::U16(i)) => i.iter().map(|&v| v as u32).collect(),
        None => (0..p.len() as u32).collect(),
    };
    for t in idx.chunks(3) {
        let t = [t[0], t[1], t[2]];
        let at = t.map(|i| place.transform_point3(Vec3::from(p[i as usize])));
        let normal = t.map(|i| place.transform_vector3(Vec3::from(n[i as usize])).normalize_or_zero());
        let c = t.iter().map(|&i| Vec3::from_slice(&colors[i as usize][..3])).sum::<Vec3>() / 3.0;
        out.push(Tri { at, normal, color: c, glow });
    }
}

/// The figure in its rest pose: arms hanging, the weapon upright in the right hand.
fn figure(class: &str) -> Vec<Tri> {
    let rig = arena::fighter_rig(class);
    let mut tris = Vec::new();
    let at = |p: Vec3| Mat4::from_translation(p);
    triangles(&arena::fighter_mesh(class), Mat4::IDENTITY, &mut tris, false);
    triangles(&rig.head, at(RIG_NECK), &mut tris, false);
    triangles(&rig.eyes, at(RIG_NECK), &mut tris, true);
    let left = Vec3::new(RIG_SHOULDER.x, RIG_SHOULDER.y, -RIG_SHOULDER.z);
    for shoulder in [RIG_SHOULDER, left] {
        triangles(&rig.arm, at(shoulder), &mut tris, false);
    }
    triangles(&rig.held, at(RIG_SHOULDER + RIG_HAND), &mut tris, false);
    if let Some(glow) = &rig.held_glow {
        triangles(glow, at(RIG_SHOULDER + RIG_HAND), &mut tris, true);
    }
    if let Some(glow) = &rig.body_glow {
        triangles(glow, Mat4::IDENTITY, &mut tris, true);
    }
    for hip in [RIG_HIP, Vec3::new(RIG_HIP.x, RIG_HIP.y, -RIG_HIP.z)] {
        triangles(&rig.leg, at(hip), &mut tris, false);
    }
    if let Some(tail) = &rig.tail {
        triangles(tail, at(RIG_TAIL), &mut tris, false);
    }
    tris
}

/// One view looking along `-from` (orthographic), centered at `center`.
/// `span` world units across.
fn render(tris: &[Tri], from: Vec3, center: Vec3, span: f32, image: &mut [[u8; 3]], stride: usize, x0: usize) {
    let forward = -from.normalize();
    let right = forward.cross(Vec3::Y).normalize();
    let up = right.cross(forward);
    let sun = Vec3::new(-0.6, 0.75, 0.35).normalize();
    let mut depth = vec![f32::INFINITY; SIZE * SIZE];
    let scale = SIZE as f32 / span;
    for t in tris {
        let s = t.at.map(|p| {
            let d = p - center;
            Vec3::new(SIZE as f32 / 2.0 + d.dot(right) * scale, SIZE as f32 / 2.0 - d.dot(up) * scale, d.dot(forward))
        });
        let area = (s[1].x - s[0].x) * (s[2].y - s[0].y) - (s[2].x - s[0].x) * (s[1].y - s[0].y);
        if area.abs() < 1e-9 {
            continue;
        }
        let (lo, hi) = (s[0].min(s[1]).min(s[2]), s[0].max(s[1]).max(s[2]));
        for y in (lo.y.floor().max(0.0) as usize)..=(hi.y.ceil().min(SIZE as f32 - 1.0) as usize) {
            for x in (lo.x.floor().max(0.0) as usize)..=(hi.x.ceil().min(SIZE as f32 - 1.0) as usize) {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let w0 = ((s[1].x - px) * (s[2].y - py) - (s[2].x - px) * (s[1].y - py)) / area;
                let w1 = ((s[2].x - px) * (s[0].y - py) - (s[0].x - px) * (s[2].y - py)) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let z = w0 * s[0].z + w1 * s[1].z + w2 * s[2].z;
                if z >= depth[y * SIZE + x] {
                    continue;
                }
                depth[y * SIZE + x] = z;
                let mut n = (t.normal[0] * w0 + t.normal[1] * w1 + t.normal[2] * w2).normalize_or_zero();
                if n.dot(forward) > 0.0 {
                    n = -n;
                }
                let lit = if t.glow {
                    t.color
                } else {
                    // Brightened so near-black cloth still shows its form.
                    t.color * (0.45 + 1.6 * n.dot(sun).max(0.0)) * 3.0
                };
                image[y * stride + x0 + x] = lit.to_array().map(|c| (c.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0) as u8);
            }
        }
    }
}

/// A minimal PNG: RGB, stored (uncompressed) deflate blocks.
fn png(width: usize, height: usize, pixels: &[[u8; 3]]) -> Vec<u8> {
    fn crc(data: &[u8]) -> u32 {
        let mut c = 0xFFFF_FFFFu32;
        for &b in data {
            c ^= b as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        !c
    }
    fn chunk(out: &mut Vec<u8>, kind: &[u8], data: &[u8]) {
        out.extend((data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend(data);
        out.extend(&body);
        out.extend(crc(&body).to_be_bytes());
    }
    let mut raw = Vec::new();
    for row in pixels.chunks(width) {
        raw.push(0);
        raw.extend(row.iter().flatten());
    }
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
    for (i, block) in blocks.iter().enumerate() {
        z.push((i == blocks.len() - 1) as u8);
        z.extend((block.len() as u16).to_le_bytes());
        z.extend((!(block.len() as u16)).to_le_bytes());
        z.extend(*block);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in &raw {
        a = (a + byte as u32) % 65_521;
        b = (b + a) % 65_521;
    }
    z.extend(((b << 16) | a).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = (width as u32).to_be_bytes().to_vec();
    header.extend((height as u32).to_be_bytes());
    header.extend([8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

#[test]
#[ignore = "writes images; run by hand when working on models"]
fn preview_fighters() {
    let dir = std::env::var("PREVIEW_DIR").map(std::path::PathBuf::from).unwrap_or_else(|_| std::env::temp_dir());
    let views = [Vec3::X, Vec3::new(1.0, 0.25, 0.9), Vec3::new(-1.0, 0.3, -0.8), Vec3::new(0.0, 30.0, 16.0)];
    for class in arena::FIGHTER_LOOKS {
        // The weapon alone, flat on and edge on.
        let rig = arena::fighter_rig(class);
        let mut weapon = Vec::new();
        triangles(&rig.held, Mat4::IDENTITY, &mut weapon, false);
        if let Some(glow) = &rig.held_glow {
            triangles(glow, Mat4::IDENTITY, &mut weapon, true);
        }
        let mut image = vec![[60u8, 70, 80]; 2 * SIZE * SIZE];
        for (i, from) in [Vec3::X, Vec3::new(0.3, 0.2, 1.0)].into_iter().enumerate() {
            render(&weapon, from, Vec3::Y * 0.6, 1.9, &mut image, 2 * SIZE, i * SIZE);
        }
        std::fs::write(dir.join(format!("weapon-{class}.png")), png(2 * SIZE, SIZE, &image)).unwrap();

        let tris = figure(class);
        let width = SIZE * views.len();
        for (name, center, span) in [("fighter", Vec3::Y * 1.0, 2.6), ("body", Vec3::Y * 0.85, 1.7), ("head", Vec3::Y * 1.6, 0.9)] {
            let mut image = vec![[200u8, 204, 200]; width * SIZE];
            for (i, from) in views.into_iter().enumerate() {
                render(&tris, from, center, span, &mut image, width, i * SIZE);
            }
            std::fs::write(dir.join(format!("{name}-{class}.png")), png(width, SIZE, &image)).unwrap();
        }
    }
}

