//! A software preview of each fighter's figure, for working on models without running the game:
//! `cargo test -p arena-client --lib preview -- --ignored` writes `fighter-<class>.png` (front,
//! three-quarter, back three-quarter and game-camera views of the rest pose, and the same of the body and head closer up
//! in `body-` and `head-<class>.png`) into `PREVIEW_DIR` (default the
//! system temp folder). Flat z-buffered triangles in vertex colors, sunlit from the upper left.

use bevy::mesh::{Indices, VertexAttributeValues};
use bevy::prelude::*;

use crate::arena::{self, RIG_HIP, RIG_KNEE, RIG_NECK, RIG_SHOULDER};

const SIZE: usize = 360;

#[derive(Clone, Copy)]
struct Tri {
    at: [Vec3; 3],
    normal: [Vec3; 3],
    color: Vec3,
    /// Drawn at full brightness, whatever the light.
    glow: bool,
    /// See-through, this opaque at each corner: drawn over what's behind it (after everything
    /// solid), unlit.
    alpha: Option<[f32; 3]>,
    /// Part of the held weapon (`posed_figure`) or its trail, for `render_view`'s mask of where
    /// it shows.
    weapon: bool,
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
        out.push(Tri { at, normal, color: c, glow, alpha: None, weapon: false });
    }
}

/// A see-through mesh (vertex alphas, as `FlatMesh::tri_faded` makes) in `color`, `alpha` opaque
/// at most.
fn see_through(mesh: &Mesh, place: Mat4, color: Vec3, alpha: f32, out: &mut Vec<Tri>) {
    let Some(VertexAttributeValues::Float32x3(p)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else { return };
    let Some(VertexAttributeValues::Float32x4(c)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR) else { return };
    let idx: Vec<u32> = match mesh.indices() {
        Some(Indices::U32(i)) => i.clone(),
        Some(Indices::U16(i)) => i.iter().map(|&v| v as u32).collect(),
        None => (0..p.len() as u32).collect(),
    };
    for t in idx.chunks(3) {
        let t = [t[0], t[1], t[2]];
        let at = t.map(|i| place.transform_point3(Vec3::from(p[i as usize])));
        out.push(Tri { at, normal: [Vec3::Y; 3], color, glow: true, alpha: Some(t.map(|i| c[i as usize][3] * alpha)), weapon: false });
    }
}

/// The figure in its rest pose: arms hanging, the weapon upright in the right hand.
fn figure(class: &str) -> Vec<Tri> {
    let rig = arena::fighter_rig(class);
    let moves = crate::rig::moves(class);
    let mut tris = Vec::new();
    let at = |p: Vec3| Mat4::from_translation(p);
    triangles(&arena::fighter_mesh(class), Mat4::IDENTITY, &mut tris, false);
    triangles(&rig.head, at(RIG_NECK), &mut tris, false);
    triangles(&rig.eyes, at(RIG_NECK), &mut tris, true);
    let left = Vec3::new(RIG_SHOULDER.x, RIG_SHOULDER.y, -RIG_SHOULDER.z);
    let lead = rig.lead_arm.as_ref();
    triangles(lead.map_or(&rig.arm, |(arm, _)| arm), at(left), &mut tris, false);
    triangles(&rig.arm, at(RIG_SHOULDER), &mut tris, false);
    triangles(&rig.forearm, at(RIG_SHOULDER + moves.elbow_at), &mut tris, false);
    if let Some(forearm) = lead.map_or(Some(&rig.forearm), |(_, forearm)| forearm.as_ref()) {
        triangles(forearm, at(left + moves.elbow_at), &mut tris, false);
    }
    triangles(&rig.held, at(RIG_SHOULDER + moves.hand_at), &mut tris, false);
    if let Some(glow) = &rig.held_glow {
        triangles(glow, at(RIG_SHOULDER + moves.hand_at), &mut tris, true);
    }
    if let Some(glow) = &rig.body_glow {
        triangles(glow, Mat4::IDENTITY, &mut tris, true);
    }
    for hip in [RIG_HIP, Vec3::new(RIG_HIP.x, RIG_HIP.y, -RIG_HIP.z)] {
        triangles(&rig.leg, at(hip), &mut tris, false);
        triangles(&rig.shin, at(hip + RIG_KNEE), &mut tris, false);
    }
    // Cloth at rest.
    for drape in rig.wardrobe.iter().flat_map(|wardrobe| &wardrobe.drapes) {
        triangles(&drape.mesh, Mat4::IDENTITY, &mut tris, false);
        if let Some(glow) = &drape.glow {
            triangles(glow, Mat4::IDENTITY, &mut tris, true);
        }
    }
    tris
}

/// One view looking along `-from` (orthographic), centered at `center`.
/// `span` world units across.
fn render(tris: &[Tri], from: Vec3, center: Vec3, span: f32, image: &mut [[u8; 3]], stride: usize, x0: usize) {
    let forward = -from.normalize();
    let right = forward.cross(Vec3::Y).normalize();
    let up = right.cross(forward);
    let scale = SIZE as f32 / span;
    let project = |p: Vec3| {
        let d = p - center;
        Some(Vec3::new(SIZE as f32 / 2.0 + d.dot(right) * scale, SIZE as f32 / 2.0 - d.dot(up) * scale, d.dot(forward)))
    };
    raster(tris, (SIZE, SIZE), &project, &|_| forward, image, stride, x0, None);
}

/// One view from `eye` looking at `look` (perspective, `fov` radians high), `size` pixels; and
/// where the held weapon shows in it (`Tri::weapon`), pixel by pixel (`SHOWS_SOLID`,
/// `SHOWS_CLEAR`).
fn render_view(tris: &[Tri], eye: Vec3, look: Vec3, fov: f32, size: (usize, usize)) -> (Vec<[u8; 3]>, Vec<u8>) {
    let forward = (look - eye).normalize();
    let right = forward.cross(Vec3::Y).normalize();
    let up = right.cross(forward);
    let (w, h) = (size.0 as f32, size.1 as f32);
    let focal = h / 2.0 / (fov / 2.0).tan();
    let project = |p: Vec3| {
        let d = p - eye;
        let z = d.dot(forward);
        (z > 0.05).then(|| Vec3::new(w / 2.0 + d.dot(right) / z * focal, h / 2.0 - d.dot(up) / z * focal, z))
    };
    let mut image = vec![[200u8, 204, 200]; size.0 * size.1];
    let mut mask = vec![0; size.0 * size.1];
    raster(tris, size, &project, &|p| (p - eye).normalize(), &mut image, size.0, 0, Some(&mut mask));
    (image, mask)
}

/// In `render_view`'s mask: the solid weapon shows there, or something see-through of it (its
/// trail) at least `CLEAR_SEEN` opaque.
const SHOWS_SOLID: u8 = 1;
const SHOWS_CLEAR: u8 = 2;
const CLEAR_SEEN: f32 = 0.1;

/// Draws `tris` where `project` puts them on screen (x, y in pixels, and depth), `size` pixels,
/// each lit as seen looking `along` to it, into `image` (`stride` wide, from column `x0`); and
/// marks where the weapon shows in `mask`, if asked.
#[allow(clippy::too_many_arguments)]
fn raster(tris: &[Tri], size: (usize, usize), project: &dyn Fn(Vec3) -> Option<Vec3>, along: &dyn Fn(Vec3) -> Vec3, image: &mut [[u8; 3]], stride: usize, x0: usize, mut mask: Option<&mut Vec<u8>>) {
    let (width, height) = size;
    let sun = Vec3::new(-0.6, 0.75, 0.35).normalize();
    let mut depth = vec![f32::INFINITY; width * height];
    // Everything solid first, then what's see-through over it.
    let (solid, clear): (Vec<&Tri>, Vec<&Tri>) = tris.iter().partition(|t| t.alpha.is_none());
    for t in solid.into_iter().chain(clear) {
        let [Some(a), Some(b), Some(c)] = t.at.map(project) else { continue };
        let s = [a, b, c];
        let area = (s[1].x - s[0].x) * (s[2].y - s[0].y) - (s[2].x - s[0].x) * (s[1].y - s[0].y);
        if area.abs() < 1e-9 {
            continue;
        }
        let (lo, hi) = (s[0].min(s[1]).min(s[2]), s[0].max(s[1]).max(s[2]));
        if hi.x < 0.0 || hi.y < 0.0 || lo.x > width as f32 || lo.y > height as f32 {
            continue;
        }
        let forward = along((t.at[0] + t.at[1] + t.at[2]) / 3.0);
        for y in (lo.y.floor().max(0.0) as usize)..=(hi.y.ceil().min(height as f32 - 1.0) as usize) {
            for x in (lo.x.floor().max(0.0) as usize)..=(hi.x.ceil().min(width as f32 - 1.0) as usize) {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let w0 = ((s[1].x - px) * (s[2].y - py) - (s[2].x - px) * (s[1].y - py)) / area;
                let w1 = ((s[2].x - px) * (s[0].y - py) - (s[0].x - px) * (s[2].y - py)) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let z = w0 * s[0].z + w1 * s[1].z + w2 * s[2].z;
                if z >= depth[y * width + x] {
                    continue;
                }
                if let Some(a) = t.alpha {
                    let a = (w0 * a[0] + w1 * a[1] + w2 * a[2]).clamp(0.0, 1.0);
                    let over = t.color.to_array().map(|c| c.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0);
                    let pixel = &mut image[y * stride + x0 + x];
                    for k in 0..3 {
                        pixel[k] = (pixel[k] as f32 * (1.0 - a) + over[k] * a) as u8;
                    }
                    if let Some(mask) = mask.as_deref_mut()
                        && t.weapon
                        && a >= CLEAR_SEEN
                    {
                        mask[y * width + x] |= SHOWS_CLEAR;
                    }
                    continue;
                }
                depth[y * width + x] = z;
                if let Some(mask) = mask.as_deref_mut() {
                    mask[y * width + x] = if t.weapon { SHOWS_SOLID } else { 0 };
                }
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
    let views = [Vec3::X, Vec3::new(1.0, 0.25, 0.9), Vec3::new(-1.0, 0.3, -0.8), crate::camera::OFFSET];
    for class in arena::FIGHTER_LOOKS {
        // The weapon alone, flat on and edge on.
        let rig = arena::fighter_rig(class);
        let mut weapon = Vec::new();
        triangles(&rig.held, Mat4::IDENTITY, &mut weapon, false);
        if let Some(glow) = &rig.held_glow {
            triangles(glow, Mat4::IDENTITY, &mut weapon, true);
        }
        // Edge on, flat on, and the head up close.
        let mut image = vec![[60u8, 70, 80]; 3 * SIZE * SIZE];
        for (i, (from, center, span)) in [(Vec3::X, Vec3::Y * 0.67, 2.6), (Vec3::new(0.3, 0.2, 1.0), Vec3::Y * 0.67, 2.6), (Vec3::Z, Vec3::Y * 1.62, 0.7)].into_iter().enumerate() {
            render(&weapon, from, center, span, &mut image, 3 * SIZE, i * SIZE);
        }
        std::fs::write(dir.join(format!("weapon-{class}.png")), png(3 * SIZE, SIZE, &image)).unwrap();

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


/// A fighter whose cloth lies on its arm (the javelinist's cowl), carrying (the arm overhead)
/// and drawn back, its cloth settled on the arm as it's held there, from around and above.
#[test]
#[ignore = "writes images; run by hand when working on models"]
fn preview_cloth_on_arm() {
    use crate::rig::{Phase, assemble, moves, targets};

    let dir = std::env::var("PREVIEW_DIR").map(std::path::PathBuf::from).unwrap_or_else(|_| std::env::temp_dir());
    let key = "javelinist";
    let moves = moves(key);
    let wardrobe = arena::fighter_rig(key).wardrobe.expect("the javelinist has cloth");
    let views = [Vec3::X, Vec3::new(1.0, 0.25, 0.9), Vec3::Z, Vec3::new(-1.0, 0.3, -0.8), Vec3::new(0.2, 1.0, 0.6), crate::camera::OFFSET];
    let width = SIZE * views.len();
    let mut image = vec![[200u8, 204, 200]; width * SIZE * 2];
    for (row, draw) in [0.0, 1.0].into_iter().enumerate() {
        let phase = Phase { draw, throw: 0.0, dashing: 0.0, second: 0.0 };
        let angles = targets(moves, phase, Vec2::ZERO, 0.0);
        let posed = assemble(moves, &angles, &angles, Vec2::X, 0.0, 0.0, 0.0, 1.0, 1.0 - phase.braced());
        let cloth = crate::cloth::settled(&wardrobe, &posed, 2.0);
        let (tris, _, _) = posed_figure(key, &posed, 1.0, Some(&cloth));
        for (i, from) in views.into_iter().enumerate() {
            render(&tris, from, Vec3::Y * 1.45, 1.4, &mut image[row * SIZE * width..], width, i * SIZE);
        }
    }
    std::fs::write(dir.join(format!("cloth-on-arm-{key}.png")), png(width, SIZE * 2, &image)).unwrap();
}

/// The figure posed as `posed` (facing +X, standing at the origin), its cloth hanging at rest
/// from the body (or as `cloth`, in the fighter's space); and where its weapon's grip and point
/// are (the point as low as any of the weapon goes).
fn posed_figure(class: &str, posed: &crate::rig::Posed, length: f32, cloth: Option<&[Mesh]>) -> (Vec<Tri>, Vec3, Vec3) {
    let rig = arena::fighter_rig(class);
    let mut tris = Vec::new();
    let place = |t: &Transform| Mat4::from_scale_rotation_translation(t.scale, t.rotation, t.translation);
    let body = place(&posed.body);
    let on = |at: Vec3, turn: Quat| body * Mat4::from_rotation_translation(turn, at);
    let mirrored = |p: Vec3| Vec3::new(p.x, p.y, -p.z);
    triangles(&arena::fighter_mesh(class), body, &mut tris, false);
    if let Some(glow) = &rig.body_glow {
        triangles(glow, body, &mut tris, true);
    }
    let head = on(RIG_NECK, posed.head);
    triangles(&rig.head, head, &mut tris, false);
    triangles(&rig.eyes, head, &mut tris, true);
    let arms = [on(RIG_SHOULDER, posed.arms[0]), on(mirrored(RIG_SHOULDER), posed.arms[1])];
    let elbow_at = crate::rig::moves(class).elbow_at;
    let elbows = [0, 1].map(|i| arms[i] * Mat4::from_rotation_translation(posed.elbows[i], elbow_at));
    for (i, (arm, elbow)) in arms.into_iter().zip(elbows).enumerate() {
        let lead = rig.lead_arm.as_ref().filter(|_| i == 1);
        triangles(lead.map_or(&rig.arm, |(arm, _)| arm), arm, &mut tris, false);
        if let Some(forearm) = lead.map_or(Some(&rig.forearm), |(_, forearm)| forearm.as_ref()) {
            triangles(forearm, elbow, &mut tris, false);
        }
    }
    for (i, hip) in [RIG_HIP, mirrored(RIG_HIP)].into_iter().enumerate() {
        let leg = on(hip, posed.legs[i]);
        triangles(&rig.leg, leg, &mut tris, false);
        triangles(&rig.shin, leg * Mat4::from_rotation_translation(posed.knees[i], RIG_KNEE), &mut tris, false);
    }
    let held = elbows[0] * place(&posed.held);
    let before = tris.len();
    triangles(&rig.held, held, &mut tris, false);
    let lowest = tris[before..].iter().flat_map(|t| t.at).map(|p| p.y).fold(f32::INFINITY, f32::min);
    if let Some(glow) = &rig.held_glow {
        triangles(glow, held, &mut tris, true);
    }
    tris[before..].iter_mut().for_each(|t| t.weapon = true);
    match cloth {
        Some(cloth) => cloth.iter().for_each(|mesh| triangles(mesh, body, &mut tris, false)),
        None => rig.wardrobe.iter().flat_map(|wardrobe| &wardrobe.drapes).for_each(|drape| triangles(&drape.mesh, body, &mut tris, false)),
    }
    // The point, as low as the weapon goes (y), wherever on it that is.
    (tris, held.transform_point3(Vec3::ZERO), held.transform_point3(Vec3::Y * length).with_y(lowest))
}

/// Flat ground around the origin, in half-meter checks, a darker line along +X (the facing).
fn ground(tris: &mut Vec<Tri>) {
    ground_in(tris, [Vec3::new(0.16, 0.2, 0.13), Vec3::new(0.13, 0.17, 0.1)], true);
}

/// Flat ground in checks of `colors` (linear), with the line along +X or not.
fn ground_in(tris: &mut Vec<Tri>, colors: [Vec3; 2], line: bool) {
    let quad = |tris: &mut Vec<Tri>, a: Vec3, b: Vec3, color: Vec3| {
        let (c, d) = (Vec3::new(b.x, 0.0, a.z), Vec3::new(a.x, 0.0, b.z));
        for at in [[a, c, b], [a, b, d]] {
            tris.push(Tri { at, normal: [Vec3::Y; 3], color, glow: false, alpha: None, weapon: false });
        }
    };
    for i in -6i32..10 {
        for j in -6..6 {
            let (x, z) = (i as f32 * 0.5, j as f32 * 0.5);
            let color = colors[(i + j).rem_euclid(2) as usize];
            quad(tris, Vec3::new(x, 0.0, z), Vec3::new(x + 0.5, 0.0, z + 0.5), color);
        }
    }
    if line {
        quad(tris, Vec3::new(-3.0, 0.002, -0.02), Vec3::new(5.0, 0.002, 0.02), Vec3::new(0.05, 0.05, 0.05));
    }
}

/// A heavy strike's impact as it breaks the ground: the fighter as it lands, the rift along its
/// blade with its rim, the dust half burst out and a few chunks in the air (`swish`), on
/// different grounds (a row each: meadow, stone, marsh), from the game's camera and up close:
/// `impact-<class>.png`.
#[test]
#[ignore = "writes images; run by hand when working on animation"]
fn preview_impact() {
    use arena_shared::protocol::ClassId;

    use crate::rig::{EMPOWERED_BLADE, EMPOWERED_STRETCH, Phase, assemble, moves, targets};
    use crate::arena::palette;

    use crate::swish::{CORE_THICKNESS, INK_THICKNESS, RIFT_ALPHA, RIM_ALPHA, bleed_shape, dust_shape, rift_shape, rubble_shape};

    let dir = std::env::var("PREVIEW_DIR").map(std::path::PathBuf::from).unwrap_or_else(|_| std::env::temp_dir());
    let key = "revenant";
    let class = ClassId::by_key(key).unwrap();
    let moves = moves(key);
    let landed = targets(moves, Phase { draw: 0.0, throw: 1.0, dashing: 0.0, second: 1.0 }, Vec2::ZERO, 0.0);
    // Plain, and grown to strike (`follow_up`): the blade grown, smeared out at its longest.
    for empowered in [false, true] {
    let (smear, blade) = if empowered { (moves.stretch + EMPOWERED_STRETCH, EMPOWERED_BLADE) } else { (0.0, 1.0) };
    let posed = assemble(moves, &landed, &landed, Vec2::X, 0.0, 0.0, smear, blade, 0.0);
    let (figure, grip, point) = posed_figure(key, &posed, arena::BLADE_LENGTH, None);
    println!("landed{}: grip {grip:5.2} point {point:5.2}", if empowered { " empowered, smeared" } else { "" });
    // Over the strike's hitbox, its lane, ahead (+X) of where it stands.
    // The plain break, grown to an empowered lane (as `swish::strike_ground` does).
    let ((plain_start, plain, plain_width), (start, range, width)) = (class.def().attack.lane(false).unwrap(), class.def().attack.lane(empowered).unwrap());
    let grown = Vec3::new((range - start) / (plain - plain_start), 1.0, width / plain_width);
    let (from, to) = (Vec3::new(start, 0.03, 0.0), Vec3::new(range, 0.03, 0.0));
    let length = plain - plain_start;
    let turn = Quat::IDENTITY;
    let linear = |c: Color| Vec3::from_slice(&c.to_linear().to_f32_array()[..3]);
    let mut effect = Vec::new();
    let at = |p: Vec3, scale: Vec3| Mat4::from_scale_rotation_translation(scale, turn, p);
    // As it settles (the light past its flare, breathing low), the mist half gone.
    see_through(&bleed_shape(length, plain_width, 7).build(), at(from - Vec3::Y * 0.006, grown), linear(palette::TATTERS), RIM_ALPHA, &mut effect);
    see_through(&rubble_shape(length, plain_width, 7).build(), at(from - Vec3::Y * 0.005, grown), linear(palette::TATTERS), RIM_ALPHA, &mut effect);
    see_through(&rift_shape(length, plain_width, 7, INK_THICKNESS).build(), at(from, grown), linear(palette::INK), RIFT_ALPHA, &mut effect);
    see_through(&rift_shape(length, plain_width, 7, CORE_THICKNESS).build(), at(from + Vec3::Y * 0.004, grown), linear(palette::WISP) * 1.4, 0.75, &mut effect);
    see_through(&dust_shape().build(), at((from + to) / 2.0 + Vec3::Y * 0.15, Vec3::new(length * grown.x + 0.9, 1.0, width + 0.9) * 0.9), linear(palette::WISP), 0.08, &mut effect);
    for i in 0..10 {
        let t = i as f32 / 9.0;
        let p = from.lerp(to, 0.1 + 0.8 * t) + Vec3::Z * if i % 2 == 0 { 0.3 } else { -0.35 } * (0.5 + t) + Vec3::Y * (0.2 + 0.5 * (i % 3) as f32 * 0.4);
        let mut chunk = Vec::new();
        triangles(&Mesh::from(Cuboid::new(1.0, 0.7, 0.85)), Mat4::from_scale_rotation_translation(Vec3::splat(0.06), Quat::from_rotation_y(i as f32) * Quat::from_rotation_x(i as f32 * 0.7), p), &mut chunk, false);
        let color = linear(if i % 3 == 0 { palette::ASH } else { palette::HUNTER_DARK });
        effect.extend(chunk.into_iter().map(|t| Tri { color, ..t }));
    }
    // The hitbox's outline (what the practice tools draw), to see the blade and the break on it.
    let mut outline = |a: Vec3, b: Vec3| {
        let side = (b - a).cross(Vec3::Y).normalize() * 0.012;
        for at in [[a - side, b - side, b + side], [a - side, b + side, a + side]] {
            effect.push(Tri { at: at.map(|p| p + Vec3::Y * 0.04), normal: [Vec3::Y; 3], color: Vec3::new(1.0, 0.15, 0.1), glow: true, alpha: None, weapon: false });
        }
    };
    let corners = [Vec3::new(start, 0.0, width / 2.0), Vec3::new(range, 0.0, width / 2.0), Vec3::new(range, 0.0, -width / 2.0), Vec3::new(start, 0.0, -width / 2.0)];
    for i in 0..4 {
        outline(corners[i], corners[(i + 1) % 4]);
    }
    println!("hitbox from {start} to {range} m, {width} m wide");
    let grounds = [[palette::MEADOW, palette::MOSS], [palette::STONE, palette::PATH], [palette::MARSH, palette::RIVERBED]];
    let mid = (start + range) / 2.0;
    let views: [(Vec3, Vec3, f32); 3] = [(crate::camera::OFFSET, Vec3::new(mid, 0.3, 0.0), range + 3.0), (Vec3::new(0.6, 0.8, 1.0), Vec3::new(mid, 0.2, 0.0), range + 1.5), (Vec3::new(0.05, 1.0, 0.0), Vec3::new(mid, 0.0, 0.0), range + 1.5)];
    let width = SIZE * views.len();
    let mut image = Vec::new();
    for colors in grounds {
        let mut tris: Vec<Tri> = Vec::new();
        ground_in(&mut tris, colors.map(|c| linear(c) * 0.33), false);
        tris.extend(figure.iter().copied());
        tris.extend(effect.iter().copied());
        let mut row = vec![[0u8; 3]; width * SIZE];
        for (i, (from, center, span)) in views.into_iter().enumerate() {
            render(&tris, from, center, span, &mut row, width, i * SIZE);
        }
        image.extend(row);
    }
    let name = if empowered { format!("impact-{key}-empowered.png") } else { format!("impact-{key}.png") };
    std::fs::write(dir.join(name), png(width, SIZE * grounds.len(), &image)).unwrap();
    }
}

/// The heavy attack, frame by frame: `attack-<class>.png`, a column per moment (carrying, through
/// the windup, the attack going off, the backswing), a row per view (from the weapon side, from
/// ahead and to the side, from the game's camera). The grip and point of each are printed, to
/// check it reaches the ground. The game's own posing (`rig::strike_phase`, `targets`, springs
/// at 60 fps, `assemble`), standing still facing +X.
#[test]
#[ignore = "writes images; run by hand when working on animation"]
fn preview_attacks() {
    use arena_shared::protocol::{AttackState, ClassId, Swing};

    use crate::rig::{Joints, Phase, ahead_of_springs, assemble, moves, strike_phase, targets};

    let dir = std::env::var("PREVIEW_DIR").map(std::path::PathBuf::from).unwrap_or_else(|_| std::env::temp_dir());
    const PER_SHEET: usize = 6;
    // One class, or every playable one and the zombie.
    let only = std::env::var("PREVIEW_CLASS").ok();
    let keys: Vec<&str> = ClassId::all().map(|c| c.def().figure()).filter(|k| only.as_deref().is_none_or(|o| o == *k)).collect::<std::collections::BTreeSet<_>>().into_iter().filter(|k| *k != "merchant").collect();
    for key in keys {
        let class = ClassId::by_key(key).unwrap();
        let moves = match std::env::var("PREVIEW_MOVES").ok().filter(|file| !file.is_empty()) {
            Some(file) => crate::rig::tweaked(key, &std::fs::read_to_string(file).unwrap()),
            None => moves(key),
        };
        let length = arena::blade_span(key).1;
        let swing = Swing { started_at: 10, dir: Vec2::X, quick: false, hastened: false, haste: 0.0, primed: false, held: 0, aside: 0.0 };
        let (windup, period) = (swing.windup_ticks(&class.def().attack) as f32, swing.ticks(&class.def().attack) as f32);
        let release = 10.0 + windup;
        // The moments to show, in ticks.
        let moments: Vec<(String, f32)> = [("carry", 9.0)]
            .into_iter()
            .map(|(n, t)| (n.to_string(), t))
            .chain([0.25, 0.5, 0.65, 0.8, 0.92].map(|f| (format!("windup {:.0}%", f * 100.0), 10.0 + windup * f)))
            .chain([0.0, 1.0, 2.5, 4.0, 5.5, 9.0].map(|t| (format!("release +{t}"), release + t)))
            .collect();
        // Or as `PREVIEW_VIEWS` has them: `x,y,z` each, `;` between.
        let views: Vec<Vec3> = std::env::var("PREVIEW_VIEWS").ok().filter(|v| !v.is_empty()).map_or_else(
            || vec![Vec3::Z, Vec3::new(1.0, 0.3, 0.7), Vec3::new(-0.4, 0.5, 1.0), crate::camera::OFFSET],
            |views| views.split(';').map(|v| Vec3::from_slice(&v.split(',').map(|c| c.trim().parse().unwrap()).collect::<Vec<f32>>())).collect(),
        );
        let width = SIZE * PER_SHEET;
        let sheets = moments.len().div_ceil(PER_SHEET);
        let mut rows = vec![vec![vec![[0u8; 3]; width * SIZE]; views.len()]; sheets];
        let mut joints = Joints::default();
        let dt = 1.0 / 60.0;
        let mut next = 0;
        let mut frame = 0;
        println!("{key}: windup {windup} ticks, attack {period} ticks; grip and point (x ahead, y up, z weapon side)");
        while next < moments.len() {
            let now = frame as f32 * dt * arena_shared::config::TICK_HZ as f32;
            frame += 1;
            let mut attack = AttackState::default();
            if now >= 10.0 && now < 10.0 + period {
                attack.swing = Some(swing);
            }
            if now >= release {
                attack.released_at = Some(release as u32);
            }
            let released = attack.released_at.map(|at| now - at as f32);
            let (shown, shown_released, ahead) = ahead_of_springs(class, moves, attack.winding_up(), released, now);
            let (draw, throw, smear, second) = strike_phase(class, moves, &attack, shown, shown_released, ahead);
            let phase = Phase { draw, throw, dashing: 0.0, second };
            let angles = joints.follow(&targets(moves, phase, Vec2::ZERO, 0.0), dt, moves);
            // The blade's point every frame through the strike, to catch it jumping sideways.
            if std::env::var_os("PREVIEW_TRACE").is_some() && key == "revenant" && now > release - 4.0 && now < release + 3.0 {
                let posed = assemble(moves, &angles, &targets(moves, phase, Vec2::ZERO, 0.0), Vec2::X, 0.0, 0.0, moves.stretch * smear, 1.0, 1.0 - phase.braced());
                let (_, _, point) = posed_figure(key, &posed, length, None);
                println!("    trace {:6.2}: phase ({draw:.2}, {throw:.3}) point {point:5.2} roll {:.3} wrist {:.3}", now - release, angles.roll, angles.tip);
            }
            if now < moments[next].1 {
                continue;
            }
            let posed = assemble(moves, &angles, &targets(moves, phase, Vec2::ZERO, 0.0), Vec2::X, 0.0, 0.0, moves.stretch * smear, 1.0, 1.0 - phase.braced());
            let (mut tris, grip, point) = posed_figure(key, &posed, length, None);
            let tip = posed.held_on_body.transform_point(Vec3::Y * arena::GRIP_TO_TIP);
            println!("  {:>14}: spear point (where a throw leaves from) {tip:5.2}", moments[next].0);
            let on_body = Mat4::from_scale_rotation_translation(posed.body.scale, posed.body.rotation, posed.body.translation).inverse().transform_point3(grip);
            println!("  {:>14}: phase ({draw:.2}, {throw:.2}) grip {grip:5.2} (on the body {on_body:5.2}) body at {:.3} point {point:5.2} arm {:.2} lean {:.2} twist {:.2} wrist {:.2} roll {:.2}", moments[next].0, posed.body.translation.y, angles.weapon_arm, angles.lean, angles.twist, angles.tip, angles.roll);
            // The lead hand, and how far it is off its grip on the weapon, if it's meant to be on.
            if angles.hold > 0.01 {
                let lead_hand = posed.body.transform_point(RIG_SHOULDER.with_z(-RIG_SHOULDER.z) + posed.arms[1] * (moves.elbow_at + posed.elbows[1] * (moves.hand_at - moves.elbow_at)));
                let on_weapon = posed.held_on_body.transform_point(Vec3::Y * moves.lead_grip / posed.held.scale.y);
                let shoulder = posed.body.transform_point(RIG_SHOULDER.with_z(-RIG_SHOULDER.z));
                let reach = moves.elbow_at.length() + (moves.hand_at - moves.elbow_at).length();
                println!("  {:>14}: lead hand {lead_hand:5.2}, {:.3} m off its grip at {on_weapon:5.2}, {:.2} m from the shoulder (reach {reach:.2}, held {:.2})", moments[next].0, lead_hand.distance(on_weapon), shoulder.distance(on_weapon), angles.hold);
            }
            ground(&mut tris);
            for (row, &from) in rows[next / PER_SHEET].iter_mut().zip(&views) {
                let mut cell = vec![[200u8, 204, 200]; SIZE * SIZE];
                render(&tris, from, Vec3::new(0.5, 1.2, 0.0), 5.2, &mut cell, SIZE, 0);
                for y in 0..SIZE {
                    let at = y * width + (next % PER_SHEET) * SIZE;
                    row[at..at + SIZE].copy_from_slice(&cell[y * SIZE..(y + 1) * SIZE]);
                }
            }
            next += 1;
        }
        for (i, sheet) in rows.iter().enumerate() {
            let image: Vec<[u8; 3]> = sheet.concat();
            std::fs::write(dir.join(format!("attack-{key}-{}.png", i + 1)), png(width, SIZE * views.len(), &image)).unwrap();
        }
    }
}

/// A class's attack as the action camera sees it, over its own shoulder (`PREVIEW_CLASS`,
/// default the revenant), striking along the crosshair's line: `view-<class>-<pitch>.png`, every
/// frame at 60 fps from the windup into the follow-through, the crosshair ringed,
/// for each of a few pitches of the camera (`PREVIEW_PITCHES`, radians, up positive: looking
/// down at the ground); and, frame by frame, how near the screen's middle its weapon and its
/// trail (`swish::trail`) come (as a share of the screen's height), to catch them passing over
/// the crosshair. With its moves changed as `PREVIEW_MOVES` (a file) has them
/// (`rig::tweaked`), to try poses out without rebuilding.
#[test]
#[ignore = "writes images; run by hand when working on animation"]
fn preview_action_view() {
    use arena_shared::config::{ACTION_LOOK_HEIGHT, ACTION_SHOULDER, TICK_HZ};
    use arena_shared::protocol::{AttackState, ClassId, Swing};

    use crate::rig::{EMPOWERED_BLADE, EMPOWERED_STRETCH, Joints, Phase, ahead_of_springs, assemble, drawn_size, lane_tilt, moves, strike_phase, targets, tweaked};
    use crate::swish::{TRAIL_DIM, TRAIL_LIFE, TRAIL_SPEED};

    let dir = std::env::var("PREVIEW_DIR").map(std::path::PathBuf::from).unwrap_or_else(|_| std::env::temp_dir());
    let key = std::env::var("PREVIEW_CLASS").unwrap_or_else(|_| "revenant".to_string());
    let pitches: Vec<f32> = std::env::var("PREVIEW_PITCHES").map_or_else(|_| vec![-0.15, 0.1, 0.35], |p| p.split(',').map(|p| p.trim().parse().unwrap()).collect());
    let class = ClassId::by_key(&key).unwrap();
    let def = class.def();
    let moves = match std::env::var("PREVIEW_MOVES").ok().filter(|file| !file.is_empty()) {
        Some(file) => tweaked(&key, &std::fs::read_to_string(file).unwrap()),
        None => moves(&key),
    };
    let (from, length) = arena::blade_span(&key);
    let size = drawn_size(class, false);
    // Its blade growing to strike (`rig::EMPOWERED_BLADE`), unless `PREVIEW_PLAIN`.
    let grows = def.attack.follow_up.is_some() && std::env::var_os("PREVIEW_PLAIN").is_none();
    let swing = Swing { started_at: 10, dir: Vec2::X, quick: false, hastened: false, haste: 0.0, primed: false, held: 0, aside: ACTION_SHOULDER };
    let (windup, period) = (swing.windup_ticks(&def.attack) as f32, swing.ticks(&def.attack) as f32);
    let release = 10.0 + windup;
    const CELL: (usize, usize) = (640, 360);
    const PER_ROW: usize = 6;
    // The crosshair's ring, as a share of the screen's height.
    const RING: f32 = 0.025;
    let (camera_distance, fov) = (4.4, 1.05);
    for pitch in pitches {
        // Facing +X, the weapon side +Z: the camera looks along the crosshair's line, over the
        // weapon shoulder.
        let look = Vec3::new(0.0, ACTION_LOOK_HEIGHT, ACTION_SHOULDER);
        let eye = look + Vec3::new(-pitch.cos(), pitch.sin(), 0.0) * camera_distance;
        let eye = eye.with_y(eye.y.max(0.4));
        let mut joints = Joints::default();
        let dt = 1.0 / 60.0;
        let mut cells = Vec::new();
        let mut nearest = Vec::new();
        // Where the blade has been lately (seconds; its base, middle and point), for its trail.
        let mut trail: std::collections::VecDeque<(f32, [Vec3; 3])> = Default::default();
        for frame in 0.. {
            let now = frame as f32 * dt * TICK_HZ as f32;
            if now > 10.0 + period + 6.0 {
                break;
            }
            let mut attack = AttackState::default();
            if now >= 10.0 && now < 10.0 + period {
                attack.swing = Some(swing);
            }
            if now >= release {
                attack.released_at = Some(release as u32);
            }
            let released = attack.released_at.map(|at| now - at as f32);
            let (shown, shown_released, ahead) = ahead_of_springs(class, moves, attack.winding_up(), released, now);
            let (draw, throw, smear, second) = strike_phase(class, moves, &attack, shown, shown_released, ahead);
            let phase = Phase { draw, throw, dashing: 0.0, second };
            // Posed as `rig::pose_rigs` does, the blow onto the crosshair's line.
            let grown = if grows { draw.max(throw) } else { 0.0 };
            let blade = 1.0 + (EMPOWERED_BLADE - 1.0) * grown;
            let stretch = moves.stretch + if grows { EMPOWERED_STRETCH } else { 0.0 };
            let mut target = targets(moves, phase, Vec2::ZERO, 0.0);
            target.arm_tilt = lane_tilt(moves, ACTION_SHOULDER, size, if grows { EMPOWERED_BLADE } else { 1.0 }) * throw;
            let angles = joints.follow(&target, dt, moves);
            let posed = assemble(moves, &angles, &target, Vec2::X, 0.0, 0.0, stretch * smear, blade, 1.0 - phase.braced());
            let secs = frame as f32 * dt;
            trail.push_back((secs, [from / length, (from / length + 1.0) / 2.0, 1.0].map(|up| posed.held_on_body.transform_point(Vec3::Y * length * up) * size)));
            while trail.front().is_some_and(|(at, _)| secs - at > TRAIL_LIFE) {
                trail.pop_front();
            }
            if now < 9.0 || now > 10.0 + period + 6.0 {
                continue;
            }
            let (mut tris, grip, point) = posed_figure(&key, &posed, length * blade, None);
            let (grip, point) = (grip * size, point * size);
            tris.iter_mut().for_each(|t| t.at = t.at.map(|p| p * size));
            ground_in(&mut tris, [Vec3::new(0.16, 0.2, 0.13), Vec3::new(0.13, 0.17, 0.1)], false);
            // Its trail as `swish::trail` draws it: by how fast the point was going, and how fresh.
            let alpha = |i: usize| {
                let (at, place) = trail[i];
                let speed = if i == 0 { 0.0 } else { place[2].distance(trail[i - 1].1[2]) / (at - trail[i - 1].0).max(1e-4) };
                let fast = ((speed - TRAIL_SPEED.0) / (TRAIL_SPEED.1 - TRAIL_SPEED.0)).clamp(0.0, 1.0);
                let fresh = 1.0 - (secs - at) / TRAIL_LIFE;
                fast * fresh * fresh * TRAIL_DIM
            };
            for i in 1..trail.len() {
                let (a0, a1) = (alpha(i - 1), alpha(i));
                let (p0, p1) = (trail[i - 1].1, trail[i].1);
                let shade = [0.0, 0.6, 1.0];
                for k in 0..2 {
                    let (lo, hi) = ((a0 * shade[k], a0 * shade[k + 1]), (a1 * shade[k], a1 * shade[k + 1]));
                    for (at, alpha) in [([p0[k], p0[k + 1], p1[k + 1]], [lo.0, lo.1, hi.1]), ([p0[k], p1[k + 1], p1[k]], [lo.0, hi.1, hi.0])] {
                        tris.push(Tri { at, normal: [Vec3::Y; 3], color: Vec3::splat(0.9), glow: true, alpha: Some(alpha), weapon: true });
                    }
                }
            }
            let (mut image, mask) = render_view(&tris, eye, look, fov, CELL);
            let (cx, cy) = (CELL.0 as f32 / 2.0, CELL.1 as f32 / 2.0);
            let from_middle = |i: usize| (((i % CELL.0) as f32 + 0.5 - cx).powi(2) + ((i / CELL.0) as f32 + 0.5 - cy).powi(2)).sqrt() / CELL.1 as f32;
            let nearest_of = |shows: u8| mask.iter().enumerate().filter(|(_, m)| **m & shows != 0).map(|(i, _)| from_middle(i)).fold(f32::INFINITY, f32::min);
            let (near, near_trail) = (nearest_of(SHOWS_SOLID), nearest_of(SHOWS_CLEAR));
            // The ring: red with the weapon in it, orange with its trail.
            let ring = if near < RING { [230, 20, 20] } else if near_trail < RING { [255, 150, 0] } else { [255, 255, 255] };
            for (i, pixel) in image.iter_mut().enumerate() {
                let r = from_middle(i);
                if (r - RING).abs() < 0.0035 || r < 0.004 {
                    *pixel = ring;
                }
            }
            nearest.push((now - release, draw, throw, near, near_trail, grip, point));
            cells.push(image);
        }
        println!("{key} at pitch {pitch}: the weapon's and its trail's nearest to the middle (share of the screen's height; ring {RING})");
        for (t, draw, throw, near, near_trail, grip, point) in &nearest {
            let mark = if *near < RING { "  <-- weapon over the crosshair" } else if *near_trail < RING { "  <-- trail over the crosshair" } else { "" };
            println!("  {t:+6.2} ticks  phase ({draw:.2}, {throw:.2})  {near:.3}  trail {near_trail:.3}  grip {grip:5.2} point {point:5.2}{mark}");
        }
        let worst = |of: fn(&(f32, f32, f32, f32, f32, Vec3, Vec3)) -> f32| nearest.iter().map(of).fold(f32::INFINITY, f32::min);
        println!("  nearest {:.3}, trail {:.3}", worst(|n| n.3), worst(|n| n.4));
        let rows = cells.len().div_ceil(PER_ROW);
        let width = CELL.0 * PER_ROW;
        let mut sheet = vec![[40u8, 40, 40]; width * CELL.1 * rows];
        for (n, cell) in cells.iter().enumerate() {
            let (row, col) = (n / PER_ROW, n % PER_ROW);
            for y in 0..CELL.1 {
                let at = (row * CELL.1 + y) * width + col * CELL.0;
                sheet[at..at + CELL.0].copy_from_slice(&cell[y * CELL.0..(y + 1) * CELL.0]);
            }
        }
        std::fs::write(dir.join(format!("view-{key}-{pitch}.png")), png(width, CELL.1 * rows, &sheet)).unwrap();
    }
}

/// Every figure's walk: `walk-<class>.png`, a column per point in one stride (the pose the joints
/// head for, at full walk, facing +X), a row per view (from the side, from ahead and to the side,
/// from the game's camera). To see each figure's gait: how its knees lift, how it bobs and sways
/// and swings its arms.
#[test]
#[ignore = "writes images; run by hand when working on animation"]
fn preview_walk() {
    use std::f32::consts::TAU;

    use arena_shared::protocol::ClassId;

    use crate::rig::{Phase, assemble, moves, targets};

    let dir = std::env::var("PREVIEW_DIR").map(std::path::PathBuf::from).unwrap_or_else(|_| std::env::temp_dir());
    const STEPS: usize = 6;
    let only = std::env::var("PREVIEW_CLASS").ok();
    let keys: Vec<&str> = ClassId::all().map(|c| c.def().figure()).filter(|k| only.as_deref().is_none_or(|o| o == *k)).collect::<std::collections::BTreeSet<_>>().into_iter().filter(|k| *k != "merchant").collect();
    let views = [Vec3::Z, Vec3::new(1.0, 0.3, 0.7), crate::camera::OFFSET];
    for key in keys {
        let moves = moves(key);
        let width = SIZE * STEPS;
        let mut image = vec![[200u8, 204, 200]; width * SIZE * views.len()];
        for i in 0..STEPS {
            let stride = i as f32 / STEPS as f32 * TAU;
            let step = Vec2::new(stride.sin(), stride.cos());
            let angles = targets(moves, Phase::default(), step, 1.0);
            let posed = assemble(moves, &angles, &angles, Vec2::X, stride, 1.0, 0.0, 1.0, 1.0);
            let (mut tris, _, _) = posed_figure(key, &posed, 1.0, None);
            ground(&mut tris);
            for (row, from) in views.into_iter().enumerate() {
                let mut cell = vec![[200u8, 204, 200]; SIZE * SIZE];
                render(&tris, from, Vec3::new(0.2, 1.0, 0.0), 3.2, &mut cell, SIZE, 0);
                for y in 0..SIZE {
                    let at = (row * SIZE + y) * width + i * SIZE;
                    image[at..at + SIZE].copy_from_slice(&cell[y * SIZE..(y + 1) * SIZE]);
                }
            }
        }
        std::fs::write(dir.join(format!("walk-{key}.png")), png(width, SIZE * views.len(), &image)).unwrap();
    }
}

/// Every figure's fidgets standing still (`rig::Idle`): `idle-<class>.png`, a column each for
/// standing as it carries, its gesture made, and its weight settled onto the back foot and onto
/// the lead one; a row per view (from the side, from ahead and to the side, from the game's
/// camera).
#[test]
#[ignore = "writes images; run by hand when working on animation"]
fn preview_idle() {
    use arena_shared::protocol::ClassId;

    use crate::rig::{Phase, assemble, moves, targets};

    let dir = std::env::var("PREVIEW_DIR").map(std::path::PathBuf::from).unwrap_or_else(|_| std::env::temp_dir());
    let only = std::env::var("PREVIEW_CLASS").ok();
    let keys: Vec<&str> = ClassId::all().map(|c| c.def().figure()).filter(|k| only.as_deref().is_none_or(|o| o == *k)).collect::<std::collections::BTreeSet<_>>().into_iter().filter(|k| *k != "merchant").collect();
    let views = [Vec3::Z, Vec3::new(1.0, 0.3, 0.7), crate::camera::OFFSET];
    // How far into its gesture, and which way its weight is rocked, in each column.
    const COLUMNS: [(f32, f32); 4] = [(0.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)];
    for key in keys {
        let moves = moves(key);
        let width = SIZE * COLUMNS.len();
        let mut image = vec![[200u8, 204, 200]; width * SIZE * views.len()];
        for (i, (making, rocking)) in COLUMNS.into_iter().enumerate() {
            let mut angles = targets(moves, Phase::default(), Vec2::ZERO, 0.0);
            angles.add(&moves.idle.posed(making, rocking, 1.0), 1.0);
            let posed = assemble(moves, &angles, &angles, Vec2::X, 0.0, 0.0, 0.0, 1.0, 1.0);
            let (mut tris, _, _) = posed_figure(key, &posed, 1.0, None);
            ground(&mut tris);
            for (row, from) in views.into_iter().enumerate() {
                let mut cell = vec![[200u8, 204, 200]; SIZE * SIZE];
                render(&tris, from, Vec3::new(0.2, 1.0, 0.0), 3.2, &mut cell, SIZE, 0);
                for y in 0..SIZE {
                    let at = (row * SIZE + y) * width + i * SIZE;
                    image[at..at + SIZE].copy_from_slice(&cell[y * SIZE..(y + 1) * SIZE]);
                }
            }
        }
        std::fs::write(dir.join(format!("idle-{key}.png")), png(width, SIZE * views.len(), &image)).unwrap();
    }
}

/// Every map from above, one pixel image per map (`map-<id>.png`): tiles in the theme's colors,
/// lairs red, chests gold, pickups green, spawns blue, the shop white. The bastion's staged
/// variants too: the layout and its unlocks, seen without running the game.
#[test]
#[ignore = "writes images; run by hand when working on maps"]
fn preview_maps() {
    use arena_shared::map::{Map, MapId};

    let dir = std::env::var("PREVIEW_DIR").map(std::path::PathBuf::from).unwrap_or_else(|_| std::env::temp_dir());
    for id in [MapId::Crossing, MapId::Keep, MapId::Gorge, MapId::Wilds, MapId::Bastion, MapId::Bastion2, MapId::Bastion3, MapId::Bastion4, MapId::Hold, MapId::Yard] {
        let m = id.map();
        let theme = crate::arena::Theme::of(id);
        const S: usize = 4;
        let (w, h) = (m.size.x as usize * S, m.size.y as usize * S);
        let mut image = vec![[0u8; 3]; w * h];
        let spot = |t: bevy::math::IVec2| (((t.x + m.size.x / 2) as usize) * S, ((m.size.y / 2 - 1 - t.y) as usize) * S);
        let paint = |image: &mut Vec<[u8; 3]>, t: bevy::math::IVec2, rgb: [u8; 3]| {
            let (x0, y0) = spot(t);
            for dy in 0..S {
                for dx in 0..S {
                    image[(y0 + dy) * w + x0 + dx] = rgb;
                }
            }
        };
        for (t, tile) in m.tiles() {
            let c = crate::arena::tile_color(theme, tile).to_srgba();
            paint(&mut image, t, [(c.red * 255.0) as u8, (c.green * 255.0) as u8, (c.blue * 255.0) as u8]);
        }
        let dot = |image: &mut Vec<[u8; 3]>, p: bevy::math::Vec2, rgb: [u8; 3]| paint(image, Map::tile_of(p), rgb);
        for &p in &m.lairs {
            dot(&mut image, p, [220, 40, 40]);
        }
        for &p in &m.chests {
            dot(&mut image, p, [240, 200, 60]);
        }
        for &(p, _) in &m.pickups {
            dot(&mut image, p, [80, 230, 120]);
        }
        for &p in &m.spawn_points {
            dot(&mut image, p, [80, 160, 255]);
        }
        if let Some(shop) = m.shop {
            dot(&mut image, shop, [255, 255, 255]);
        }
        std::fs::write(dir.join(format!("map-{id:?}.png")), png(w, h, &image)).unwrap();
    }
}
