//! What stands in the way on a map, as boxes: every wall, rock, crate, house and roof, fence,
//! parapet, post and chimney is one or a few (`Collider`), worked out once from the map
//! (`build`) and listed by the tiles they're near, so a move only looks at what's around it.
//! A box stops a body from the side up to its top, holds it up from above, and lets it under if
//! the body fits beneath (`Map::colliders_block`, `Map::floor_at`, `Map::ceiling_at`); it stops a
//! shot that flies into it, and one over its top clears it (`Map::shot_meets`). The scene
//! draws each from the same numbers (`Map::ruin_at`, `boulder`, `Map::parapet`,
//! `Map::torch_post`, the `House` and well constants), so what's drawn is what's hit.

use bevy::math::{IVec2, Vec2, Vec3};

use super::{CRATE, FENCE_TOP, House, Map, NAV_RADIUS, PARAPET_DROP, PARAPET_HEIGHT, PARAPET_IN, PARAPET_THICK, SIDES, STEP_UP, STEPS, TORCH_POST, TORCH_STAKE, Tile};
use crate::config::BODY_HEIGHT;

/// A box standing in the way (on the gameplay plane, `lo` to `hi`; heights in meters), or a
/// roof: its top sloping down either side of a ridge (`Roof`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Collider {
    pub lo: Vec2,
    pub hi: Vec2,
    pub bottom: f32,
    /// Its top; a roof's along its ridge.
    pub top: f32,
    pub roof: Option<Roof>,
}

/// A roof's slope: its ridge runs along x (else along y) at `ridge` across it, and its top falls
/// `slope` meters for every meter out from the ridge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Roof {
    pub along_x: bool,
    pub ridge: f32,
    pub slope: f32,
}

impl Collider {
    /// A plain box, from `lo` to `hi` (each the lesser and greater corner).
    pub(super) fn block(lo: Vec2, hi: Vec2, bottom: f32, top: f32) -> Collider {
        Collider { lo, hi, bottom, top, roof: None }
    }

    /// How high its top is over `p`.
    pub fn top_at(&self, p: Vec2) -> f32 {
        match self.roof {
            None => self.top,
            Some(roof) => self.top - roof.slope * if roof.along_x { (p.y - roof.ridge).abs() } else { (p.x - roof.ridge).abs() },
        }
    }

    /// How far `p` is from it, on the plane (nothing inside it).
    fn reach(&self, p: Vec2) -> f32 {
        p.distance(p.clamp(self.lo, self.hi))
    }

    /// Whether it stands in the way of a body with its feet `feet` high near `p`: it isn't under
    /// the body (its top no more than `STEP_UP` over the feet) nor over its head (its bottom no
    /// lower than `BODY_HEIGHT` over them).
    fn in_the_way(&self, p: Vec2, feet: f32) -> bool {
        self.bottom < feet + BODY_HEIGHT && self.top_at(p.clamp(self.lo, self.hi)) > feet + STEP_UP
    }

    /// Whether it keeps a body with its feet `feet` high from moving straight from `a` to `b`:
    /// it's in the way there (`in_the_way`), the move comes within the body's reach of it, and
    /// the body isn't already reaching into it and getting no closer (it came down beside it:
    /// it may always move off).
    pub(super) fn blocks(&self, a: Vec2, b: Vec2, feet: f32) -> bool {
        let grown = Vec2::splat(NAV_RADIUS);
        if !self.in_the_way(b, feet) {
            return false;
        }
        if !super::segment_hits_box(a, b, self.lo - grown, self.hi + grown) {
            return false;
        }
        !(self.reach(a) < NAV_RADIUS && self.reach(b) >= self.reach(a))
    }

    /// Whether something going straight from `a`, `from` high, to `b`, `to` high, meets it on
    /// the way: is ever within it (between its bottom and its top, a roof's sloping one).
    fn cut_by(&self, (a, from): (Vec2, f32), (b, to): (Vec2, f32)) -> bool {
        // How much of the way (0 at `a`, 1 at `b`) is within it across the plane and between
        // its bottom and its highest top, each way at once.
        let mut span = (0.0f32, 1.0f32);
        for (start, end, low, high) in [(a.x, b.x, self.lo.x, self.hi.x), (a.y, b.y, self.lo.y, self.hi.y), (from, to, self.bottom, self.top)] {
            let d = end - start;
            if d.abs() < 1e-9 {
                if start < low || start > high {
                    return false;
                }
                continue;
            }
            let (t0, t1) = ((low - start) / d, (high - start) / d);
            span = (span.0.max(t0.min(t1)), span.1.min(t0.max(t1)));
            if span.0 > span.1 {
                return false;
            }
        }
        let Some(roof) = self.roof else { return true };
        // Under a roof's slope somewhere in there: the roof falls away from its ridge either way,
        // so if anywhere, at either end of that stretch or where it crosses the ridge.
        let (across_a, across_b) = if roof.along_x { (a.y, b.y) } else { (a.x, b.x) };
        let ridge = if (across_b - across_a).abs() < 1e-9 { span.0 } else { ((roof.ridge - across_a) / (across_b - across_a)).clamp(span.0, span.1) };
        [span.0, span.1, ridge].into_iter().any(|t| from + (to - from) * t < self.top_at(a.lerp(b, t).clamp(self.lo, self.hi)))
    }
}

/// How far past its edge a box still holds a body up: a body stands on a fence's rail or a
/// parapet's top while its middle is over it or a little beyond.
const SUPPORT: f32 = 0.15;

/// A number in [0, 1) for tile `t`, the same everywhere (client and server, every run): a
/// different one for each `salt`.
fn jitter(t: IVec2, salt: u32) -> f32 {
    let mut x = (t.x as u32).wrapping_mul(0x9E37_79B1) ^ (t.y as u32).wrapping_mul(0x85EB_CA77) ^ salt.wrapping_mul(0xC2B2_AE3D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    x = x.wrapping_mul(0x297A_2D39);
    x ^= x >> 15;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

/// A ruined wall on a tile: how high its blocks stand, and the smaller block on top of some
/// (its size, and how far off the tile's middle).
#[derive(Clone, Copy, Debug)]
pub struct Ruin {
    pub height: f32,
    pub cap: Option<(f32, Vec2)>,
}

impl Ruin {
    /// Its top, the cap's if it has one.
    pub fn top(&self) -> f32 {
        self.height + self.cap.map_or(0.0, |(size, _)| size)
    }
}

/// The ruined wall on tile `t`: head height and over, a ruin you can't see across.
pub fn ruin(t: IVec2) -> Ruin {
    let height = 1.9 + 0.9 * jitter(t, 1);
    let cap = (jitter(t, 2) < 0.45).then(|| (0.5 + 0.3 * jitter(t, 3), Vec2::new(jitter(t, 4) - 0.5, jitter(t, 5) - 0.5) * 0.4));
    Ruin { height, cap }
}

/// How big a boulder's rock is drawn (a scale on the rock mesh, `ROCK_RADIUS`), and which way
/// it's turned (radians about the up axis): taller than a fighter, its bulk going up, not out
/// over the tiles beside it.
pub fn boulder(t: IVec2) -> (Vec3, f32) {
    let size = Vec3::new(1.1 + 0.2 * jitter(t, 6), 1.8 + 0.5 * jitter(t, 7), 1.1 + 0.2 * jitter(t, 8));
    (size, jitter(t, 9) * 3.0)
}

/// The rock mesh's radius (meters, before its scale), and how high its middle is set over the
/// ground for its height (its scale's height).
pub const ROCK_RADIUS: f32 = 0.6;
pub const ROCK_LIFT: f32 = 0.3;

/// The village well's stone ring (its reach from the middle, its height), and its little roof
/// (half its size each way, how high its underside and top are).
pub const WELL_RING: f32 = 0.77;
pub const WELL_HEIGHT: f32 = 0.9;
pub const WELL_ROOF: (Vec2, f32, f32) = (Vec2::new(1.0, 0.75), 2.2, 2.7);

/// How high the merchant's counter's top is.
pub const COUNTER_TOP: f32 = 1.08;

/// Every collider on `map`.
pub(super) fn build(map: &Map) -> Vec<Collider> {
    let mut out = Vec::new();
    let ground = |p: Vec2| map.height_at(p);
    let tile_box = |t: IVec2, bottom: f32, top: f32| {
        let c = Map::center(t);
        Collider::block(c - Vec2::splat(0.5), c + Vec2::splat(0.5), bottom, top)
    };
    for house in &map.houses {
        let g = ground(house.center());
        let (lo, hi) = (house.lo.as_vec2(), house.hi.as_vec2());
        out.push(Collider::block(lo, hi, g, g + House::WALLS));
        let reach = if house.long_x() { Vec2::new(House::GABLE_EAVES, House::EAVES) } else { Vec2::new(House::EAVES, House::GABLE_EAVES) };
        let ridge = if house.long_x() { house.center().y } else { house.center().x };
        out.push(Collider {
            lo: lo - reach,
            hi: hi + reach,
            bottom: g + House::WALLS - House::EAVES * House::SLOPE - 0.05,
            top: g + house.ridge(),
            roof: Some(Roof { along_x: house.long_x(), ridge, slope: House::SLOPE }),
        });
        let chimney = house.chimney();
        let half = Vec2::splat(House::CHIMNEY / 2.0);
        out.push(Collider::block(chimney - half, chimney + half, g + House::WALLS, g + House::WALLS + house.rise() + House::CHIMNEY_TOP));
    }
    for &(t, high) in &map.crates {
        let g = ground(Map::center(t));
        out.push(tile_box(t, g, g + CRATE * high as f32));
    }
    for &t in &map.counter {
        let g = ground(Map::center(t));
        out.push(tile_box(t, g, g + COUNTER_TOP));
    }
    if let Some(well) = map.well {
        let g = ground(well);
        out.push(Collider::block(well - Vec2::splat(WELL_RING), well + Vec2::splat(WELL_RING), g, g + WELL_HEIGHT));
        let (half, under, top) = WELL_ROOF;
        out.push(Collider::block(well - half, well + half, g + under, g + top));
    }
    for (t, tile) in map.tiles() {
        let c = Map::center(t);
        let g = ground(c);
        if let Some(ruin) = map.ruin_at(t) {
            out.push(tile_box(t, g, g + ruin.height));
            if let Some((size, off)) = ruin.cap {
                out.push(Collider::block(c + off - Vec2::splat(size / 2.0), c + off + Vec2::splat(size / 2.0), g + ruin.height, g + ruin.height + size));
            }
        }
        match tile {
            // The well's stones are its own.
            Tile::Rock if map.well.is_some_and(|well| Map::tile_of(well) == t) => {}
            Tile::Rock => {
                let (size, _) = boulder(t);
                let half = Vec2::new(size.x, size.z) * ROCK_RADIUS * 0.85;
                out.push(Collider::block(c - half, c + half, g, g + (ROCK_LIFT + ROCK_RADIUS * 0.85) * size.y));
            }
            _ => {}
        }
    }
    for &(a, b) in &map.fences {
        let top = ground(a).max(ground(b)) + FENCE_TOP;
        let run = b - a;
        // Along x or y, one box; slanting, a box for each short length of it.
        let pieces = if run.x == 0.0 || run.y == 0.0 { 1 } else { (run.length() / 0.4).ceil() as usize };
        for i in 0..pieces {
            let (p, q) = (a + run * (i as f32 / pieces as f32), a + run * ((i + 1) as f32 / pieces as f32));
            out.push(Collider::block(p.min(q) - Vec2::splat(0.08), p.max(q) + Vec2::splat(0.08), ground(p).min(ground(q)), top));
        }
    }
    for (t, _) in map.tiles() {
        out.extend(SIDES.into_iter().filter_map(|d| map.parapet(t, d)));
    }
    for &p in &map.torches {
        let (base, post) = map.torch_post(p);
        let g = ground(p) + base;
        out.push(Collider::block(p - Vec2::splat(0.07), p + Vec2::splat(0.07), g, g + post));
    }
    out
}

impl Map {
    /// The ruined wall on tile `t`, if there is one (`ruin`): a wall tile that isn't a house's,
    /// a stack of crates or the merchant's counter.
    pub fn ruin_at(&self, t: IVec2) -> Option<Ruin> {
        let taken = self.houses.iter().any(|house| house.covers(t)) || self.crates.iter().any(|(at, _)| *at == t) || self.counter.contains(&t);
        (self.get(t) == Tile::Wall && !taken).then(|| ruin(t))
    }

    /// How high over the ground at `p` a torch there stands its post, and how tall the post is:
    /// a short stake on top of a ruin, a brazier's post taller than anyone on the ground.
    pub fn torch_post(&self, p: Vec2) -> (f32, f32) {
        self.ruin_at(Map::tile_of(p)).map_or((0.0, TORCH_POST), |ruin| (ruin.top(), TORCH_STAKE))
    }

    /// The parapet along side `d` (`SIDES`) of tile `t`, if the ground drops `PARAPET_DROP` or more
    /// past it (but not into the forest): the side's length, on the high tile's edge.
    pub fn parapet(&self, t: IVec2, d: IVec2) -> Option<Collider> {
        let c = Map::center(t);
        let here = self.height_at(c);
        if self.get(t) == Tile::Forest || here - self.height_at(Map::center(t + d)) < PARAPET_DROP || self.scene_tile(t + d) == Tile::Forest {
            return None;
        }
        let mid = c + d.as_vec2() * (0.5 - PARAPET_IN);
        let half = d.as_vec2().abs() * (PARAPET_THICK / 2.0) + d.as_vec2().perp().abs() * 0.5;
        Some(Collider::block(mid - half, mid + half, here, here + PARAPET_HEIGHT))
    }

    /// The colliders near tile `t`: those reaching within a body's reach of it (`index_colliders`).
    pub(super) fn colliders_on(&self, t: IVec2) -> impl Iterator<Item = &Collider> + '_ {
        self.index(t).and_then(|i| self.near.get(i)).into_iter().flatten().map(|&i| &self.colliders[i as usize])
    }

    /// Lists each collider on the tiles it reaches onto, grown by a body's reach, and finds where
    /// on each walkable tile a body stands (`stand_on`).
    pub(super) fn index_colliders(&mut self) {
        let mut near = vec![Vec::new(); self.tiles.len()];
        let grown = Vec2::splat(NAV_RADIUS + 0.01);
        for (i, collider) in self.colliders.iter().enumerate() {
            let (a, b) = (Map::tile_of(collider.lo - grown), Map::tile_of(collider.hi + grown));
            for y in a.y..=b.y {
                for x in a.x..=b.x {
                    let Some(k) = self.index(IVec2::new(x, y)) else { continue };
                    near[k].push(i as u16);
                }
            }
        }
        self.near = near;
        self.stands = (0..self.tiles.len()).map(|i| self.stand_on(self.tile_at(i))).collect();
    }

    /// Where on tile `t` a body stands, if anywhere (`Map::stand`): its middle, or if something
    /// on the ground there is in the way (a post, a parapet, a fence's rail) the spot nearest it
    /// that's clear of that, and of the tiles round it no one walks on. None on a tile that
    /// isn't open ground, or has no room left on it.
    fn stand_on(&self, t: IVec2) -> Option<Vec2> {
        if !self.get(t).walkable() {
            return None;
        }
        // Clear of a box as a walk's line is (`line_walkable`): out of it grown square by a
        // body's reach.
        let apart = |p: Vec2, lo: Vec2, hi: Vec2| (p.clamp(lo, hi) - p).abs().max_element() >= NAV_RADIUS;
        let fits = |p: Vec2| {
            self.colliders_on(t).all(|collider| apart(p, collider.lo, collider.hi) || !collider.in_the_way(p, self.height_at(p)))
                && STEPS.iter().map(|&d| t + d).all(|n| self.get(n).walkable() || apart(p, Map::center(n) - Vec2::splat(0.5), Map::center(n) + Vec2::splat(0.5)))
        };
        let c = Map::center(t);
        if fits(c) {
            return Some(c);
        }
        // The nearest of a grid over the tile finer than anything that stands in the way.
        let mut spots: Vec<Vec2> = (-9..=9).flat_map(|y| (-9..=9).map(move |x| c + Vec2::new(x as f32, y as f32) * 0.05)).collect();
        spots.sort_by(|a, b| a.distance_squared(c).total_cmp(&b.distance_squared(c)));
        spots.into_iter().find(|&p| fits(p))
    }

    /// Whether a collider keeps a body with its feet `feet` high from moving straight from `a`
    /// to `b` (`Collider::blocks`): any one near a tile along the way.
    pub fn colliders_block(&self, a: Vec2, b: Vec2, feet: f32) -> bool {
        self.line_blocker_by(a, b, |t| self.colliders_on(t).any(|collider| collider.blocks(a, b, feet))).is_some()
    }

    /// What a body with its feet `feet` high stands on at `p`: what's there up to `STEP_UP`
    /// over its feet (`surface_at`).
    pub fn floor_at(&self, p: Vec2, feet: f32) -> f32 {
        self.surface_at(p, feet + STEP_UP)
    }

    /// The highest of what's at `p` no higher than `up_to`: the ground, or the top of a collider
    /// (`p` over it or `SUPPORT` beyond).
    pub fn surface_at(&self, p: Vec2, up_to: f32) -> f32 {
        self.surface_within(p, up_to, SUPPORT)
    }

    /// `surface_at` just under `p`, nothing past the edge of what it's over: where something
    /// drawn on the surfaces lies (a mark, a scuff), so none of it hangs out in the air.
    pub fn surface_under(&self, p: Vec2, up_to: f32) -> f32 {
        self.surface_within(p, up_to, 0.0)
    }

    /// The highest of what's at `p` no higher than `up_to`: the ground, or the top of a collider
    /// `p` is over or within `beyond` of.
    fn surface_within(&self, p: Vec2, up_to: f32, beyond: f32) -> f32 {
        // Every collider within `SUPPORT` of `p` is near its tile: nearer than a body's reach.
        self.colliders_on(Map::tile_of(p))
            .filter(|collider| collider.reach(p) <= beyond)
            .map(|collider| collider.top_at(p.clamp(collider.lo, collider.hi)))
            .filter(|&top| top <= up_to)
            .fold(self.height_at(p), f32::max)
    }

    /// How high a body with its feet `feet` high at `p` may rise before its head meets the
    /// underside of something over it (`f32::INFINITY`: nothing is).
    pub fn ceiling_at(&self, p: Vec2, feet: f32) -> f32 {
        self.colliders_on(Map::tile_of(p))
            .filter(|collider| collider.reach(p) < NAV_RADIUS && collider.bottom >= feet + BODY_HEIGHT - 1e-3)
            .map(|collider| collider.bottom - BODY_HEIGHT)
            .fold(f32::INFINITY, f32::min)
    }

    /// Whether something flying straight from `a`, `from` high, to `b`, `to` high (a shot's
    /// middle, the line it would fly along) meets what stops shots on the way: a collider (a
    /// wall, a roof, a rock, a crate, a fence: `Collider::cut_by`), a tree or the forest (none of
    /// their own: `Tile::blocks_shots_at`, over the ground there), or the ground (a hillside, a
    /// cliff). One over the top of each clears it.
    pub fn shot_meets(&self, a: (Vec2, f32), b: (Vec2, f32)) -> bool {
        self.line_meets(a, b, 0.25, true)
    }

    /// Whether the line from an eye at `a`, `from` high, to `b`, `to` high is hidden by something
    /// solid on the way: a collider (a wall, a roof, a rock: `Collider::cut_by`) or the ground (a
    /// hillside). Not by a tree or the forest, seen between. For what's drawn over the scene, as
    /// a health bar is.
    pub fn sight_meets(&self, a: (Vec2, f32), b: (Vec2, f32)) -> bool {
        self.line_meets(a, b, 1.0, false)
    }

    /// `shot_meets` and `sight_meets`: a collider on the way, or the ground (looked at every
    /// `step` meters), or (`trees`) a tree or the forest there.
    fn line_meets(&self, (a, from): (Vec2, f32), (b, to): (Vec2, f32), step: f32, trees: bool) -> bool {
        if self.line_blocker_by(a, b, |t| self.colliders_on(t).any(|collider| collider.cut_by((a, from), (b, to)))).is_some() {
            return true;
        }
        let steps = (a.distance(b) / step).ceil().max(1.0) as i32;
        (0..=steps).any(|s| {
            let k = s as f32 / steps as f32;
            let (p, height) = (a.lerp(b, k), from + (to - from) * k);
            let tile = self.get(Map::tile_of(p));
            let over = height - self.height_at(p);
            over < 0.0 || (trees && matches!(tile, Tile::Tree | Tile::Forest) && tile.blocks_shots_at(over))
        })
    }

    /// Every collider (for showing them: the "3D hitboxes" switch).
    pub fn colliders(&self) -> &[Collider] {
        &self.colliders
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::MapId;

    fn maps() -> impl Iterator<Item = &'static Map> {
        MapId::ALL.into_iter().map(MapId::map)
    }

    /// Sight (`sight_meets`) is hidden by a house between, and by the ground, but not by a tree
    /// it's seen past, though that stops a shot.
    #[test]
    fn a_house_hides_what_is_behind_it_and_a_tree_does_not() {
        let yard = MapId::Yard.map();
        let at = |p: Vec2, up: f32| (p, yard.height_at(p) + up);
        for house in &yard.houses {
            let (c, across) = (house.center(), (house.hi - house.lo).x as f32 / 2.0 + 1.5);
            let (a, b) = (at(c - Vec2::X * across, 1.5), at(c + Vec2::X * across, 1.5));
            assert!(yard.sight_meets(a, b) && yard.sight_meets(b, a), "seen through the house at {c}");
            // From high over it, looking down past its eaves, what's beside it shows.
            assert!(!yard.sight_meets(at(c, 30.0), b), "the house hides what's beside it from above");
        }
        let lone_tree = yard.tiles().map(|(t, _)| t).find(|&t| yard.get(t) == Tile::Tree && [t - IVec2::X, t + IVec2::X].iter().all(|&n| yard.walkable(n))).expect("a tree in the open");
        let (a, b) = (at(Map::center(lone_tree - IVec2::X), 1.5), at(Map::center(lone_tree + IVec2::X), 1.5));
        assert!(yard.shot_meets(a, b) && !yard.sight_meets(a, b), "a tree hides what's behind it, or doesn't stop a shot");
        let (low, beyond) = (at(Map::center(lone_tree - IVec2::X), -0.5), at(Map::center(lone_tree + IVec2::X), -0.5));
        assert!(yard.sight_meets(low, beyond), "seen through the ground");
    }

    /// The same numbers every run, each in -`range`..`range`.
    fn numbers(seed: u64) -> impl FnMut(f32) -> f32 {
        let mut seed = seed;
        move |range: f32| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((seed >> 40) as f32 / (1u64 << 24) as f32 - 0.5) * 2.0 * range
        }
    }

    #[test]
    fn every_collider_is_a_box_the_right_way_up() {
        for m in maps() {
            for c in m.colliders() {
                assert!(c.lo.x < c.hi.x && c.lo.y < c.hi.y, "{:?}: {c:?} is inside out", m.id);
                assert!(c.bottom < c.top, "{:?}: {c:?} is upside down", m.id);
                // A roof's top stays over its underside out to its eaves.
                for p in [c.lo, c.hi, Vec2::new(c.lo.x, c.hi.y), Vec2::new(c.hi.x, c.lo.y)] {
                    assert!(c.top_at(p) > c.bottom, "{:?}: {c:?} is thinner than nothing at {p}", m.id);
                }
            }
        }
    }

    /// What the tile index finds (`colliders_block`, `surface_at`, `ceiling_at`) is just what looking
    /// through every collider there is finds, for steps a tick long and long lines alike, from
    /// the ground and up in the air.
    #[test]
    fn colliders_found_by_tile_are_all_the_colliders_there_are() {
        for m in maps() {
            let mut rand = numbers(777);
            let extent = m.half_extents() - Vec2::splat(2.0);
            for i in 0..4000 {
                let a = Vec2::new(rand(extent.x), rand(extent.y));
                let reach = if i % 2 == 0 { 0.4 } else { 10.0 };
                let b = a + Vec2::new(rand(reach), rand(reach));
                let feet = m.height_at(a) + rand(2.5).abs() + if i % 3 == 0 { 0.0 } else { rand(0.5) };
                let brute = m.colliders().iter().any(|c| c.blocks(a, b, feet));
                assert_eq!(m.colliders_block(a, b, feet), brute, "{:?}: {a} -> {b}, feet {feet}", m.id);
                let surface = m.colliders().iter().filter(|c| c.reach(a) <= SUPPORT).map(|c| c.top_at(a.clamp(c.lo, c.hi))).filter(|&top| top <= feet).fold(m.height_at(a), f32::max);
                assert_eq!(m.surface_at(a, feet), surface, "{:?}: the surface at {a} up to {feet}", m.id);
                let under = m.colliders().iter().filter(|c| c.reach(a) == 0.0).map(|c| c.top_at(a)).filter(|&top| top <= feet).fold(m.height_at(a), f32::max);
                assert_eq!(m.surface_under(a, feet), under, "{:?}: the surface just under {a} up to {feet}", m.id);
                let ceiling = m.colliders().iter().filter(|c| c.reach(a) < NAV_RADIUS && c.bottom >= feet + BODY_HEIGHT - 1e-3).map(|c| c.bottom - BODY_HEIGHT).fold(f32::INFINITY, f32::min);
                assert_eq!(m.ceiling_at(a, feet), ceiling, "{:?}: the ceiling at {a} over {feet}", m.id);
            }
        }
    }

    /// Where a body stands on a tile (`Map::stand`) is clear of everything that stands in the way
    /// on the ground there and of the tiles no one walks on, worked out from every collider and
    /// tile there is: its middle where that's clear, else no further from it than it has to be;
    /// and a parapet along a terrace's edge, or a post in a tile's middle, leaves open ground
    /// walkable.
    #[test]
    fn a_body_stands_on_a_tile_clear_of_what_stands_there_as_near_its_middle_as_it_can() {
        let apart = |p: Vec2, lo: Vec2, hi: Vec2| (p.clamp(lo, hi) - p).abs().max_element() >= NAV_RADIUS;
        let clear = |m: &Map, p: Vec2| {
            m.colliders().iter().all(|collider| apart(p, collider.lo, collider.hi) || !collider.in_the_way(p, m.height_at(p)))
                && m.tiles().all(|(n, tile)| tile.walkable() || apart(p, Map::center(n) - Vec2::splat(0.5), Map::center(n) + Vec2::splat(0.5)))
        };
        for m in maps() {
            for (t, tile) in m.tiles() {
                if !m.walkable(t) {
                    assert!(!tile.walkable() || !(-9..=9).any(|x| (-9..=9).any(|y| clear(m, Map::center(t) + Vec2::new(x as f32, y as f32) * 0.05))), "{:?}: no stand on {t}, yet room", m.id);
                    continue;
                }
                let (c, stand) = (Map::center(t), m.stand(t));
                assert!(clear(m, stand), "{:?}: the stand on {t} at {stand} is in something", m.id);
                assert!(Map::tile_of(stand) == t && (clear(m, c) == (stand == c)), "{:?}: the stand on {t} is {stand}", m.id);
            }
        }
        // The yard's terraces' edges and torch posts.
        let yard = crate::map::MapId::Yard.map();
        let along_parapets: Vec<IVec2> = yard.tiles().filter(|&(t, tile)| tile.walkable() && SIDES.iter().any(|&d| yard.parapet(t, d).is_some())).map(|(t, _)| t).collect();
        let off: Vec<IVec2> = along_parapets.iter().copied().filter(|&t| !yard.walkable(t)).collect();
        assert!(along_parapets.len() > 20 && off.is_empty(), "of the {} open tiles along parapets these aren't walkable: {off:?}", along_parapets.len());
        for &p in &yard.torches {
            let t = Map::tile_of(p);
            assert!(!yard.get(t).walkable() || (yard.walkable(t) && yard.stand(t) != Map::center(t)), "the torch at {p} leaves its tile {t} unwalkable or is walked into");
        }
    }

    /// The steps worked out once per map (`work_out_steps`) are the ones the ground and what
    /// stands on it allow, worked out afresh.
    #[test]
    fn the_steps_worked_out_once_are_the_ones_there_are() {
        for m in maps() {
            for (t, _) in m.tiles().filter(|&(t, _)| m.walkable(t)) {
                let fresh: Vec<_> = crate::map::STEPS
                    .iter()
                    .map(|&d| t + d)
                    .filter(|&next| {
                        let d = next - t;
                        let diagonal = d.x != 0 && d.y != 0;
                        m.walkable(next) && (!diagonal || (m.walkable(t + IVec2::new(d.x, 0)) && m.walkable(t + IVec2::new(0, d.y)))) && m.line_walkable(m.stand(t), m.stand(next))
                    })
                    .collect();
                let worked_out: Vec<_> = m.steps(t).map(|(next, _)| next).collect();
                assert_eq!(worked_out, fresh, "{:?}: the steps from {t}", m.id);
            }
        }
    }

    #[test]
    fn a_house_is_its_walls_under_a_roof_with_a_chimney_on_it() {
        for m in maps() {
            for house in &m.houses {
                let g = m.height_at(house.center());
                let (lo, hi) = (house.lo.as_vec2(), house.hi.as_vec2());
                assert!(m.colliders().iter().any(|c| c.lo == lo && c.hi == hi && c.bottom == g && c.top == g + House::WALLS && c.roof.is_none()), "{:?}: no walls for {house:?}", m.id);
                let roof = m.colliders().iter().find(|c| c.roof.is_some() && c.lo.cmple(lo).all() && c.hi.cmpge(hi).all()).expect("a roof");
                let ridge = roof.roof.unwrap();
                // Its ridge down its middle, along its long side, as high as the house says.
                assert_eq!(ridge.along_x, house.long_x());
                assert_eq!(roof.top, g + house.ridge());
                assert_eq!(roof.top_at(house.center()), g + house.ridge());
                // Down at the eaves: as far below the ridge as the slope falls from it.
                let half = house.size().x.min(house.size().y) / 2.0;
                let edge = if house.long_x() { Vec2::new(house.center().x, lo.y - House::EAVES) } else { Vec2::new(lo.x - House::EAVES, house.center().y) };
                assert!((roof.top_at(edge) - (roof.top - House::SLOPE * (half + House::EAVES))).abs() < 1e-4);
                // The eaves' underside is over the walls' tops' edge, under the roof's top there.
                assert!(roof.bottom < g + House::WALLS && roof.bottom < roof.top_at(edge));
                let chimney = house.chimney();
                assert!(m.colliders().iter().any(|c| (c.lo + c.hi) / 2.0 == chimney && c.top > roof.top), "{:?}: no chimney over the ridge of {house:?}", m.id);
                // No ruin stands in a house's walls.
                for y in house.lo.y..house.hi.y {
                    for x in house.lo.x..house.hi.x {
                        assert!(m.ruin_at(IVec2::new(x, y)).is_none(), "{:?}: a ruin in {house:?}", m.id);
                    }
                }
            }
        }
    }

    #[test]
    fn ruins_crates_and_torches_stand_as_high_as_they_are_drawn() {
        for m in maps() {
            for (t, tile) in m.tiles() {
                let c = Map::center(t);
                let g = m.height_at(c);
                if let Some(ruin) = m.ruin_at(t) {
                    assert_eq!(tile, Tile::Wall);
                    // Stood on at its top, its cap's if it has one (always over its middle).
                    let top = g + ruin.top();
                    assert!((m.surface_at(c, top + 0.01) - top).abs() < 1e-4, "{:?}: the ruin at {t} is stood on at {}, drawn to {top}", m.id, m.surface_at(c, top + 0.01));
                }
            }
            for &(t, high) in &m.crates {
                let c = Map::center(t);
                let top = m.height_at(c) + CRATE * high as f32;
                assert_eq!(m.surface_at(c, top + 0.01), top, "{:?}: the crates at {t}", m.id);
                assert!(m.ruin_at(t).is_none());
            }
            for &p in &m.torches {
                let (base, post) = m.torch_post(p);
                let top = m.height_at(p) + base + post;
                assert!(m.colliders().iter().any(|c| (c.lo + c.hi) / 2.0 == p && c.top == top), "{:?}: no post for the torch at {p}", m.id);
                assert_eq!(base > 0.0, m.ruin_at(Map::tile_of(p)).is_some(), "{:?}: the torch at {p} stands on a ruin, or doesn't", m.id);
            }
        }
    }

    #[test]
    fn a_parapet_runs_along_every_sheer_drop_and_nowhere_else() {
        for m in maps() {
            let mut parapets = 0;
            for (t, tile) in m.tiles() {
                for d in SIDES {
                    let drop = m.height_at(Map::center(t)) - m.height_at(Map::center(t + d));
                    let expected = tile != Tile::Forest && drop >= PARAPET_DROP && m.scene_tile(t + d) != Tile::Forest;
                    let parapet = m.parapet(t, d);
                    assert_eq!(parapet.is_some(), expected, "{:?}: the side {d} of {t}, a drop of {drop}", m.id);
                    if let Some(parapet) = parapet {
                        parapets += 1;
                        assert!(m.colliders().contains(&parapet), "{:?}: the parapet on {t}'s side {d} stops nothing", m.id);
                        // On the high tile, at its edge.
                        assert_eq!(Map::tile_of((parapet.lo + parapet.hi) / 2.0), t);
                        assert_eq!(parapet.top, m.height_at(Map::center(t)) + PARAPET_HEIGHT);
                    }
                }
            }
            if m.id == MapId::Yard {
                assert!(parapets > 0, "the yard's terraces have no parapets");
            }
        }
    }

    /// A ruin on some map with two tiles of open level ground off one of its sides: something
    /// solid to walk at, from that side (the map, the ruin, and the way off it).
    fn a_ruin() -> (&'static Map, IVec2, IVec2) {
        maps()
            .flat_map(|m| m.tiles().flat_map(move |(t, _)| SIDES.map(|d| (m, t, d))))
            .find(|&(m, t, d)| {
                let g = m.height_at(Map::center(t));
                m.ruin_at(t).is_some() && [1, 2].iter().all(|&k| m.walkable(t + d * k) && m.height_at(Map::center(t + d * k)) == g && m.colliders_on(t + d * k).all(|c| c.reach(Map::center(t + d * k)) > 1.0 || c.lo == Map::center(t) - Vec2::splat(0.5)))
            })
            .expect("a ruin with open ground off a side")
    }

    #[test]
    fn a_box_stops_a_body_from_the_side_holds_it_up_from_above_and_lets_it_off() {
        let (m, t, side) = a_ruin();
        let (c, out) = (Map::center(t), side.as_vec2());
        let g = m.height_at(c);
        let top = m.surface_at(c, g + m.ruin_at(t).unwrap().top() + 0.01);
        let off = c + out * 1.5;
        // At it from the side, on the ground: stopped short of it.
        assert!(m.colliders_block(off, c + out * 0.6, g));
        assert!(!m.colliders_block(off, c + out * (0.5 + NAV_RADIUS + 0.05), g));
        // Over it (and the wall it's part of, as high as that stands round it): walked across,
        // and stood on.
        let wall = m.colliders_on(t).map(|c| c.top).fold(top, f32::max);
        assert!(!m.colliders_block(off, c, wall + 0.01));
        assert!((m.floor_at(c, top) - top).abs() < 1e-4);
        // Come down beside it, reaching into it: free to move off, not on in.
        let beside = c + out * (0.5 + NAV_RADIUS - 0.1);
        assert!(!m.colliders_block(beside, beside + out * 0.1, g));
        assert!(m.colliders_block(beside, beside - out * 0.05, g));
    }

    #[test]
    fn under_the_eaves_a_head_meets_the_roof_and_out_in_the_open_nothing() {
        let m = MapId::Yard.map();
        let house = &m.houses[0];
        let roof = m.colliders().iter().find(|c| c.roof.is_some() && c.lo.cmple(house.lo.as_vec2()).all()).unwrap();
        let g = m.height_at(house.center());
        let eaves = if house.long_x() { Vec2::new(house.center().x, house.lo.y as f32 - House::EAVES / 2.0) } else { Vec2::new(house.lo.x as f32 - House::EAVES / 2.0, house.center().y) };
        assert_eq!(m.ceiling_at(eaves, g), roof.bottom - BODY_HEIGHT);
        let away = eaves - (house.center() - eaves).normalize() * 3.0;
        assert_eq!(m.ceiling_at(away, m.height_at(away)), f32::INFINITY);
    }
}
