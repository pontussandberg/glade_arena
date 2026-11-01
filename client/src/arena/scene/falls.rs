//! The waterfall off the end of the river (`Map::falls`): the river coming over the cliff's lip
//! in a sheet that arcs out and falls, streaks of white running down it, foam churning where it
//! lands; wet rocks at its lip and in the pool under it, and rock down the cliff's face.

use super::*;

use arena_shared::map::Falls;

/// Foam: white with a little of the water's cold in it.
pub(super) const FOAM: Color = Color::srgb(0.84, 0.89, 0.92);
/// How far out from the lip the falling water lands.
const OUT: f32 = 2.4;
/// How many streaks of white run down the falls at once.
const SPILLS: usize = 36;

/// How high the river runs over the lip of `falls` (its surface, up on the cliff).
fn lip_height(m: &Map, falls: &Falls) -> f32 {
    m.height_at(Vec2::new(falls.lip - 0.5, (falls.river.0 + falls.river.1) / 2.0)) + WATER_LEVEL
}

/// Where water `s` of the way down (0 at the lip, 1 where it lands) is: how far out east of the
/// lip, and how high: thrown out level off the lip, falling ever faster. Short of the lip
/// (`s` below 0), it's on the river up on the cliff, running to it.
fn arc(falls: &Falls, top: f32, out: f32, s: f32) -> (f32, f32) {
    (falls.lip + out * s, top + (WATER_LEVEL - top) * s * s.max(0.0))
}

/// How far up the river (as a share of the fall) a streak of white starts.
const RUN_IN: f32 = 0.7;

/// The falling water: strips across the river, each arcing out a little more or less than the
/// next, whiter toward the foot; both sides of it, as it's seen from the cliff top behind it too.
pub(super) fn falls_mesh(m: &Map, theme: &Theme, rng: &mut Lcg) -> Option<Mesh> {
    let falls = m.falls?;
    let top = lip_height(m, &falls);
    let mut b = FlatMesh::default();
    let (from, to) = (falls.river.0 + 0.15, falls.river.1 - 0.15);
    let strips = ((to - from) / 0.45).round() as i32;
    let segments = 14;
    for i in 0..strips {
        let (y0, y1) = (from + (to - from) * i as f32 / strips as f32, from + (to - from) * (i + 1) as f32 / strips as f32);
        let out = OUT * rng.range(0.85, 1.15);
        let white = rng.range(0.3, 0.7);
        let at = |y: f32, s: f32| {
            let (x, h) = arc(&falls, top, out, s);
            to_world(Vec2::new(x, y), h)
        };
        for k in 0..segments {
            let (s0, s1) = (k as f32 / segments as f32, (k + 1) as f32 / segments as f32);
            let color = theme.pond.mix(&FOAM, (white + 0.3 * s1).min(1.0));
            let corners = [at(y0, s0), at(y0, s1), at(y1, s1), at(y1, s0)];
            b.quad(corners, color);
            b.quad([corners[3], corners[2], corners[1], corners[0]], color.darker(0.1));
        }
    }
    Some(b.build())
}

/// A streak of white running down the falls, over and over: across it at `y`, how far out its
/// strip arcs, and where in its run it is at the start.
#[derive(Component)]
pub(super) struct Spill {
    falls: Falls,
    top: f32,
    y: f32,
    out: f32,
    phase: f32,
}

/// The streaks of white down the falls (`Spill`).
pub(super) fn spawn_spills(commands: &mut Commands, meshes: &mut Assets<Mesh>, materials: &mut Assets<PlainMaterial>, m: &Map, rng: &mut Lcg) {
    let Some(falls) = m.falls else { return };
    let top = lip_height(m, &falls);
    let mesh = meshes.add(Cuboid::new(0.06, 0.07, 0.3));
    let material = materials.add(matte(FOAM));
    for _ in 0..SPILLS {
        let spill = Spill { falls, top, y: rng.range(falls.river.0 + 0.3, falls.river.1 - 0.3), out: OUT * rng.range(0.95, 1.2), phase: rng.next() };
        commands.spawn((ArenaScene, spill, Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), Transform::default(), NotShadowCaster));
    }
}

/// Each streak runs along the river to the lip and down the falls, stretching as it speeds up,
/// and starts again up the river.
pub(super) fn pour_falls(time: Res<Time>, mut spills: Query<(&Spill, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (spill, mut transform) in &mut spills {
        let s = (t * 0.45 + spill.phase).fract() * (1.0 + RUN_IN) - RUN_IN;
        let (x, h) = arc(&spill.falls, spill.top, spill.out, s);
        let (x2, h2) = arc(&spill.falls, spill.top, spill.out, (s + 0.02).min(1.0));
        let at = to_world(Vec2::new(x + 0.04, spill.y), h);
        let ahead = (to_world(Vec2::new(x2 + 0.04, spill.y), h2) - at).normalize_or(Vec3::NEG_Y);
        transform.translation = at;
        transform.rotation = Quat::from_rotation_arc(Vec3::Z, ahead);
        transform.scale = Vec3::new(1.0, 1.0, 1.0 + 6.0 * s.max(0.0));
    }
}

impl Props {
    /// The rock about the falls: down the cliff's face in uneven bands, big wet stones at the
    /// lip either side of the river, foam churning where the water lands, and stones standing
    /// up out of the pool under it.
    pub(super) fn falls(&mut self, m: &Map, rng: &mut Lcg) {
        let Some(falls) = m.falls else { return };
        let top = m.height_at(Vec2::new(falls.lip - 0.5, falls.river.0 - 3.0));
        let river = |y: f32| y > falls.river.0 - 1.0 && y < falls.river.1 + 1.0;
        let rock = |rng: &mut Lcg, k: f32| self.theme.slate.mix(&self.theme.wall, 0.2 + 0.5 * rng.next()).darker(0.05 + 0.1 * k);
        // The face, from its foot up, strewn evenly (not in columns): north of the barbican's
        // crag it steps back west and stands over the crag's top.
        let mut y = -84.0;
        while y < 84.0 {
            let at = y + rng.range(-0.7, 0.7);
            let (x, foot) = if at < 16.0 { (falls.lip - 1.0, 0.0) } else { (falls.lip - 3.0, if at < 64.0 { 6.5 } else { 0.0 }) };
            if !river(at) {
                for _ in 0..2 {
                    let h = rng.range(foot + 0.8, top - 0.6);
                    let p = Vec2::new(x + rng.range(-0.4, 0.4), at);
                    let s = rng.range(1.4, 2.4);
                    let color = rock(rng, 1.0 - (h - foot) / (top - foot));
                    let t = Transform::from_translation(to_world(p, h - (self.ground)(p)))
                        .with_rotation(Quat::from_euler(EulerRot::XYZ, rng.range(0.0, 3.0), rng.range(0.0, 3.0), 0.0))
                        .with_scale(Vec3::new(s * 0.6, s, s));
                    self.add(Shape::Rock, color, t);
                }
            }
            y += 1.4;
        }
        // At the lip, either side of the river, and one splitting it.
        for (y, s) in [(falls.river.0 - 0.6, 1.9), (falls.river.0 - 1.8, 1.4), (falls.river.1 + 0.6, 2.1), (falls.river.1 + 1.9, 1.3), ((falls.river.0 + falls.river.1) / 2.0 + 0.8, 0.9)] {
            let p = Vec2::new(falls.lip - 0.4, y);
            let t = Transform::from_translation(to_world(p, top - 0.5 - (self.ground)(p))).with_scale(Vec3::splat(s));
            self.add(Shape::Rock, self.theme.slate.darker(0.3), t);
        }
        // Foam where it lands, thinning out downstream; stones out of the pool.
        for k in 0..46 {
            let u = rng.next();
            let p = Vec2::new(falls.lip + 0.4 + u * u * 6.0, rng.range(falls.river.0 + 0.2, falls.river.1 - 0.2));
            let s = rng.range(0.5, 1.2) * (1.0 - 0.5 * u);
            let t = Transform::from_translation(to_world(p, WATER_LEVEL + 0.02 - (self.ground)(p))).with_scale(Vec3::new(s, 0.16, s * rng.range(0.7, 1.3)));
            self.add(Shape::Ball, FOAM.mix(&self.theme.pond, 0.15 * (k % 3) as f32), t);
        }
        // The stones stay in the fall's own foam, short of where anyone can wade: none stands
        // where it would be walked through.
        for _ in 0..6 {
            let p = Vec2::new(falls.lip + rng.range(0.4, 1.3), rng.range(falls.river.0 + 0.5, falls.river.1 - 0.5));
            let s = rng.range(0.6, 1.0);
            self.add(Shape::Rock, self.theme.slate.darker(0.35), Transform::from_translation(to_world(p, WATER_LEVEL - 0.25 - (self.ground)(p))).with_scale(Vec3::splat(s)));
        }
    }
}
