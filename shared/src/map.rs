//! The arena map: a forest clearing on a 1 m tile grid, shared by server and client.
//!
//! A river runs north-south through the middle, crossed by a stone bridge and two fords. Ruined
//! walls, boulders and trees give cover. The layout is point-symmetric (x, y) -> (-x, -y), so
//! both banks are equally good.
//!
//! Everything here must give identical results on every platform (native server, wasm client),
//! because the client predicts movement with it. So the shapes use only +, -, *, / and sqrt,
//! which IEEE floats compute identically everywhere; no sin/atan2 from the platform's libm.

use std::cell::RefCell;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::rc::Rc;
use std::sync::LazyLock;

use bevy::math::{IVec2, Vec2};

/// Tiles across and down. Even, so the grid is point-symmetric around (0, 0).
pub const MAP_TILES: IVec2 = IVec2::new(64, 44);
pub const MAP_HALF_EXTENTS: Vec2 = Vec2::new(MAP_TILES.x as f32 / 2.0, MAP_TILES.y as f32 / 2.0);

/// Player clearance used when checking straight-line movement against walls. Smaller than the
/// drawn pawn (0.48 m), so a fighter can brush a wall corner slightly; it must stay below 0.5 so
/// that from any tile center the next path step is always in a clear line.
const NAV_RADIUS: f32 = 0.3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tile {
    Grass,
    /// Dirt road: walkable, visual only.
    Path,
    /// Shallow crossing through the river.
    Ford,
    Bridge,
    Water,
    Wall,
    Rock,
    Tree,
    /// Dense forest outside the clearing.
    Forest,
}

impl Tile {
    pub fn walkable(self) -> bool {
        matches!(self, Tile::Grass | Tile::Path | Tile::Ford | Tile::Bridge)
    }

    /// Projectiles fly over water but not through walls, rocks or trees.
    pub fn blocks_shots(self) -> bool {
        matches!(self, Tile::Wall | Tile::Rock | Tile::Tree | Tile::Forest)
    }
}

/// Spawn points, four per bank, mirrored.
pub const SPAWN_POINTS: [Vec2; 8] = [
    Vec2::new(-23.5, -0.5),
    Vec2::new(-20.5, 7.5),
    Vec2::new(-20.5, -8.5),
    Vec2::new(-11.5, -2.5),
    Vec2::new(23.5, 0.5),
    Vec2::new(20.5, -7.5),
    Vec2::new(20.5, 8.5),
    Vec2::new(11.5, 2.5),
];

/// Smooth periodic wave in [-1, 1] with period 4 (a sine look-alike from a cubic), using only
/// basic arithmetic so it is bit-identical on every platform.
fn wave(t: f32) -> f32 {
    let u = t - 4.0 * (t * 0.25).floor(); // [0, 4)
    let (sign, v) = if u < 2.0 { (1.0, u) } else { (-1.0, u - 2.0) }; // half period in [0, 2)
    let s = v - 1.0; // [-1, 1)
    sign * (1.0 - s * s) * (1.0 + 0.5 * (1.0 - s * s)) / 1.5
}

/// River centerline x for a given y. Odd in y, so the river is point-symmetric.
pub fn river_x(y: f32) -> f32 {
    1.8 * wave(y * 0.12)
}

pub const RIVER_HALF_WIDTH: f32 = 1.9;
pub const BRIDGE_HALF_WIDTH: f32 = 2.0;
const FORD_ROWS: [f32; 2] = [12.5, 13.5];

/// Meters inside the clearing's edge (negative outside). Continuous, for terrain and decoration.
pub fn clearing_margin(p: Vec2) -> f32 {
    let rho = ((p.x / 27.0) * (p.x / 27.0) + (p.y / 17.5) * (p.y / 17.5)).sqrt();
    // Even in (x, y): a product of two odd waves, so the edge stays point-symmetric.
    let wobble = 0.07 * wave(p.x * 0.11) * wave(p.y * 0.17) + 0.04 * wave(p.x * 0.23) * wave(p.y * 0.09);
    (1.0 + wobble - rho) * 18.0
}

pub struct Map {
    tiles: Vec<Tile>,
    /// Torch positions (decoration, but part of the layout).
    pub torches: Vec<Vec2>,
}

static MAP: LazyLock<Map> = LazyLock::new(Map::build);

/// The one arena map.
pub fn map() -> &'static Map {
    &MAP
}

/// Ruins, rocks and groves on the west bank (x < 0), in tile-center coordinates. Each is
/// mirrored onto the east bank. Walls are (from, to, torch on the `from` end).
const WEST_WALLS: &[(Vec2, Vec2, bool)] = &[
    // Ruined keep corner, with a collapsed gap.
    (Vec2::new(-15.5, 6.5), Vec2::new(-13.5, 6.5), false),
    (Vec2::new(-9.5, 6.5), Vec2::new(-11.5, 6.5), true),
    (Vec2::new(-15.5, 1.5), Vec2::new(-15.5, 5.5), true),
    // Low wall guarding the southern ford.
    (Vec2::new(-7.5, -8.5), Vec2::new(-7.5, -11.5), true),
    // Lone pillars.
    (Vec2::new(-20.5, -3.5), Vec2::new(-20.5, -3.5), false),
    (Vec2::new(-4.5, 5.5), Vec2::new(-4.5, 5.5), false),
];
const WEST_ROCKS: &[Vec2] = &[
    Vec2::new(-22.5, 3.5),
    Vec2::new(-21.5, 3.5),
    Vec2::new(-10.5, -4.5),
    Vec2::new(-4.5, -15.5),
    Vec2::new(-13.5, -12.5),
];
const WEST_TREES: &[Vec2] = &[
    Vec2::new(-18.5, -10.5),
    Vec2::new(-17.5, -11.5),
    Vec2::new(-24.5, -4.5),
    Vec2::new(-8.5, 11.5),
    Vec2::new(-3.5, 14.5),
    Vec2::new(-12.5, 12.5),
];

impl Map {
    fn build() -> Map {
        let mut map = Map { tiles: Vec::with_capacity((MAP_TILES.x * MAP_TILES.y) as usize), torches: Vec::new() };
        for j in 0..MAP_TILES.y {
            for i in 0..MAP_TILES.x {
                let c = Map::center(IVec2::new(i, j));
                let in_river = (c.x - river_x(c.y)).abs() < RIVER_HALF_WIDTH;
                let tile = if clearing_margin(c) < 0.0 {
                    Tile::Forest
                } else if in_river && c.y.abs() < BRIDGE_HALF_WIDTH {
                    Tile::Bridge
                } else if in_river && FORD_ROWS.iter().any(|r| (c.y.abs() - r).abs() < 0.01) {
                    Tile::Ford
                } else if in_river {
                    Tile::Water
                } else if c.y.abs() < 1.0 {
                    Tile::Path
                } else {
                    Tile::Grass
                };
                map.tiles.push(tile);
            }
        }
        for &(a, b, torch) in WEST_WALLS {
            let steps = (b - a).abs().max_element() as i32;
            for s in 0..=steps {
                let p = a + (b - a) * if steps == 0 { 0.0 } else { s as f32 / steps as f32 };
                map.set_mirrored(p, Tile::Wall);
            }
            if torch {
                map.torches.extend([a, -a]);
            }
        }
        for &p in WEST_ROCKS {
            map.set_mirrored(p, Tile::Rock);
        }
        for &p in WEST_TREES {
            map.set_mirrored(p, Tile::Tree);
        }
        // Torches at the four bridge corners (the ruins got theirs above).
        for y in [-BRIDGE_HALF_WIDTH - 0.4, BRIDGE_HALF_WIDTH + 0.4] {
            for side in [-1.0, 1.0] {
                map.torches.push(Vec2::new(river_x(y) + side * (RIVER_HALF_WIDTH + 0.3), y));
            }
        }
        map
    }

    fn set_mirrored(&mut self, p: Vec2, tile: Tile) {
        for q in [p, -p] {
            let t = Map::tile_of(q);
            if let Some(i) = self.index(t) {
                self.tiles[i] = tile;
            }
        }
    }

    fn index(&self, t: IVec2) -> Option<usize> {
        (t.x >= 0 && t.y >= 0 && t.x < MAP_TILES.x && t.y < MAP_TILES.y).then(|| (t.y * MAP_TILES.x + t.x) as usize)
    }

    /// Inverse of `index`.
    fn tile_at(i: usize) -> IVec2 {
        IVec2::new(i as i32 % MAP_TILES.x, i as i32 / MAP_TILES.x)
    }

    /// The tile containing a gameplay position.
    pub fn tile_of(p: Vec2) -> IVec2 {
        (p + MAP_HALF_EXTENTS).floor().as_ivec2()
    }

    pub fn center(t: IVec2) -> Vec2 {
        t.as_vec2() + Vec2::splat(0.5) - MAP_HALF_EXTENTS
    }

    /// Outside the grid counts as forest.
    pub fn get(&self, t: IVec2) -> Tile {
        self.index(t).map_or(Tile::Forest, |i| self.tiles[i])
    }

    pub fn walkable(&self, t: IVec2) -> bool {
        self.get(t).walkable()
    }

    pub fn walkable_at(&self, p: Vec2) -> bool {
        self.walkable(Map::tile_of(p))
    }

    pub fn tiles(&self) -> impl Iterator<Item = (IVec2, Tile)> + '_ {
        (0..MAP_TILES.y).flat_map(move |j| (0..MAP_TILES.x).map(move |i| (IVec2::new(i, j), self.get(IVec2::new(i, j)))))
    }

    /// True if a body of radius `NAV_RADIUS` can move in a straight line from `a` to `b`.
    ///
    /// Exact (segment against blocked tiles grown by the radius), not sampled, so every piece
    /// of a walkable line is walkable too: following a line never turns it unwalkable halfway.
    pub fn line_walkable(&self, a: Vec2, b: Vec2) -> bool {
        self.line_blocker(a, b).is_none()
    }

    /// A blocked tile that keeps a body from moving straight from `a` to `b` (`None`: nothing
    /// does). Looks from `a`'s end, so it finds an obstacle near `a` first: where a walker is,
    /// and usually what hides the rest of its path too.
    fn line_blocker(&self, a: Vec2, b: Vec2) -> Option<IVec2> {
        let half = Vec2::splat(0.5 + NAV_RADIUS);
        // Only visit tiles the grown segment can reach: per row, the x-span of the part of the
        // segment inside that row's band (padded a hair so rounding never skips a real hit).
        let band = half.y + 0.01;
        let d = b - a;
        let rows = Map::tile_of(a.min(b) - half).y..=Map::tile_of(a.max(b) + half).y;
        let (mut up, mut down) = (rows.clone(), rows.rev());
        let rows: &mut dyn Iterator<Item = i32> = if d.y >= 0.0 { &mut up } else { &mut down };
        for j in rows {
            let row_y = Map::center(IVec2::new(0, j)).y;
            let (t0, t1) = if d.y == 0.0 {
                if (a.y - row_y).abs() > band {
                    continue;
                }
                (0.0, 1.0)
            } else {
                let (u0, u1) = ((row_y - band - a.y) / d.y, (row_y + band - a.y) / d.y);
                (u0.min(u1).max(0.0), u0.max(u1).min(1.0))
            };
            if t0 > t1 {
                continue;
            }
            let (x0, x1) = (a.x + d.x * t0, a.x + d.x * t1);
            let first = Map::tile_of(Vec2::new(x0.min(x1) - band, 0.0)).x;
            let last = Map::tile_of(Vec2::new(x0.max(x1) + band, 0.0)).x;
            let (mut right, mut left) = ((first..=last), (first..=last).rev());
            let columns: &mut dyn Iterator<Item = i32> = if d.x >= 0.0 { &mut right } else { &mut left };
            for i in columns {
                if self.blocks_line(IVec2::new(i, j), a, b) {
                    return Some(IVec2::new(i, j));
                }
            }
        }
        None
    }

    /// Does tile `t`, if it's blocked, keep a body from moving straight from `a` to `b`?
    fn blocks_line(&self, t: IVec2, a: Vec2, b: Vec2) -> bool {
        let half = Vec2::splat(0.5 + NAV_RADIUS);
        !self.walkable(t) && segment_hits_box(a, b, Map::center(t) - half, Map::center(t) + half)
    }

    /// True if no wall, rock or tree lies on the line from `a` to `b` (water doesn't block).
    /// Sampled, which is fine for its uses: melee hits on the server, and the bot.
    pub fn shot_clear(&self, a: Vec2, b: Vec2) -> bool {
        let steps = ((b - a).length() / 0.25).ceil().max(1.0) as i32;
        (0..=steps).all(|s| !self.get(Map::tile_of(a.lerp(b, s as f32 / steps as f32))).blocks_shots())
    }

    /// The walkable tile closest to `t` (searching outward up to `max_radius` tiles).
    pub fn nearest_walkable(&self, t: IVec2, max_radius: i32) -> Option<IVec2> {
        (0..=max_radius).find_map(|r| {
            let mut best: Option<(i32, IVec2)> = None;
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs().max(dy.abs()) != r {
                        continue;
                    }
                    let c = t + IVec2::new(dx, dy);
                    let d = dx * dx + dy * dy;
                    if self.walkable(c) && best.is_none_or(|(bd, _)| d < bd) {
                        best = Some((d, c));
                    }
                }
            }
            best.map(|(_, c)| c)
        })
    }

    /// A* over tiles, 8 directions, no cutting corners past blocked tiles. Includes both ends.
    pub fn find_path(&self, from: IVec2, to: IVec2) -> Option<Vec<IVec2>> {
        let (start, goal) = (self.index(from)?, self.index(to)?);
        if !self.walkable(from) || !self.walkable(to) {
            return None;
        }
        let n = self.tiles.len();
        let mut cost = vec![u32::MAX; n];
        let mut came_from = vec![usize::MAX; n];
        let mut open = BinaryHeap::new();
        let heuristic = |t: IVec2| {
            let d = (t - to).abs();
            10 * d.max_element() as u32 + 4 * d.min_element() as u32
        };
        cost[start] = 0;
        open.push(Reverse((heuristic(from), start)));
        while let Some(Reverse((_, i))) = open.pop() {
            if i == goal {
                let mut path = vec![to];
                let mut at = goal;
                while at != start {
                    at = came_from[at];
                    path.push(Map::tile_at(at));
                }
                path.reverse();
                return Some(path);
            }
            let here = Map::tile_at(i);
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
                let next = here + IVec2::new(dx, dy);
                let diagonal = dx != 0 && dy != 0;
                if !self.walkable(next)
                    || (diagonal && !(self.walkable(here + IVec2::new(dx, 0)) && self.walkable(here + IVec2::new(0, dy))))
                {
                    continue;
                }
                let j = self.index(next).unwrap();
                let c = cost[i] + if diagonal { 14 } else { 10 };
                if c < cost[j] {
                    cost[j] = c;
                    came_from[j] = i;
                    open.push(Reverse((c + heuristic(next), j)));
                }
            }
        }
        None
    }

    /// Where a click at `p` sends a walker: `p` itself if a body fits there; pulled toward its
    /// tile's center if it's too close to something blocked to stand on; or, if `p` isn't on
    /// walkable ground, the center of the nearest walkable tile (up to `max_radius` tiles out).
    /// `next_waypoint` reaches whatever this returns.
    pub fn walk_target(&self, p: Vec2, max_radius: i32) -> Option<Vec2> {
        if !p.is_finite() {
            return None;
        }
        let tile = self.nearest_walkable(Map::tile_of(p), max_radius)?;
        let center = Map::center(tile);
        if tile != Map::tile_of(p) {
            return Some(center);
        }
        if self.line_walkable(center, p) {
            return Some(p);
        }
        // The farthest point toward `p` a body can still reach from the center.
        let (mut reachable, mut blocked) = (0.0, 1.0);
        for _ in 0..12 {
            let mid = (reachable + blocked) / 2.0;
            if self.line_walkable(center, center.lerp(p, mid)) {
                reachable = mid;
            } else {
                blocked = mid;
            }
        }
        Some(center.lerp(p, reachable))
    }

    /// Where to head next on the way from `pos` to `target`: the farthest point on the A* path
    /// reachable in a straight line, so movement looks direct (LoL-style) instead of zig-zagging
    /// along tiles. A target too close to a wall to stand on (see `walk_target`) means its tile's
    /// center. `None` if the target can't be reached.
    pub fn next_waypoint(&self, pos: Vec2, target: Vec2) -> Option<Vec2> {
        let target_tile = Map::tile_of(target);
        if !target.is_finite() || !self.walkable(target_tile) {
            return None;
        }
        // From the target tile's center the goal is always in a straight line.
        let target_center = Map::center(target_tile);
        let goal = if self.line_walkable(target_center, target) { target } else { target_center };
        let Some(mut blocker) = self.line_blocker(pos, goal) else { return Some(goal) };
        let start = Map::tile_of(pos);
        let path = self.cached_path(start, target_tile)?;
        // The goal is out of sight: find the farthest tile center on the path that isn't, the
        // target tile's first. Tiles next to each other on the path are mostly hidden by the
        // same obstacle, so the one that hid the last tile is tried first. Exactly what checking
        // every line in full would find, only cheaper.
        let farthest_visible = path.get(1..).unwrap_or_default().iter().rev().map(|t| Map::center(*t)).find(|&p| {
            if self.blocks_line(blocker, pos, p) {
                return false;
            }
            match self.line_blocker(pos, p) {
                Some(t) => {
                    blocker = t;
                    false
                }
                None => true,
            }
        });
        // Nothing visible (we're hugging a wall, e.g. after a server teleport): go to our own
        // tile's center first. That stays inside one walkable tile, and from any tile center the
        // next path step is always visible.
        Some(farthest_visible.unwrap_or(Map::center(start)))
    }
}

impl Map {
    /// `find_path`, remembered. Movement asks for the same (tile, target) pair every tick until
    /// the player changes tile, and again on every rollback replay; the answer only depends on
    /// those two tiles, so a cache returns exactly what a fresh search would.
    fn cached_path(&self, from: IVec2, to: IVec2) -> Option<Rc<[IVec2]>> {
        thread_local! {
            static PATHS: RefCell<HashMap<(IVec2, IVec2), Option<Rc<[IVec2]>>>> = RefCell::default();
        }
        PATHS.with_borrow_mut(|paths| {
            if paths.len() > 512 {
                paths.clear();
            }
            paths.entry((from, to)).or_insert_with(|| self.find_path(from, to).map(Rc::from)).clone()
        })
    }
}

/// Segment `a`-`b` against an axis-aligned box (slab test). Touching the boundary doesn't count.
fn segment_hits_box(a: Vec2, b: Vec2, min: Vec2, max: Vec2) -> bool {
    let d = b - a;
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    for axis in 0..2 {
        let (o, dir, lo, hi) = (a[axis], d[axis], min[axis], max[axis]);
        if dir == 0.0 {
            if o <= lo || o >= hi {
                return false;
            }
        } else {
            let (mut near, mut far) = ((lo - o) / dir, (hi - o) / dir);
            if near > far {
                std::mem::swap(&mut near, &mut far);
            }
            t0 = t0.max(near);
            t1 = t1.min(far);
            if t0 >= t1 {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_is_point_symmetric() {
        let m = map();
        for (t, tile) in m.tiles() {
            let mirror = MAP_TILES - IVec2::ONE - t;
            assert_eq!(tile, m.get(mirror), "{t} vs {mirror}");
        }
    }

    #[test]
    fn spawn_points_are_walkable() {
        for p in SPAWN_POINTS {
            assert!(map().walkable_at(p), "spawn point {p} is not walkable");
        }
    }

    #[test]
    fn banks_connect_over_bridge_and_fords() {
        let m = map();
        let west = Map::tile_of(SPAWN_POINTS[0]);
        let east = Map::tile_of(SPAWN_POINTS[4]);
        let path = m.find_path(west, east).expect("banks must be connected");
        assert!(path.iter().any(|t| m.get(*t) == Tile::Bridge), "the short way crosses the bridge");
        assert!(m.tiles().any(|(_, t)| t == Tile::Ford));
        assert!(m.tiles().filter(|(_, t)| *t == Tile::Water).count() > 100);
    }

    #[test]
    fn path_never_enters_blocked_tiles_or_cuts_corners() {
        let m = map();
        let path = m.find_path(Map::tile_of(SPAWN_POINTS[1]), Map::tile_of(SPAWN_POINTS[5])).unwrap();
        for w in path.windows(2) {
            assert!(m.walkable(w[1]));
            let d = w[1] - w[0];
            assert!(d.abs().max_element() == 1);
            if d.x != 0 && d.y != 0 {
                assert!(m.walkable(w[0] + IVec2::new(d.x, 0)) && m.walkable(w[0] + IVec2::new(0, d.y)));
            }
        }
    }

    #[test]
    fn unreachable_targets_have_no_path() {
        let m = map();
        let water = m.tiles().find(|(_, t)| *t == Tile::Water).unwrap().0;
        assert!(m.find_path(Map::tile_of(SPAWN_POINTS[0]), water).is_none());
        assert!(m.find_path(Map::tile_of(SPAWN_POINTS[0]), IVec2::new(-5, 3)).is_none());
        assert!(m.nearest_walkable(water, 6).is_some_and(|t| m.walkable(t)));
    }

    /// The pruned scan in `line_walkable` must agree with checking every blocked tile.
    #[test]
    fn line_walkable_matches_brute_force() {
        let m = map();
        let half = Vec2::splat(0.5 + NAV_RADIUS);
        let brute = |a: Vec2, b: Vec2| {
            m.tiles().all(|(t, tile)| tile.walkable() || !segment_hits_box(a, b, Map::center(t) - half, Map::center(t) + half))
        };
        let mut seed = 12345u64;
        let mut rand = |range: f32| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((seed >> 40) as f32 / (1u64 << 24) as f32 - 0.5) * 2.0 * range
        };
        for _ in 0..5000 {
            let a = Vec2::new(rand(28.0), rand(18.0));
            // Include axis-aligned segments too.
            let b = match (rand(2.0).abs() * 2.0) as i32 {
                0 => Vec2::new(a.x + rand(12.0), a.y),
                1 => Vec2::new(a.x, a.y + rand(12.0)),
                _ => a + Vec2::new(rand(12.0), rand(12.0)),
            };
            assert_eq!(m.line_walkable(a, b), brute(a, b), "{a} -> {b}");
        }
    }

    /// `next_waypoint`'s shortcuts (the remembered path, trying the last obstacle first) must
    /// give exactly what a plain search gives: client and server must agree to the bit.
    #[test]
    fn next_waypoint_matches_a_plain_search() {
        let m = map();
        let plain = |pos: Vec2, target: Vec2| {
            let tile = Map::tile_of(target);
            if !m.walkable(tile) {
                return None;
            }
            let goal = if m.line_walkable(Map::center(tile), target) { target } else { Map::center(tile) };
            if m.line_walkable(pos, goal) {
                return Some(goal);
            }
            let start = Map::tile_of(pos);
            let path = m.find_path(start, tile)?;
            let farthest = path[1..].iter().rev().map(|t| Map::center(*t)).find(|p| m.line_walkable(pos, *p));
            Some(farthest.unwrap_or(Map::center(start)))
        };
        let all: Vec<IVec2> = m.tiles().map(|(t, _)| t).collect();
        let walkable: Vec<IVec2> = all.iter().copied().filter(|t| m.walkable(*t)).collect();
        let mut seed = 777u64;
        let mut pick = |n: usize| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as usize % n
        };
        for _ in 0..2000 {
            // Anywhere inside a walkable tile, to any tile: mostly walkable ones, sometimes
            // water, walls or off the map.
            let offsets = [0, 1, 2, 3].map(|_| pick(1000) as f32 / 1000.0 - 0.5);
            let pos = Map::center(walkable[pick(walkable.len())]) + Vec2::new(offsets[0], offsets[1]);
            let to = Vec2::new(offsets[2], offsets[3]);
            let target = match pick(10) {
                0 => Map::center(all[pick(all.len())]) + to,
                1 => Vec2::new(-50.0, 3.0),
                _ => Map::center(walkable[pick(walkable.len())]) + to,
            };
            assert_eq!(m.next_waypoint(pos, target), plain(pos, target), "{pos} -> {target}");
        }
    }

    #[test]
    fn wave_is_odd_and_bounded() {
        for i in -200..200 {
            let t = i as f32 * 0.137;
            assert!((wave(t) + wave(-t)).abs() < 1e-5);
            assert!(wave(t).abs() <= 1.0 + 1e-6);
        }
    }

    #[test]
    fn walk_target_keeps_open_points_and_pulls_wall_huggers_in() {
        let m = map();
        let wall = m.tiles().find(|(t, tile)| *tile == Tile::Wall && m.walkable(*t + IVec2::X)).unwrap().0;
        let beside = wall + IVec2::X;
        // Open ground: the very point clicked.
        let open = Map::center(Map::tile_of(SPAWN_POINTS[0])) + Vec2::new(0.2, -0.3);
        assert_eq!(m.walk_target(open, 4), Some(open));
        // Right against the wall: pulled in to where a body fits, still in that tile.
        let hugging = Map::center(beside) - Vec2::new(0.45, 0.0);
        let fixed = m.walk_target(hugging, 4).unwrap();
        assert_eq!(Map::tile_of(fixed), beside);
        assert!(m.line_walkable(Map::center(beside), fixed) && fixed.x > hugging.x);
        // In the wall: the nearest walkable tile's center.
        assert!(m.walk_target(Map::center(wall), 4).is_some_and(|p| m.walkable_at(p) && p == Map::center(Map::tile_of(p))));
        assert_eq!(m.walk_target(Vec2::NAN, 4), None);
    }
}
