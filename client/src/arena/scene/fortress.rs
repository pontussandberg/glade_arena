//! A fortress's stonework (on a map whose theme builds `Theme::battlements`): its towers, its
//! walls laid in courses, the gatehouse over the gate, and the
//! banners hung from it all.

use super::*;

use arena_shared::map::{TREAD, Tower};

/// How tall a course of a wall's or a tower's stones is, give or take.
const COURSE: f32 = 0.78;
/// The least height of stone face laid in courses (lower ones are one block).
pub(super) const COURSED: f32 = 1.6;

/// Slate for the towers' roofs, the deep red of the banners, and the dark of an arrow slit.
const SLATE_ROOF: Color = Color::srgb_u8(0x3A, 0x3F, 0x4C);
const BANNER: Color = Color::srgb_u8(0x6A, 0x1A, 0x20);
const SLIT: Color = Color::srgb_u8(0x14, 0x14, 0x18);
/// How high the arch over the gate is (absolute): its underside, over anyone's reach from the
/// passage's floor, and its top.
const GATE_ARCH: (f32, f32) = (7.6, 9.6);

impl Props {
    /// A plain box (no bevel: for the many stones of a wall's courses) of `size`, its middle `at`
    /// `height` meters up (not over the ground there: absolute).
    fn stone(&mut self, size: Vec3, color: Color, at: Vec2, height: f32) {
        let mesh = faceted(Cuboid::new(size.x, size.y, size.z).mesh().build()).translated_by(to_world(at, height));
        self.push(mesh, color);
    }

    /// A stone face laid in courses along a tile's side: `size` its whole extent (across the
    /// side, up, and its thickness), its middle `at`, from `foot` up (absolute). The courses
    /// alternate a little proud and set back, each its own shade, every other one split into two
    /// stones, so the joints don't run straight up.
    pub(super) fn coursed(&mut self, size: Vec3, color: Color, at: Vec2, foot: f32, along_x: bool, rng: &mut Lcg) {
        let courses = (size.y / COURSE).round().max(1.0) as usize;
        let h = size.y / courses as f32;
        for k in 0..courses {
            let shade = color.mix(&self.theme.wall, rng.range(0.0, 0.45)).darker(rng.range(0.0, 0.06));
            let proud = if k % 2 == 0 { 0.02 } else { -0.01 };
            let mid = foot + h * (k as f32 + 0.5);
            let (width, deep) = if along_x { (size.x, size.z + proud) } else { (size.z, size.x + proud) };
            let pieces = if k % 2 == 0 { 1 } else { 2 };
            for i in 0..pieces {
                let w = width / pieces as f32;
                let off = (i as f32 + 0.5) * w - width / 2.0;
                let shade = if i == 0 { shade } else { shade.mix(&self.theme.stone, rng.range(0.0, 0.3)) };
                let (p, s) = if along_x { (at + Vec2::X * off, Vec3::new(w - 0.015, h - 0.03, deep)) } else { (at + Vec2::Y * off, Vec3::new(deep, h - 0.03, w - 0.015)) };
                self.stone(s, shade, p, mid);
            }
        }
    }

    /// Stone stairs up every paved ramp (`Map::stairs_up`): treads `TREAD` deep, each a block
    /// from the slope's foot under it up to its top, so the slope reads as steps.
    pub(super) fn stairs(&mut self, m: &Map, rng: &mut Lcg) {
        for (t, tile) in m.tiles() {
            let Some(up) = m.stairs_up(t) else { continue };
            let c = Map::center(t);
            let base = if tile == Tile::Bridge { self.theme.stone } else { self.theme.path.mix(&self.theme.stone, 0.5) };
            let steps = (1.0 / TREAD) as usize;
            for k in 0..steps {
                let mid = c + up * (TREAD * (k as f32 + 0.5) - 0.5);
                let (foot, top) = (m.height_at(mid - up * TREAD / 2.0), m.height_at(mid + up * TREAD / 2.0));
                let size = if up.x != 0.0 { Vec3::new(TREAD, top - foot + 0.06, 1.0) } else { Vec3::new(1.0, top - foot + 0.06, TREAD) };
                self.stone(size, base.darker(rng.range(0.0, 0.08)), mid, (top + foot - 0.06) / 2.0);
                // The nosing: a darker edge along the tread's front.
                let nose = if up.x != 0.0 { Vec3::new(0.06, 0.05, 1.0) } else { Vec3::new(1.0, 0.05, 0.06) };
                self.stone(nose, base.darker(0.25), mid - up * (TREAD / 2.0 - 0.03), top - 0.02);
            }
        }
    }

    /// Fallen stones (`Map::rubble`): rough blocks lying every which way, half sunk in the ground.
    pub(super) fn rubble(&mut self, m: &Map, rng: &mut Lcg) {
        for &(at, size) in &m.rubble {
            let color = self.theme.stone.mix(&self.theme.wall, rng.range(0.2, 0.7)).darker(rng.range(0.0, 0.12));
            let lie = Quat::from_rotation_y(rng.range(0.0, std::f32::consts::TAU)) * Quat::from_rotation_x(rng.range(-0.35, 0.35)) * Quat::from_rotation_z(rng.range(-0.35, 0.35));
            let dims = Vec3::new(size, size * rng.range(0.45, 0.7), size * rng.range(0.6, 0.9));
            self.block(dims, color, Transform::from_translation(to_world(at, dims.y * 0.25)).with_rotation(lie));
        }
    }

    /// The map's towers (`Map::towers`): a shaft of coursed stone on a battered footing, quoins
    /// at its corners, a string course, arrow slits up its faces; at the top a parapet jutting
    /// out on corbels, crenellated; and over it a tall slate spire with a finial and a pennant,
    /// or (a battlemented tower) a banner pole. The keep (the biggest) is lit at its windows and
    /// has a turret at each corner.
    pub(super) fn towers(&mut self, m: &Map, rng: &mut Lcg) {
        let biggest = m.towers.iter().map(|t| t.size().x * t.size().y).fold(0.0, f32::max);
        for tower in &m.towers {
            self.tower(m, tower, tower.size().x * tower.size().y >= biggest, rng);
        }
        self.gatehouse(m);
    }

    fn tower(&mut self, m: &Map, tower: &Tower, keep: bool, rng: &mut Lcg) {
        let (c, size) = (tower.center(), tower.size());
        // The ground round it at its lowest: the footing's steps stand over that.
        let ground = m.tower_ground(tower);
        let stone = self.theme.stone.mix(&self.theme.wall, rng.range(0.1, 0.3));
        let (base, top) = (tower.base, tower.top);
        // The footing: battered out, two steps (`Tower::FOOTING`).
        let [(low_out, low_top), (high_out, foot_top)] = Tower::FOOTING.map(|(out, top)| (out, ground + top));
        self.stone(Vec3::new(size.x + 2.0 * low_out, low_top - base, size.y + 2.0 * low_out), stone.darker(0.12), c, (base + low_top) / 2.0);
        self.stone(Vec3::new(size.x + 2.0 * high_out, foot_top - low_top, size.y + 2.0 * high_out), stone.darker(0.06), c, (low_top + foot_top) / 2.0);
        // The shaft, in courses, each its own shade.
        let shaft_top = top - 1.1;
        let courses = ((shaft_top - foot_top) / COURSE).round().max(1.0) as usize;
        let h = (shaft_top - foot_top) / courses as f32;
        for k in 0..courses {
            let shade = stone.mix(&self.theme.wall, rng.range(0.0, 0.4)).darker(rng.range(0.0, 0.05));
            let proud = if k % 2 == 0 { 0.03 } else { 0.0 };
            self.stone(Vec3::new(size.x + proud, h - 0.03, size.y + proud), shade, c, foot_top + h * (k as f32 + 0.5));
            // Quoins: paler stones at the corners, long and short by turns.
            let long = if k % 2 == 0 { 0.75 } else { 0.45 };
            for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                let corner = c + Vec2::new(sx * size.x, sy * size.y) / 2.0;
                let (qx, qy) = if k % 2 == 0 { (long, 0.45) } else { (0.45, long) };
                let at = corner - Vec2::new(sx * (qx / 2.0 - 0.04), sy * (qy / 2.0 - 0.04));
                self.stone(Vec3::new(qx, h - 0.04, qy), self.theme.stone.lighter(0.06).mix(&shade, 0.3), at, foot_top + h * (k as f32 + 0.5));
            }
        }
        // A string course a little over halfway up.
        let band = foot_top + (shaft_top - foot_top) * 0.58;
        self.stone(Vec3::new(size.x + 0.22, 0.22, size.y + 0.22), stone.darker(0.1), c, band);
        // Arrow slits (or, on the keep, lit windows) up each face.
        let faces = [(Vec2::X, size.y, size.x), (Vec2::NEG_X, size.y, size.x), (Vec2::Y, size.x, size.y), (Vec2::NEG_Y, size.x, size.y)];
        for (out, run, depth) in faces {
            let count = ((run / 3.0).floor() as usize).max(1);
            let mut height = foot_top + 2.2;
            let mut row = 0;
            while height < shaft_top - 1.2 {
                for i in 0..count {
                    let off = ((i as f32 + 0.5) / count as f32 - 0.5) * run;
                    // Staggered row by row on the narrow towers, so they read as a stair inside.
                    let off = if count == 1 { off + if row % 2 == 0 { -0.6 } else { 0.6 } * (run / 5.0).min(1.0) } else { off };
                    let at = c + out * (depth / 2.0 + 0.02) + out.perp() * off;
                    // Across the face (`w`), up it (`tall`), out from it (`deep`).
                    let slab = |w: f32, tall: f32, deep: f32| if out.x != 0.0 { Vec3::new(deep, tall, w) } else { Vec3::new(w, tall, deep) };
                    if keep {
                        // A dark reveal; in it two lights, warm, split by a stone mullion and
                        // crossed by a transom; a sill and a lintel.
                        let (w, tall) = (0.7, 1.4);
                        self.stone(slab(w + 0.16, tall + 0.14, 0.06), SLIT, at, height);
                        for side in [-1.0, 1.0] {
                            self.stone(slab(w / 2.0 - 0.07, tall - 0.08, 0.06), WINDOW.darker(0.12), at + out * 0.012 + out.perp() * side * w / 4.0, height);
                        }
                        self.stone(slab(0.08, tall, 0.09), stone.darker(0.18), at + out * 0.02, height);
                        self.stone(slab(w, 0.07, 0.09), stone.darker(0.18), at + out * 0.02, height + tall * 0.18);
                        self.stone(slab(w + 0.3, 0.13, 0.16), stone.darker(0.12), at, height - tall / 2.0 - 0.1);
                        self.stone(slab(w + 0.3, 0.2, 0.14), stone.darker(0.12), at, height + tall / 2.0 + 0.12);
                    } else {
                        self.stone(slab(0.13, 0.95, 0.06), SLIT, at, height);
                    }
                }
                height += if keep { 3.4 } else { 2.9 };
                row += 1;
            }
        }
        // The top: corbels under a parapet jutting out all round, crenellated.
        let jut = 0.38;
        let (outer, deck) = (size + Vec2::splat(2.0 * jut), top - 0.5);
        for (out, run, depth) in faces {
            let n = (run / 0.7).floor() as i32;
            for i in 0..n {
                let off = ((i as f32 + 0.5) / n as f32 - 0.5) * run;
                let at = c + out * (depth / 2.0 + jut / 2.0) + out.perp() * off;
                let s = if out.x != 0.0 { Vec3::new(jut, 0.5, 0.28) } else { Vec3::new(0.28, 0.5, jut) };
                self.stone(s, stone.darker(0.08), at, shaft_top - 0.25);
            }
        }
        self.stone(Vec3::new(outer.x, 1.1, outer.y), stone.mix(&self.theme.wall, 0.2), c, deck - 0.05);
        for (out, run, depth) in [(Vec2::X, outer.y, outer.x), (Vec2::NEG_X, outer.y, outer.x), (Vec2::Y, outer.x, outer.y), (Vec2::NEG_Y, outer.x, outer.y)] {
            // The parapet's low wall round the roof, the merlons standing on it.
            let wall = if out.x != 0.0 { Vec3::new(0.34, 0.56, run) } else { Vec3::new(run, 0.56, 0.34) };
            self.stone(wall, stone.darker(0.02), c + out * (depth / 2.0 - 0.17), top + 0.23);
            let n = ((run / 1.1).round() as i32).max(2);
            for i in 0..n {
                let off = ((i as f32 + 0.5) / n as f32 - 0.5) * (run - 0.3);
                let at = c + out * (depth / 2.0 - 0.15) + out.perp() * off;
                let s = if out.x != 0.0 { Vec3::new(0.3, 0.7, 0.55) } else { Vec3::new(0.55, 0.7, 0.3) };
                self.stone(s, stone.darker(0.04), at, top + 0.85);
            }
        }
        if tower.roof {
            // A slate spire over the parapet (a pyramid stretched to the tower's plan), a finial
            // and a pennant on its point.
            let width = size.max_element() + 0.5;
            let rise = width * if keep { 0.75 } else { 1.15 } + 1.5;
            let spire = faceted(cone(width / 2.0 * std::f32::consts::SQRT_2, rise, 4))
                .rotated_by(Quat::from_rotation_y(std::f32::consts::FRAC_PI_4))
                .scaled_by(Vec3::new((size.x + 0.5) / width, 1.0, (size.y + 0.5) / width))
                .translated_by(to_world(c, top + 0.5 + rise / 2.0));
            self.push(spire, SLATE_ROOF.mix(&self.theme.slate, rng.range(0.0, 0.3)));
            let point = top + 0.5 + rise;
            self.stone(Vec3::new(0.08, 1.4, 0.08), IRON, c, point + 0.5);
            self.add(Shape::Ball, GOLD.darker(0.35), Transform::from_translation(to_world(c, point + 0.15 - (self.ground)(c))).with_scale(Vec3::splat(0.14)));
            let flag = c + Vec2::X * 0.45;
            self.stone(Vec3::new(0.85, 0.42, 0.03), BANNER, flag, point + 0.95);
            if keep {
                // A turret at each corner, standing up out of the parapet with a spire of its own.
                for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                    let at = c + Vec2::new(sx * (outer.x / 2.0 - 0.6), sy * (outer.y / 2.0 - 0.6));
                    let turret = faceted(cylinder(1.0, 3.6, 8)).translated_by(to_world(at, top + 1.3));
                    self.push(turret, stone.lighter(0.03));
                    let ring = faceted(cylinder(1.15, 0.35, 8)).translated_by(to_world(at, top + 3.1));
                    self.push(ring, stone.darker(0.06));
                    let cap = faceted(cone(1.2, 3.2, 8)).translated_by(to_world(at, top + 3.25 + 1.6));
                    self.push(cap, SLATE_ROOF);
                }
            }
        } else {
            // A banner pole standing up from the middle of its roof, the banner flying out.
            self.stone(Vec3::new(0.1, 4.2, 0.1), self.theme.bark, c, top + 2.1);
            self.stone(Vec3::new(1.5, 0.95, 0.04), BANNER, c + Vec2::X * 0.8, top + 3.6);
            self.stone(Vec3::new(1.5, 0.1, 0.05), GOLD.darker(0.4), c + Vec2::X * 0.8, top + 3.1);
        }
        // Banners down the face toward the field (south), from under the corbels.
        let banners = if keep { 3 } else if tower.roof { 0 } else { 1 };
        for i in 0..banners {
            let off = ((i as f32 + 0.5) / banners as f32 - 0.5) * size.x * 0.7;
            self.banner(c + Vec2::new(off, -size.y / 2.0 - 0.05), shaft_top - 0.6, if keep { 5.0 } else { 4.2 });
        }
    }

    /// A banner hung flat down a south face at `at` from `top` (absolute), `long` meters: deep
    /// red with a gold border and a dark chevron, on a pole across its top.
    fn banner(&mut self, at: Vec2, top: f32, long: f32) {
        let w = 1.3;
        self.stone(Vec3::new(w + 0.4, 0.1, 0.1), self.theme.bark, at - Vec2::Y * 0.06, top + 0.05);
        self.stone(Vec3::new(w, long, 0.04), BANNER, at - Vec2::Y * 0.04, top - long / 2.0);
        for side in [-1.0, 1.0] {
            self.stone(Vec3::new(0.08, long, 0.05), GOLD.darker(0.45), at + Vec2::new(side * (w / 2.0 - 0.04), -0.05), top - long / 2.0);
        }
        // The chevron: two bars leaning together, and the swallowtail's notch at the foot.
        for side in [-1.0, 1.0] {
            let t = Transform::from_translation(to_world(at + Vec2::new(side * 0.26, -0.07), top - long * 0.42 - (self.ground)(at)))
                .with_rotation(Quat::from_rotation_z(side * 0.75));
            self.block(Vec3::new(0.16, 0.8, 0.03), GOLD.darker(0.35), t);
        }
        let notch = faceted(Cuboid::new(0.5, 0.5, 0.06).mesh().build())
            .rotated_by(Quat::from_rotation_z(std::f32::consts::FRAC_PI_4))
            .translated_by(to_world(at - Vec2::Y * 0.03, top - long));
        self.push(notch, self.theme.stone.darker(0.25));
    }

    /// Over the gate (the gap between the two battlemented towers facing each other across it):
    /// an arch of stone joining them up high, crenellated, with the portcullis drawn up under
    /// it, and the gates swung open against the passage's sides.
    fn gatehouse(&mut self, m: &Map) {
        let gate: Vec<&Tower> = m.towers.iter().filter(|t| !t.roof).collect();
        let [a, b] = gate[..] else { return };
        let (west, east) = if a.center().x < b.center().x { (a, b) } else { (b, a) };
        let (x0, x1) = (west.hi.x as f32, east.lo.x as f32);
        let (y0, y1) = (west.lo.y.max(east.lo.y) as f32 + 0.6, west.hi.y.min(east.hi.y) as f32 - 0.6);
        let mid = Vec2::new((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        let width = x1 - x0;
        let stone = self.theme.stone.mix(&self.theme.wall, 0.25);
        let (under, over) = GATE_ARCH;
        self.stone(Vec3::new(width + 0.2, over - under, y1 - y0), stone, mid, (under + over) / 2.0);
        self.stone(Vec3::new(width + 0.3, 0.25, y1 - y0 + 0.3), stone.darker(0.1), mid, under - 0.12);
        for y in [y0 + 0.15, y1 - 0.15] {
            let n = (width / 1.1).round() as i32;
            for i in 0..n {
                let x = x0 + width * (i as f32 + 0.5) / n as f32;
                self.stone(Vec3::new(0.55, 0.7, 0.3), stone.darker(0.04), Vec2::new(x, y), over + 0.35);
            }
        }
        // The portcullis, drawn up: its spiked foot showing under the arch's south side.
        let face = y0 + 0.5;
        let bars = (width / 0.42).round() as i32;
        for i in 0..bars {
            let x = x0 + width * (i as f32 + 0.5) / bars as f32;
            self.stone(Vec3::new(0.07, 0.75, 0.07), IRON.darker(0.2), Vec2::new(x, face), under - 0.3);
        }
        self.stone(Vec3::new(width, 0.09, 0.09), IRON.darker(0.25), Vec2::new(mid.x, face), under - 0.45);
    }
}
