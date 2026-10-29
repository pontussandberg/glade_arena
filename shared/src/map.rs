//! The arena maps, on a 1 m tile grid, shared by server and client. Each room plays on one
//! (`MapId`), picked by its leader:
//!
//! - `crossing`: a forest clearing split by a river, crossed by a stone bridge and two fords.
//! - `keep`: a ruined keep on an island in its moat, marshes around it that slow whoever wades in.
//! - `gorge`: highlands split by a chasm that shots fly over but no one walks, crossed by a stone
//!   arch and two rope bridges, and by waygates that jump a fighter to the far side.
//! - `wilds`: the waves mode's map, far bigger: a camp in the middle and places to explore all
//!   round it, monsters coming out of lairs out there.
//! - `yard`: practice's: a flat field with two target dummies standing on it from the start.
//!
//! Every PvP layout (`MapId::PVP`) is point-symmetric, (x, y) -> (-x, -y), so both halves are
//! equally good; the wilds aren't, there are no sides there. Tile
//! `t` covers [t, t + 1) in meters: the grid of a map of `size` tiles runs from `-size / 2` to
//! `size / 2`, centered on (0, 0), and where a point is on it doesn't depend on the map.
//!
//! Everything here must give identical results on every platform (native server, wasm client),
//! so both agree on it (the client picks the point a click walks to, `walk_target`, and draws
//! shots by `sim`). So the shapes use only +, -, *, / and sqrt,
//! which IEEE floats compute identically everywhere; no sin/atan2 from the platform's libm.

use std::cell::RefCell;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::rc::Rc;
use std::sync::LazyLock;

use bevy::math::{IVec2, Vec2};
use serde::{Deserialize, Serialize};

use crate::protocol::PickupKind;

pub use colliders::{COUNTER_TOP, Collider, ROCK_LIFT, ROCK_RADIUS, Roof, Ruin, WELL_HEIGHT, WELL_RING, WELL_ROOF, boulder, ruin};

mod bastion;
mod colliders;
mod crossing;
mod gorge;
mod keep;
mod wilds;
mod yard;

/// Player clearance used when checking straight-line movement against walls. Smaller than the
/// drawn pawn (0.48 m), so a fighter can brush a wall corner slightly; it must stay below 0.5 so
/// that from any tile center the next path step is always in a clear line.
pub(crate) const NAV_RADIUS: f32 = 0.3;

/// How fast a fighter walks through marsh, as a share of its speed.
pub const MARSH_SPEED: f32 = 0.6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tile {
    Grass,
    /// Dirt road: walkable, visual only.
    Path,
    /// Shallow crossing through water.
    Ford,
    Bridge,
    /// A rope bridge of planks over a chasm.
    Planks,
    /// Boggy ground: walkable, but walking through it is slow (`MARSH_SPEED`).
    Marsh,
    Water,
    /// A drop no one walks, but shots fly over, like water.
    Chasm,
    Wall,
    Rock,
    Tree,
    /// Dense forest outside the clearing.
    Forest,
}

impl Tile {
    pub fn walkable(self) -> bool {
        matches!(self, Tile::Grass | Tile::Path | Tile::Ford | Tile::Bridge | Tile::Planks | Tile::Marsh)
    }

    /// Projectiles fly over water and chasms but not through walls, rocks or trees (at the
    /// height a shot flies level; a shot high enough clears what it's over: `blocks_shots_at`).
    pub fn blocks_shots(self) -> bool {
        matches!(self, Tile::Wall | Tile::Rock | Tile::Tree | Tile::Forest)
    }

    /// How high (meters) what blocks shots on it reaches: a ruined wall's blocks, a boulder, a
    /// tree's crown; the forest has no top to clear. As the scene draws them, give or take.
    pub fn top(self) -> f32 {
        match self {
            Tile::Wall => 2.8,
            Tile::Rock => 2.2,
            Tile::Tree => 6.0,
            Tile::Forest => f32::INFINITY,
            _ => 0.0,
        }
    }

    /// Whether a shot whose underside is `height` above the ground is stopped on it: by
    /// something that blocks shots, unless the shot is over its top (a lob over a wall).
    pub fn blocks_shots_at(self, height: f32) -> bool {
        self.blocks_shots() && height < self.top()
    }

    /// How fast walking on it is, as a share of a fighter's speed.
    pub fn speed_factor(self) -> f32 {
        if self == Tile::Marsh { MARSH_SPEED } else { 1.0 }
    }
}

/// Which map a room plays on.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum MapId {
    Crossing,
    /// Where every game starts, and the map rooms play on unless their leader picks another.
    #[default]
    Keep,
    Gorge,
    /// The waves mode's.
    Wilds,
    /// The waves mode's second map: a drowned fortress whose courts unlock as the run deepens.
    /// The staged variants are the same bastion with more of it open (`at_stage`); only the base
    /// is ever picked, the server advances the stage between waves.
    Bastion,
    Bastion2,
    Bastion3,
    Bastion4,
    /// Practice's own: open ground with target dummies on it from the start.
    Yard,
}

impl MapId {
    /// Every pickable map (the bastion's staged variants aren't: they're reached through
    /// `at_stage` alone).
    pub const ALL: [MapId; 6] = [MapId::Crossing, MapId::Keep, MapId::Gorge, MapId::Wilds, MapId::Bastion, MapId::Yard];
    /// The maps for fighting each other: mirrored, of a size for it.
    pub const PVP: [MapId; 3] = [MapId::Crossing, MapId::Keep, MapId::Gorge];

    pub fn map(self) -> &'static Map {
        static CROSSING: LazyLock<Map> = LazyLock::new(|| crossing::build().finished());
        static KEEP: LazyLock<Map> = LazyLock::new(|| keep::build().finished());
        static GORGE: LazyLock<Map> = LazyLock::new(|| gorge::build().finished());
        static WILDS: LazyLock<Map> = LazyLock::new(|| wilds::build().finished());
        static BASTION: LazyLock<Map> = LazyLock::new(|| bastion::build(MapId::Bastion, 0).finished());
        static BASTION2: LazyLock<Map> = LazyLock::new(|| bastion::build(MapId::Bastion2, 1).finished());
        static BASTION3: LazyLock<Map> = LazyLock::new(|| bastion::build(MapId::Bastion3, 2).finished());
        static BASTION4: LazyLock<Map> = LazyLock::new(|| bastion::build(MapId::Bastion4, 3).finished());
        static YARD: LazyLock<Map> = LazyLock::new(|| yard::build().finished());
        match self {
            MapId::Crossing => &CROSSING,
            MapId::Keep => &KEEP,
            MapId::Gorge => &GORGE,
            MapId::Wilds => &WILDS,
            MapId::Bastion => &BASTION,
            MapId::Bastion2 => &BASTION2,
            MapId::Bastion3 => &BASTION3,
            MapId::Bastion4 => &BASTION4,
            MapId::Yard => &YARD,
        }
    }

    /// The map without its stage: what pickers offer, and what records are kept under.
    pub fn base(self) -> MapId {
        match self {
            MapId::Bastion2 | MapId::Bastion3 | MapId::Bastion4 => MapId::Bastion,
            id => id,
        }
    }

    /// This map at the stage `wave` has reached: on the bastion, a court unlocks with waves 5, 9
    /// and 13; every other map is all there from the start. A stage only ever opens ground, so
    /// advancing never shuts anyone in.
    pub fn at_stage(self, wave: u32) -> MapId {
        if self.base() != MapId::Bastion {
            return self;
        }
        match (wave.max(1) - 1) / 4 {
            0 => MapId::Bastion,
            1 => MapId::Bastion2,
            2 => MapId::Bastion3,
            _ => MapId::Bastion4,
        }
    }

    /// The middle of the gate this staged bastion just opened (none for any base map): where
    /// the opening's thunder lands.
    pub fn gate_opened(self) -> Option<Vec2> {
        bastion::gate_opened(self)
    }

    /// The court this staged bastion opened, said for the banner.
    pub fn opened_blurb(self) -> Option<&'static str> {
        match self {
            MapId::Bastion2 => Some("THE CISTERN LIES OPEN"),
            MapId::Bastion3 => Some("THE ARMORY LIES OPEN"),
            MapId::Bastion4 => Some("THE SANCTUM LIES OPEN"),
            _ => None,
        }
    }

    /// The one after it in `ALL`, round to the first.
    pub fn next(self) -> MapId {
        let at = MapId::ALL.iter().position(|m| *m == self.base()).expect("every map is in ALL");
        MapId::ALL[(at + 1) % MapId::ALL.len()]
    }

    pub fn label(self) -> &'static str {
        match self.base() {
            MapId::Crossing => "River Crossing",
            MapId::Keep => "Sunken Keep",
            MapId::Gorge => "Riven Gorge",
            MapId::Wilds => "Hollow Wilds",
            MapId::Yard => "Practice Yard",
            _ => "Sunken Bastion",
        }
    }

    /// One line on what's special about it.
    pub fn blurb(self) -> &'static str {
        match self.base() {
            MapId::Crossing => "A river, a bridge and two fords",
            MapId::Keep => "A moated keep; marshes slow you",
            MapId::Gorge => "A chasm to shoot across; waygates",
            MapId::Wilds => "A camp in the wilds; monsters all round",
            MapId::Yard => "Open ground; target dummies to hit",
            _ => "A drowned fortress; its courts unlock as you survive",
        }
    }
}

pub struct Map {
    pub id: MapId,
    /// Tiles across and down. Even, so the grid is point-symmetric around (0, 0).
    pub size: IVec2,
    tiles: Vec<Tile>,
    /// Where fighters (re)spawn, mirrored.
    pub spawn_points: Vec<Vec2>,
    /// Where the pickups lie (tile centers), mirrored.
    pub pickups: Vec<(Vec2, PickupKind)>,
    /// Waygates: walking onto one end (a tile center) puts a fighter on the other. Each pair is
    /// its own mirror.
    pub gates: Vec<[Vec2; 2]>,
    /// Torch positions, each on a post (`Map::torch_post`).
    pub torches: Vec<Vec2>,
    /// Where monsters come out, in the waves mode (tile centers).
    pub lairs: Vec<Vec2>,
    /// Where chests may turn up during a wave (tile centers).
    pub chests: Vec<Vec2>,
    /// Where target dummies stand from the start, in practice (tile centers).
    pub dummies: Vec<Vec2>,
    /// Where the shopkeeper stands, if the map has a shop.
    pub shop: Option<Vec2>,
    /// Which way the shopkeeper faces: toward whoever comes to trade.
    pub shop_faces: Vec2,
    /// The tiles of the shopkeeper's counter, behind them (walls), drawn as their stall.
    pub counter: Vec<IVec2>,
    /// Houses: wall tiles under each, drawn as a house (`House`).
    pub houses: Vec<House>,
    /// Fences (from, to): rails in the way up to their tops (`FENCE_TOP`).
    pub fences: Vec<(Vec2, Vec2)>,
    /// A village well (a rock tile, drawn as the well).
    pub well: Option<Vec2>,
    /// Flower beds (tile centers): decoration.
    pub gardens: Vec<Vec2>,
    /// Stacks of crates (tile, how many high): wall tiles under each, stood on (`CRATE`).
    pub crates: Vec<(IVec2, u8)>,
    /// Everything that stands in the way, as boxes (`colliders`, worked out in `finished`), the
    /// ones near each tile (by tile index), and where on each tile a body stands, if anywhere
    /// (`stand`).
    colliders: Vec<Collider>,
    near: Vec<Vec<u16>>,
    stands: Vec<Option<Vec2>>,
    /// Which steps a walker can take from each tile (by tile index; `work_out_steps`).
    open_steps: Vec<u8>,
    /// Meters inside the playable area's edge (negative outside). Continuous, for terrain and
    /// decoration.
    edge: fn(Vec2) -> f32,
    /// How high the ground is (meters) at a gameplay position: the lie of the land, hills and
    /// ledges, continuous (`height_at`). Flat (0) on most maps. A rise steeper than `CLIMB` is a
    /// cliff: not walked up, only down off.
    relief: fn(Vec2) -> f32,
    /// What the land is beyond the playable area, for the scene: forest, unless water or a chasm
    /// runs on out there.
    beyond: fn(Vec2) -> Tile,
}

/// Flat ground: most maps' `relief`.
pub fn flat(_: Vec2) -> f32 {
    0.0
}

/// A house: its footprint of tiles (`lo` inclusive to `hi` exclusive), walls to the sim, and
/// which way its door faces (a unit direction on the grid).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct House {
    pub lo: IVec2,
    pub hi: IVec2,
    pub door: IVec2,
}

impl House {
    pub fn covers(&self, t: IVec2) -> bool {
        t.x >= self.lo.x && t.y >= self.lo.y && t.x < self.hi.x && t.y < self.hi.y
    }

    /// Its middle, and its size (meters), on the gameplay plane.
    pub fn center(&self) -> Vec2 {
        (self.lo + self.hi).as_vec2() / 2.0
    }

    pub fn size(&self) -> Vec2 {
        (self.hi - self.lo).as_vec2()
    }

    /// How high its walls stand, to the eaves (meters over its ground).
    pub const WALLS: f32 = 2.7;
    /// How steeply its roof rises from the eaves to the ridge (meters up for each meter in).
    pub const SLOPE: f32 = 0.716;
    /// How thick the roof lies over its slope.
    pub const ROOF_THICK: f32 = 0.12;
    /// How wide its chimney is, and how far its top stands over the ridge (meters).
    pub const CHIMNEY: f32 = 0.55;
    pub const CHIMNEY_TOP: f32 = 0.45;
    /// How far the roof reaches out past its walls: down its sloping sides, and past its
    /// gable ends.
    pub const EAVES: f32 = 0.5;
    pub const GABLE_EAVES: f32 = 0.4;

    /// Where its chimney stands, on the roof near one gable end.
    pub fn chimney(&self) -> Vec2 {
        let (size, c) = (self.size(), self.center());
        let (along, across, length) = if self.long_x() { (Vec2::X, Vec2::Y, size.x) } else { (Vec2::Y, Vec2::X, size.y) };
        c + along * (length / 2.0 - 0.7) + across * 0.3
    }

    /// How high its ridge is over the walls.
    pub fn rise(&self) -> f32 {
        let size = self.size();
        size.x.min(size.y) / 2.0 * Self::SLOPE
    }

    /// How high the top of its roof's ridge is (meters over the house's ground).
    pub fn ridge(&self) -> f32 {
        Self::WALLS + Self::ROOF_THICK + self.rise()
    }

    /// Whether its ridge runs along x: along its long side.
    pub fn long_x(&self) -> bool {
        let size = self.size();
        size.x >= size.y
    }

}

/// How high a fence's top rail is, a parapet over the drop it runs along, and a standing
/// torch's post (meters); how thick a parapet is, how far in from a tile's edge its middle,
/// and the least drop that has one.
pub const FENCE_TOP: f32 = 0.92;
pub const PARAPET_HEIGHT: f32 = 0.5;
pub const PARAPET_THICK: f32 = 0.36;
pub const PARAPET_IN: f32 = 0.15;
pub const PARAPET_DROP: f32 = 0.9;
pub const TORCH_POST: f32 = 2.2;
/// How tall a torch's stake is on top of a ruin (`Map::torch_post`).
pub const TORCH_STAKE: f32 = 0.6;

/// The four sides of a tile, as the steps to the tiles past them.
pub const SIDES: [IVec2; 4] = [IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y];

/// The eight steps a walker can take from a tile to the next (`Map::steps`): straight, then
/// diagonal.
const STEPS: [IVec2; 8] = [IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y, IVec2::new(1, 1), IVec2::new(1, -1), IVec2::new(-1, 1), IVec2::new(-1, -1)];

/// How tall one crate is (meters): a stack of them (`Map::crates`) is a step up to a roof.
pub const CRATE: f32 = 0.9;

/// How far above a body's feet the top of something solid may be for it to count as under the
/// body, not in its way (`Map::line_walkable_at`): enough for a body's edge reaching onto a
/// roof's rising slope, short of a crate.
pub const STEP_UP: f32 = 0.45;

/// How much the ground may rise per meter walked (a tangent): steeper is a cliff, walked down
/// off but not up (`Map::line_walkable`, `Map::steps`).
pub const CLIMB: f32 = 1.0;

/// A rounded square's "radius" through `p`: the same along its sides, smaller across its corners.
fn squircle(p: Vec2) -> f32 {
    let (x2, y2) = (p.x * p.x, p.y * p.y);
    (x2 * x2 + y2 * y2).sqrt().sqrt()
}

/// Smooth periodic wave in [-1, 1] with period 4 (a sine look-alike from a cubic), using only
/// basic arithmetic so it is bit-identical on every platform.
fn wave(t: f32) -> f32 {
    let u = t - 4.0 * (t * 0.25).floor(); // [0, 4)
    let (sign, v) = if u < 2.0 { (1.0, u) } else { (-1.0, u - 2.0) }; // half period in [0, 2)
    let s = v - 1.0; // [-1, 1)
    sign * (1.0 - s * s) * (1.0 + 0.5 * (1.0 - s * s)) / 1.5
}

impl Map {
    // Building a layout: it lists one half, and each thing in it is mirrored onto the other.

    /// A map of `size` tiles: grass inside the edge, forest outside.
    fn blank(id: MapId, size: IVec2, edge: fn(Vec2) -> f32, beyond: fn(Vec2) -> Tile) -> Map {
        let mut map = Map {
            id,
            size,
            tiles: Vec::with_capacity((size.x * size.y) as usize),
            spawn_points: Vec::new(),
            pickups: Vec::new(),
            gates: Vec::new(),
            torches: Vec::new(),
            lairs: Vec::new(),
            chests: Vec::new(),
            dummies: Vec::new(),
            shop: None,
            shop_faces: Vec2::NEG_Y,
            counter: Vec::new(),
            houses: Vec::new(),
            fences: Vec::new(),
            well: None,
            gardens: Vec::new(),
            crates: Vec::new(),
            colliders: Vec::new(),
            near: Vec::new(),
            stands: Vec::new(),
            open_steps: Vec::new(),
            edge,
            relief: flat,
            beyond,
        };
        for i in 0..(size.x * size.y) as usize {
            let c = Map::center(map.tile_at(i));
            map.tiles.push(if edge(c) < 0.0 { Tile::Forest } else { Tile::Grass });
        }
        map
    }

    /// Repaints every tile inside the edge: `f` gets its center and what it is now.
    fn paint(&mut self, f: impl Fn(Vec2, Tile) -> Tile) {
        for i in 0..self.tiles.len() {
            let c = Map::center(self.tile_at(i));
            if (self.edge)(c) >= 0.0 {
                self.tiles[i] = f(c, self.tiles[i]);
            }
        }
    }

    /// Sets the tile `p` is on and its mirror. (`p`'s tile center is what's mirrored: a point on
    /// a tile's edge, mirrored as it is, could land on the wrong one.)
    fn set_mirrored(&mut self, p: Vec2, tile: Tile) {
        let p = Map::center(Map::tile_of(p));
        debug_assert!((self.edge)(p) >= 0.0, "{:?}: {p} is out in the forest", self.id);
        for q in [p, -p] {
            if let Some(i) = self.index(Map::tile_of(q)) {
                self.tiles[i] = tile;
            }
        }
    }

    /// Sets the tile `p` is on (no mirror: for layouts that aren't mirrored).
    fn set(&mut self, p: Vec2, tile: Tile) {
        if let Some(i) = self.index(Map::tile_of(p)) {
            self.tiles[i] = tile;
        }
    }

    /// A house with its lower-left tile at `lo`, `size` tiles, its door facing `door`: walls
    /// under it (no mirror).
    fn house(&mut self, lo: IVec2, size: IVec2, door: IVec2) {
        let house = House { lo, hi: lo + size, door };
        for y in lo.y..house.hi.y {
            for x in lo.x..house.hi.x {
                if let Some(i) = self.index(IVec2::new(x, y)) {
                    self.tiles[i] = Tile::Wall;
                }
            }
        }
        self.houses.push(house);
    }

    /// The map with its colliders worked out from what's on it (`colliders::build`): its walls,
    /// rocks, houses and roofs, crates, fences, a parapet along every drop of `PARAPET_DROP` or
    /// more, torch posts and chimneys.
    fn finished(mut self) -> Map {
        self.colliders = colliders::build(&self);
        self.index_colliders();
        self.work_out_steps();
        self
    }

    /// A stack of `high` crates on tile `t`: a wall tile, stood on (its collider).
    fn crates(&mut self, t: IVec2, high: u8) {
        if let Some(i) = self.index(t) {
            self.tiles[i] = Tile::Wall;
        }
        self.crates.push((t, high));
    }

    /// Every tile in the rectangle from `lo` to `hi` (tile centers, both inclusive), no mirror.
    fn fill(&mut self, lo: Vec2, hi: Vec2, tile: Tile) {
        let (a, b) = (Map::tile_of(lo), Map::tile_of(hi));
        for y in a.y.min(b.y)..=a.y.max(b.y) {
            for x in a.x.min(b.x)..=a.x.max(b.x) {
                if let Some(i) = self.index(IVec2::new(x, y)) {
                    self.tiles[i] = tile;
                }
            }
        }
    }

    /// Every tile from `a` to `b` (tile centers, in a straight or diagonal line), no mirror.
    fn line(&mut self, a: Vec2, b: Vec2, tile: Tile) {
        for p in line_points(a, b) {
            self.set(p, tile);
        }
    }

    /// Every tile from `a` to `b` (tile centers, in a straight or diagonal line), mirrored.
    fn line_mirrored(&mut self, a: Vec2, b: Vec2, tile: Tile) {
        for p in line_points(a, b) {
            self.set_mirrored(p, tile);
        }
    }

    /// Where a walker on tile `here` can step next, and what the step costs (10 straight, 14
    /// diagonal): the eight tiles round it that are walkable, never cutting a blocked corner.
    fn steps(&self, here: IVec2) -> impl Iterator<Item = (IVec2, u32)> + '_ {
        let open = self.index(here).and_then(|i| self.open_steps.get(i)).copied().unwrap_or(0);
        STEPS.into_iter().enumerate().filter_map(move |(k, d)| {
            let next = here + d;
            let diagonal = d.x != 0 && d.y != 0;
            let clear = open & (1 << k) != 0 && self.walkable(next) && (!diagonal || (self.walkable(here + IVec2::new(d.x, 0)) && self.walkable(here + IVec2::new(0, d.y))));
            clear.then_some((next, if diagonal { 14 } else { 10 }))
        })
    }

    /// Which of the steps from each tile (`STEPS`) the ground and what stands on it let a walker
    /// take, from where it stands on the one to where it stands on the next (`stand`,
    /// `line_walkable`), a bit each: the same every time, so worked out once (`finished`).
    fn work_out_steps(&mut self) {
        self.open_steps = (0..self.tiles.len())
            .map(|i| {
                let t = self.tile_at(i);
                let here = self.stand(t);
                STEPS.iter().enumerate().filter(|(_, d)| self.line_walkable(here, self.stand(t + **d))).fold(0, |open, (k, _)| open | 1 << k)
            })
            .collect();
    }

    /// Walls (from, to, a torch on the `from` end), rocks and trees on one half, mirrored. Returns
    /// the torches (one half's).
    fn place_half(&mut self, walls: &[(Vec2, Vec2, bool)], rocks: &[Vec2], trees: &[Vec2]) -> Vec<Vec2> {
        for &(a, b, _) in walls {
            self.line_mirrored(a, b, Tile::Wall);
        }
        for &p in rocks {
            self.set_mirrored(p, Tile::Rock);
        }
        for &p in trees {
            self.set_mirrored(p, Tile::Tree);
        }
        walls.iter().filter(|(.., torch)| *torch).map(|(a, ..)| *a).collect()
    }

    /// Spawn points, pickups and torches listed for one half, with their mirrors.
    fn mirror_spots(&mut self, spawns: &[Vec2], pickups: &[(Vec2, PickupKind)], torches: &[Vec2]) {
        self.spawn_points.extend(spawns.iter().copied().chain(spawns.iter().map(|p| -*p)));
        self.pickups.extend(pickups.iter().copied().chain(pickups.iter().map(|(p, k)| (-*p, *k))));
        self.torches.extend(torches.iter().copied().chain(torches.iter().map(|p| -*p)));
    }

    pub fn half_extents(&self) -> Vec2 {
        self.size.as_vec2() / 2.0
    }

    fn index(&self, t: IVec2) -> Option<usize> {
        let u = t + self.size / 2;
        (u.x >= 0 && u.y >= 0 && u.x < self.size.x && u.y < self.size.y).then(|| (u.y * self.size.x + u.x) as usize)
    }

    /// Inverse of `index`.
    fn tile_at(&self, i: usize) -> IVec2 {
        IVec2::new(i as i32 % self.size.x, i as i32 / self.size.x) - self.size / 2
    }

    /// The tile containing a gameplay position.
    pub fn tile_of(p: Vec2) -> IVec2 {
        p.floor().as_ivec2()
    }

    pub fn center(t: IVec2) -> Vec2 {
        t.as_vec2() + Vec2::splat(0.5)
    }

    /// Outside the grid counts as forest.
    pub fn get(&self, t: IVec2) -> Tile {
        self.index(t).map_or(Tile::Forest, |i| self.tiles[i])
    }

    /// Whether a walker can stand on tile `t`: open ground, with room for a body somewhere on it
    /// beside whatever stands there (a post, a parapet, a fence's rail: `stand`).
    pub fn walkable(&self, t: IVec2) -> bool {
        self.index(t).is_some_and(|i| self.tiles[i].walkable() && self.stands.get(i).is_none_or(Option::is_some))
    }

    /// Where on tile `t` a walker stands: its middle, or the spot nearest it that's clear of
    /// what stands there (`index_colliders`). Paths go from one tile's to the next's.
    pub fn stand(&self, t: IVec2) -> Vec2 {
        self.index(t).and_then(|i| self.stands.get(i).copied().flatten()).unwrap_or_else(|| Map::center(t))
    }

    pub fn walkable_at(&self, p: Vec2) -> bool {
        self.walkable(Map::tile_of(p))
    }

    /// How fast walking at `p` is, as a share of a fighter's speed.
    pub fn speed_factor_at(&self, p: Vec2) -> f32 {
        self.get(Map::tile_of(p)).speed_factor()
    }

    /// Every tile on the grid, row by row from the bottom.
    pub fn tiles(&self) -> impl Iterator<Item = (IVec2, Tile)> + '_ {
        (0..self.tiles.len()).map(move |i| (self.tile_at(i), self.tiles[i]))
    }

    /// Meters inside the playable area's edge (negative outside), for the scene.
    pub fn edge_margin(&self, p: Vec2) -> f32 {
        (self.edge)(p)
    }

    /// How high the ground is at `p` (meters): where whatever stands there stands.
    pub fn height_at(&self, p: Vec2) -> f32 {
        (self.relief)(p)
    }

    /// The lie of the land itself (`height_at`), for the scene to stand its props on.
    pub fn relief(&self) -> fn(Vec2) -> f32 {
        self.relief
    }

    /// Whether a walker on the ground can go from `a` to `b` as far as the lie of the land and
    /// what stands on it go: up a slope no steeper than `CLIMB` (or down anything), and nothing
    /// in its way (`colliders_block`).
    fn climbable(&self, a: Vec2, b: Vec2) -> bool {
        self.height_at(b) - self.height_at(a) <= CLIMB * (b - a).length() + 1e-4 && !self.colliders_block(a, b, self.height_at(a))
    }

    /// What the land at tile `t` looks like, on the grid or off it: the tile itself in the
    /// playable area, and out in the forest whatever runs on there (water, a chasm) or forest.
    pub fn scene_tile(&self, t: IVec2) -> Tile {
        match self.get(t) {
            // Water and chasms run on out of the clearing; anything else is forest out there.
            Tile::Forest => match (self.beyond)(Map::center(t)) {
                land @ (Tile::Water | Tile::Chasm) => land,
                _ => Tile::Forest,
            },
            tile => tile,
        }
    }

    /// `line_walkable` for a body with its feet `feet` high, on the ground or up in the air:
    /// water, chasms, trees and the forest block it at any height; anything else is a collider,
    /// in its way only below its top (`colliders_block`), and stood on over it. Where it ends up
    /// may be no higher over its feet than `CLIMB` lets it step, and the edge of its square
    /// comes over no ground rising more steeply than that either: come at a terrace's face
    /// straight, at a slant or round its corner, the body stops at it, not in it. A body already
    /// reaching into a blocked tile may move out of it, never further in.
    pub fn line_walkable_at(&self, a: Vec2, b: Vec2, feet: f32) -> bool {
        let blocks = |t: IVec2| {
            let tile = self.get(t);
            // What blocks at any height: its tile as a box as tall as there is.
            let void = !tile.walkable() && !matches!(tile, Tile::Wall | Tile::Rock) && {
                let c = Map::center(t);
                Collider::block(c - Vec2::splat(0.5), c + Vec2::splat(0.5), f32::NEG_INFINITY, f32::INFINITY).blocks(a, b, feet)
            };
            void || self.colliders_on(t).any(|collider| collider.blocks(a, b, feet))
        };
        let length = (b - a).length();
        // Whether the point `o` off the body's middle at `p`, `length` along, is over ground too
        // steep to have climbed to: checked round the edge of its square, its corners and the
        // middles of its sides, for any newly there.
        let sheer = |p: Vec2, o: Vec2, length: f32| self.height_at(p + o) - feet > CLIMB * (length + o.length()) + 1e-4;
        self.line_blocker_by(a, b, blocks).is_none()
            && self.floor_at(b, feet) - feet <= CLIMB * length + 1e-4
            && !STEPS.iter().map(|d| d.as_vec2() * NAV_RADIUS).any(|o| sheer(b, o, length) && !sheer(a, o, 0.0))
    }

    /// True if a body of radius `NAV_RADIUS` can move in a straight line from `a` to `b`: nothing
    /// blocked in the way, and the ground not rising too steeply (`climbable`).
    ///
    /// Exact (segment against blocked tiles grown by the radius), not sampled, so every piece
    /// of a walkable line is walkable too: following a line never turns it unwalkable halfway.
    pub fn line_walkable(&self, a: Vec2, b: Vec2) -> bool {
        self.line_blocker(a, b).is_none() && self.climbable(a, b)
    }

    /// A blocked tile that keeps a body from moving straight from `a` to `b` (`None`: nothing
    /// does). Looks from `a`'s end, so it finds an obstacle near `a` first: where a walker is,
    /// and usually what hides the rest of its path too.
    fn line_blocker(&self, a: Vec2, b: Vec2) -> Option<IVec2> {
        self.line_blocker_by(a, b, |t| self.blocks_line(t, a, b))
    }

    /// `line_blocker`, with what keeps a tile in the way: `blocks`.
    fn line_blocker_by(&self, a: Vec2, b: Vec2, blocks: impl Fn(IVec2) -> bool) -> Option<IVec2> {
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
                if blocks(IVec2::new(i, j)) {
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

    /// Whether a shot could fly straight from a body at `a` to one at `b`, each with how high
    /// its feet are, at the height shots fly over them (`SHOT_HEIGHT`): nothing that stops shots
    /// lies between (`shot_meets`: a wall, a roof, a rock, a tree, a hillside; not a crate or a
    /// fence it flies over; water and chasms don't). For melee hits and novas on the server, and
    /// the bot's and monsters' eyes.
    pub fn shot_clear(&self, (a, from): (Vec2, f32), (b, to): (Vec2, f32)) -> bool {
        !self.shot_meets((a, from + crate::config::SHOT_HEIGHT), (b, to + crate::config::SHOT_HEIGHT))
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
                    path.push(self.tile_at(at));
                }
                path.reverse();
                return Some(path);
            }
            let here = self.tile_at(i);
            for (next, step) in self.steps(here) {
                let j = self.index(next).unwrap();
                let c = cost[i] + step;
                if c < cost[j] {
                    cost[j] = c;
                    came_from[j] = i;
                    open.push(Reverse((c + heuristic(next), j)));
                }
            }
        }
        None
    }

    /// Where a click at `p` sends a walker: `p` itself if a body fits there; pulled toward where
    /// a body stands on its tile (`stand`) if it's too close to something blocked to stand on;
    /// or, if `p` isn't on walkable ground, where one stands on the nearest walkable tile (up to
    /// `max_radius` tiles out). `next_waypoint` reaches whatever this returns.
    pub fn walk_target(&self, p: Vec2, max_radius: i32) -> Option<Vec2> {
        if !p.is_finite() {
            return None;
        }
        let tile = self.nearest_walkable(Map::tile_of(p), max_radius)?;
        Some(if tile == Map::tile_of(p) { self.standing_point(p) } else { self.stand(tile) })
    }

    /// `p` (on a walkable tile) if a body fits there, else the farthest point toward it from
    /// where one stands on its tile (`stand`) that one does. From there it's always in a straight
    /// line.
    pub(crate) fn standing_point(&self, p: Vec2) -> Vec2 {
        let stand = self.stand(Map::tile_of(p));
        if self.line_walkable(stand, p) {
            return p;
        }
        let (mut reachable, mut blocked) = (0.0, 1.0);
        for _ in 0..12 {
            let mid = (reachable + blocked) / 2.0;
            if self.line_walkable(stand, stand.lerp(p, mid)) {
                reachable = mid;
            } else {
                blocked = mid;
            }
        }
        stand.lerp(p, reachable)
    }

    /// Where to head next on the way from `pos` to `target`: the farthest point on the A* path
    /// reachable in a straight line, so movement looks direct (LoL-style) instead of zig-zagging
    /// along tiles. A target too close to a wall to stand on is pulled in first, the same way
    /// `walk_target` does, so whatever sends the target, the walk ends exactly on a point
    /// `sim::arrived` recognizes. `None` if the target can't be reached.
    pub fn next_waypoint(&self, pos: Vec2, target: Vec2) -> Option<Vec2> {
        let target_tile = Map::tile_of(target);
        if !target.is_finite() || !self.walkable(target_tile) {
            return None;
        }
        let goal = self.standing_point(target);
        if self.line_walkable(pos, goal) {
            return Some(goal);
        }
        let mut blocker = self.line_blocker(pos, goal);
        let start = Map::tile_of(pos);
        let path = self.cached_path(start, target_tile)?;
        // The goal is out of sight (or up too steep a rise): find the farthest tile on the path
        // whose stand isn't, the target tile's first. Tiles next to each other on the path are
        // mostly hidden by the same obstacle, so the one that hid the last tile is tried first.
        // Exactly what checking every line in full would find, only cheaper.
        let farthest_visible = path.get(1..).unwrap_or_default().iter().rev().map(|t| self.stand(*t)).find(|&p| {
            if blocker.is_some_and(|blocker| self.blocks_line(blocker, pos, p)) {
                return false;
            }
            match self.line_blocker(pos, p) {
                Some(t) => {
                    blocker = Some(t);
                    false
                }
                None => self.climbable(pos, p),
            }
        });
        // Nothing visible (we're hugging a wall, e.g. after a server teleport): go to where one
        // stands on our own tile first. That stays inside one walkable tile, and from any tile's
        // stand the next path step is always visible.
        Some(farthest_visible.unwrap_or(self.stand(start)))
    }
}

/// The tile centers from `a` to `b` (tile centers, in a straight or diagonal line).
fn line_points(a: Vec2, b: Vec2) -> impl Iterator<Item = Vec2> {
    let steps = (b - a).abs().max_element() as i32;
    (0..=steps).map(move |s| a + (b - a) * if steps == 0 { 0.0 } else { s as f32 / steps as f32 })
}

/// A remembered path (`None`: there's no way).
type Path = Option<Rc<[IVec2]>>;

impl Map {
    /// `find_path`, remembered. Movement asks for the same (tile, target) pair every tick until
    /// the player changes tile; the answer only depends on the map and
    /// those two tiles, so a cache returns exactly what a fresh search would.
    fn cached_path(&self, from: IVec2, to: IVec2) -> Path {
        thread_local! {
            static PATHS: RefCell<HashMap<(MapId, IVec2, IVec2), Path>> = RefCell::default();
        }
        PATHS.with_borrow_mut(|paths| {
            if paths.len() > 512 {
                paths.clear();
            }
            paths.entry((self.id, from, to)).or_insert_with(|| self.find_path(from, to).map(Rc::from)).clone()
        })
    }
}

/// How far every walkable tile is from the nearest of some sources, walking (10 a step, 14 a
/// diagonal one, no cutting corners): for many walkers all headed for the nearest of a few
/// points, one search instead of one each (`downhill`). Only the server uses it, for monsters.
pub struct FlowField {
    dist: Vec<u32>,
}

impl Map {
    /// The field toward `sources` (points on the map; ones off walkable ground are skipped).
    pub fn flow_field(&self, sources: impl IntoIterator<Item = Vec2>) -> FlowField {
        let mut dist = vec![u32::MAX; self.tiles.len()];
        let mut open = BinaryHeap::new();
        for p in sources {
            let t = Map::tile_of(p);
            if let Some(i) = self.index(t).filter(|_| self.walkable(t)) {
                dist[i] = 0;
                open.push(Reverse((0, i)));
            }
        }
        while let Some(Reverse((d, i))) = open.pop() {
            if d > dist[i] {
                continue;
            }
            let here = self.tile_at(i);
            for (next, step) in self.steps(here) {
                let j = self.index(next).expect("walkable tiles are on the grid");
                let nd = d + step;
                if nd < dist[j] {
                    dist[j] = nd;
                    open.push(Reverse((nd, j)));
                }
            }
        }
        FlowField { dist }
    }

    /// How far `p` is from the field's nearest source, walking (`None`: it can't get there).
    #[cfg(test)]
    pub fn flow_distance(&self, field: &FlowField, p: Vec2) -> Option<u32> {
        self.index(Map::tile_of(p)).map(|i| field.dist[i]).filter(|d| *d != u32::MAX)
    }

    /// Where a walker at `pos` heads to get nearer the field's sources: the farthest tile's stand
    /// it can see in a straight line, of the next few tiles downhill. `None` if there's no way.
    pub fn downhill(&self, field: &FlowField, pos: Vec2) -> Option<Vec2> {
        let mut at = Map::tile_of(pos);
        let mut best = None;
        for _ in 0..8 {
            let here = self.index(at).map(|i| field.dist[i])?;
            if here == 0 || here == u32::MAX {
                break;
            }
            let next = self.steps(at).filter_map(|(t, _)| self.index(t).map(|i| (field.dist[i], t))).min_by_key(|(d, _)| *d)?;
            if next.0 >= here {
                break;
            }
            at = next.1;
            if self.line_walkable(pos, self.stand(at)) {
                best = Some(self.stand(at));
            } else if best.is_some() {
                break;
            }
        }
        best.or_else(|| Some(self.stand(Map::tile_of(pos))))
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

    fn maps() -> impl Iterator<Item = &'static Map> {
        MapId::ALL.into_iter().map(MapId::map)
    }

    /// The mirrored ones, for fighting each other.
    fn pvp_maps() -> impl Iterator<Item = &'static Map> {
        MapId::PVP.into_iter().map(MapId::map)
    }

    #[test]
    fn layouts_are_point_symmetric() {
        for m in pvp_maps() {
            for (t, tile) in m.tiles() {
                let mirror = -t - IVec2::ONE;
                assert_eq!(tile, m.get(mirror), "{:?}: {t} vs {mirror}", m.id);
            }
            for p in &m.spawn_points {
                assert!(m.spawn_points.contains(&-*p), "{:?}: spawn point {p} has no mirror", m.id);
            }
            for p in &m.torches {
                assert!(m.torches.contains(&-*p), "{:?}: torch {p} has no mirror", m.id);
            }
        }
    }

    #[test]
    fn spawn_points_are_walkable_and_apart() {
        for m in maps() {
            assert!(m.spawn_points.len() >= 8, "{:?} has too few spawn points", m.id);
            for p in &m.spawn_points {
                assert!(m.walkable_at(*p) && m.standing_point(*p) == *p, "{:?}: spawn point {p} is not walkable", m.id);
                assert_eq!(m.speed_factor_at(*p), 1.0, "{:?}: spawn point {p} is in a marsh", m.id);
                let near = m.spawn_points.iter().filter(|q| q.distance(*p) < 3.0).count();
                assert_eq!(near, 1, "{:?}: spawn point {p} is crowded", m.id);
            }
        }
    }

    /// No walkable pocket anyone could be cut off in (or that a click could aim at in vain):
    /// every walkable tile is reachable from every spawn point.
    #[test]
    fn every_walkable_tile_is_reachable() {
        for m in maps() {
            let start = Map::tile_of(m.spawn_points[0]);
            let mut seen = std::collections::HashSet::from([start]);
            let mut todo = vec![start];
            while let Some(t) = todo.pop() {
                for d in [IVec2::X, -IVec2::X, IVec2::Y, -IVec2::Y] {
                    if m.walkable(t + d) && seen.insert(t + d) {
                        todo.push(t + d);
                    }
                }
            }
            for (t, tile) in m.tiles() {
                assert!(!m.walkable(t) || seen.contains(&t), "{:?}: {t} ({tile:?}) can't be reached", m.id);
            }
        }
    }

    #[test]
    fn pickups_lie_mirrored_on_open_ground_away_from_spawns() {
        for m in pvp_maps() {
            assert!(!m.pickups.is_empty());
            for &(at, kind) in &m.pickups {
                assert_eq!(at, Map::center(Map::tile_of(at)), "{:?}: {at} isn't a tile center", m.id);
                assert!(m.walkable_at(at), "{:?}: pickup at {at} isn't walkable", m.id);
                assert!(m.pickups.contains(&(-at, kind)), "{:?}: pickup at {at} has no mirror", m.id);
                assert!(m.spawn_points.iter().all(|s| s.distance(at) >= 4.0), "{:?}: pickup at {at} is on a spawn", m.id);
                // Not where a waygate would whisk its taker away before it's taken.
                assert!(m.gates.iter().flatten().all(|g| g.distance(at) >= 3.0), "{:?}: pickup at {at} is on a gate", m.id);
            }
        }
    }

    #[test]
    fn waygates_stand_free_mirrored_and_apart() {
        for m in pvp_maps() {
            for &[a, b] in &m.gates {
                assert_eq!(a, -b, "{:?}: gate {a} - {b} isn't its own mirror", m.id);
                for end in [a, b] {
                    assert_eq!(end, Map::center(Map::tile_of(end)), "{:?}: gate {end} isn't a tile center", m.id);
                    // Room to step on and off it from every side.
                    for d in [IVec2::ZERO, IVec2::X, -IVec2::X, IVec2::Y, -IVec2::Y] {
                        let t = Map::tile_of(end) + d;
                        assert!(m.walkable(t), "{:?}: gate {end} is hemmed in at {t}", m.id);
                    }
                    assert!(m.spawn_points.iter().all(|s| s.distance(end) >= 4.0), "{:?}: gate {end} is on a spawn", m.id);
                }
                let others = m.gates.iter().flatten().filter(|g| **g != a && **g != b);
                assert!(others.clone().all(|g| g.distance(a) >= 4.0 && g.distance(b) >= 4.0), "{:?}: gates too close", m.id);
            }
        }
    }

    #[test]
    fn halves_connect() {
        for m in pvp_maps() {
            let (west, east) = (m.spawn_points[0], -m.spawn_points[0]);
            assert!(m.find_path(Map::tile_of(west), Map::tile_of(east)).is_some(), "{:?}: halves must be connected", m.id);
        }
        let m = MapId::Crossing.map();
        let path = m.find_path(Map::tile_of(m.spawn_points[0]), Map::tile_of(-m.spawn_points[0])).unwrap();
        assert!(path.iter().any(|t| m.get(*t) == Tile::Bridge), "the short way crosses the bridge");
        assert!(m.tiles().any(|(_, t)| t == Tile::Ford));
        assert!(m.tiles().filter(|(_, t)| *t == Tile::Water).count() > 100);
    }

    #[test]
    fn each_map_has_its_features() {
        let count = |id: MapId, tile: Tile| id.map().tiles().filter(|(_, t)| *t == tile).count();
        assert!(count(MapId::Keep, Tile::Marsh) > 60, "the keep has marshes");
        assert!(count(MapId::Keep, Tile::Water) > 60, "the keep has its moat");
        assert!(count(MapId::Gorge, Tile::Chasm) > 150, "the gorge has its chasm");
        assert!(count(MapId::Gorge, Tile::Planks) > 0, "the gorge has rope bridges");
        assert!(!MapId::Gorge.map().gates.is_empty(), "the gorge has waygates");
    }

    #[test]
    fn maps_fit_twice_the_first_at_most() {
        let area = |id: MapId| id.map().size.x * id.map().size.y;
        for id in MapId::PVP {
            let m = id.map();
            assert!(m.size.x % 2 == 0 && m.size.y % 2 == 0);
            assert!(area(id) <= 2 * area(MapId::Crossing), "{id:?} is too big");
            // The edge of the playable area stays on the grid.
            for (t, tile) in m.tiles() {
                let on_rim = t.x == -m.size.x / 2 || t.y == -m.size.y / 2 || t.x == m.size.x / 2 - 1 || t.y == m.size.y / 2 - 1;
                assert!(!on_rim || tile == Tile::Forest, "{id:?}: {t} on the grid's rim isn't forest");
            }
        }
    }

    #[test]
    fn path_never_enters_blocked_tiles_or_cuts_corners() {
        for m in maps() {
            let path = m.find_path(Map::tile_of(m.spawn_points[1]), Map::tile_of(m.spawn_points[5])).unwrap();
            for w in path.windows(2) {
                assert!(m.walkable(w[1]));
                let d = w[1] - w[0];
                assert!(d.abs().max_element() == 1);
                if d.x != 0 && d.y != 0 {
                    assert!(m.walkable(w[0] + IVec2::new(d.x, 0)) && m.walkable(w[0] + IVec2::new(0, d.y)));
                }
            }
        }
    }

    #[test]
    fn unreachable_targets_have_no_path() {
        for m in maps() {
            let from = Map::tile_of(m.spawn_points[0]);
            // Water or a chasm, or a wall on a map without any (the yard's).
            let blocked = m.tiles().find(|(_, t)| matches!(t, Tile::Water | Tile::Chasm)).or_else(|| m.tiles().find(|(_, t)| *t == Tile::Wall)).unwrap().0;
            assert!(m.find_path(from, blocked).is_none());
            assert!(m.find_path(from, IVec2::new(-500, 3)).is_none());
            assert!(m.nearest_walkable(blocked, 6).is_some_and(|t| m.walkable(t)));
        }
    }

    /// Paths are remembered per map: the same two tiles on another map aren't the same path.
    #[test]
    fn remembered_paths_are_per_map() {
        for m in maps() {
            let from = m.spawn_points[0];
            for (t, _) in m.tiles().filter(|(t, _)| m.walkable(*t)).step_by(97) {
                // Through each map in turn, so another map's paths are remembered first.
                for other in maps() {
                    let _ = other.next_waypoint(from, Map::center(t));
                }
                let path = m.cached_path(Map::tile_of(from), t).map(|p| p.to_vec());
                assert_eq!(path, m.find_path(Map::tile_of(from), t), "{:?}: {t}", m.id);
            }
        }
    }

    /// The pruned scan in `line_walkable` must agree with checking every blocked tile.
    #[test]
    fn line_walkable_matches_brute_force() {
        for m in maps() {
            let half = Vec2::splat(0.5 + NAV_RADIUS);
            let brute = |a: Vec2, b: Vec2| {
                m.tiles().all(|(t, _)| m.walkable(t) || !segment_hits_box(a, b, Map::center(t) - half, Map::center(t) + half))
            };
            let mut seed = 12345u64;
            let mut rand = |range: f32| {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                ((seed >> 40) as f32 / (1u64 << 24) as f32 - 0.5) * 2.0 * range
            };
            let extent = m.half_extents() - Vec2::splat(4.0);
            for _ in 0..3000 {
                let a = Vec2::new(rand(extent.x), rand(extent.y));
                // Include axis-aligned segments too.
                let b = match (rand(2.0).abs() * 2.0) as i32 {
                    0 => Vec2::new(a.x + rand(12.0), a.y),
                    1 => Vec2::new(a.x, a.y + rand(12.0)),
                    _ => a + Vec2::new(rand(12.0), rand(12.0)),
                };
                // The obstacles' part of it: the ground's rise is its own rule (`climbable`).
                assert_eq!(m.line_blocker(a, b).is_none(), brute(a, b), "{:?}: {a} -> {b}", m.id);
            }
        }
    }

    #[test]
    fn the_yards_high_ground_is_climbed_by_its_ramp_not_its_sides() {
        let m = MapId::Yard.map();
        let (middle, radius, top) = (Vec2::new(-9.5, 23.5), 3.5, 6.0);
        assert!((m.height_at(middle) - top).abs() < 1e-5, "the plateau's top");
        // Up its sheer east side is no way; down it is, hopping its parapet.
        let (foot, brink) = (middle + Vec2::X * (radius + 0.7), middle + Vec2::X * (radius - 0.9));
        assert!(!m.line_walkable(foot, brink), "walked up a cliff");
        assert!(!m.line_walkable(brink, foot), "walked through the parapet");
        assert!(m.line_walkable_at(brink, foot, top + 0.3), "couldn't hop down off it");
        // Up the stairs from the square onto the terrace, and on up the ramp onto the plateau, a
        // walker's step at a time.
        let mut p = Vec2::new(-9.5, 5.0);
        while p.y < 12.0 {
            let next = p + Vec2::Y * 0.12;
            assert!(m.line_walkable(p, next), "the stairs aren't walkable at {p}");
            p = next;
        }
        assert!((m.height_at(p) - 2.5).abs() < 1e-5, "the stairs didn't come out on the terrace");
        // Nor up the stairs' sides, where they're drawn: a stair's side is a tile edge, sheer
        // from just past it to the stair.
        for y in [7.5, 8.5, 9.5] {
            let (beside, on) = (Vec2::new(-7.9, y), Vec2::new(-8.1, y));
            assert!(m.height_at(beside) < 1e-5, "the ground beside the stair is raised at {beside}");
            assert!(m.height_at(on) > 0.5, "the stair doesn't reach its drawn side at {on}");
            assert!(!m.line_walkable(beside, on), "walked up the stair's side at {on}");
        }
        let mut p = Vec2::new(middle.x, 12.0);
        while p.y < middle.y {
            let next = p + Vec2::Y * 0.12;
            assert!(m.line_walkable(p, next), "the ramp isn't walkable at {p}");
            p = next;
        }
        assert!((m.height_at(p) - top).abs() < 1e-5, "the ramp didn't come out on top");
        // Up the tower's switchback onto its top: east along its south side, north along its
        // east side, and west onto it.
        let mut p = Vec2::new(-16.0, -13.5);
        while p.x < -7.5 {
            let next = p + Vec2::X * 0.12;
            assert!(m.line_walkable(p, next), "the tower's first leg isn't walkable at {p}");
            p = next;
        }
        while p.y < -7.5 {
            let next = p + Vec2::Y * 0.12;
            assert!(m.line_walkable(p, next), "the tower's second leg isn't walkable at {p}");
            p = next;
        }
        while p.x > -10.5 {
            let next = p - Vec2::X * 0.12;
            assert!(m.line_walkable(p, next), "couldn't step onto the tower at {p}");
            p = next;
        }
        assert!((m.height_at(p) - 6.0).abs() < 1e-5, "the tower's ramp didn't come out on top: {} at {p}", m.height_at(p));
        // Up the south stair onto the inn's terrace.
        let mut p = Vec2::new(4.5, -5.0);
        while p.y > -10.0 {
            let next = p - Vec2::Y * 0.12;
            assert!(m.line_walkable(p, next), "the south stair isn't walkable at {p}");
            p = next;
        }
        assert!((m.height_at(p) - 1.5).abs() < 1e-5, "the south stair didn't come out on the inn's terrace");
    }

    /// `next_waypoint`'s shortcuts (the remembered path, trying the last obstacle first) must
    /// give exactly what a plain search gives: client and server must agree to the bit.
    #[test]
    fn next_waypoint_matches_a_plain_search() {
        for m in maps() {
            let plain = |pos: Vec2, target: Vec2| {
                let tile = Map::tile_of(target);
                if !m.walkable(tile) {
                    return None;
                }
                let goal = m.standing_point(target);
                if m.line_walkable(pos, goal) {
                    return Some(goal);
                }
                let start = Map::tile_of(pos);
                let path = m.find_path(start, tile)?;
                let farthest = path[1..].iter().rev().map(|t| m.stand(*t)).find(|p| m.line_walkable(pos, *p));
                Some(farthest.unwrap_or(m.stand(start)))
            };
            let all: Vec<IVec2> = m.tiles().map(|(t, _)| t).collect();
            let walkable: Vec<IVec2> = all.iter().copied().filter(|t| m.walkable(*t)).collect();
            let mut seed = 777u64;
            let mut pick = |n: usize| {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                (seed >> 33) as usize % n
            };
            for _ in 0..1000 {
                // Anywhere inside a walkable tile, to any tile: mostly walkable ones, sometimes
                // water, walls or off the map.
                let offsets = [0, 1, 2, 3].map(|_| pick(1000) as f32 / 1000.0 - 0.5);
                let pos = Map::center(walkable[pick(walkable.len())]) + Vec2::new(offsets[0], offsets[1]);
                let to = Vec2::new(offsets[2], offsets[3]);
                let target = match pick(10) {
                    0 => Map::center(all[pick(all.len())]) + to,
                    1 => Vec2::new(-500.0, 3.0),
                    _ => Map::center(walkable[pick(walkable.len())]) + to,
                };
                assert_eq!(m.next_waypoint(pos, target), plain(pos, target), "{:?}: {pos} -> {target}", m.id);
            }
        }
    }

    #[test]
    fn every_map_has_a_merchant_on_open_ground() {
        for id in MapId::ALL {
            let m = id.map();
            assert!(m.shop.is_some_and(|shop| m.walkable_at(shop)), "{id:?}");
        }
    }

    #[test]
    fn the_wilds_have_a_camp_lairs_and_chests_on_open_ground() {
        let m = MapId::Wilds.map();
        let shop = m.shop.expect("the wilds have a shop");
        assert!(m.walkable_at(shop));
        assert!(m.lairs.len() >= 6 && m.chests.len() >= 8);
        for &p in m.lairs.iter().chain(&m.chests).chain(m.pickups.iter().map(|(p, _)| p)) {
            assert!(m.walkable_at(p), "{p} isn't on open ground");
            assert_eq!(p, Map::center(Map::tile_of(p)), "{p} isn't a tile center");
        }
        for &lair in &m.lairs {
            assert!(m.spawn_points.iter().all(|s| s.distance(lair) > 25.0), "a lair at {lair} is too near the camp");
            assert!(m.find_path(Map::tile_of(lair), Map::tile_of(m.spawn_points[0])).is_some(), "a lair at {lair} is cut off");
        }
        for &[a, b] in &m.gates {
            for end in [a, b] {
                for d in [IVec2::ZERO, IVec2::X, -IVec2::X, IVec2::Y, -IVec2::Y] {
                    assert!(m.walkable(Map::tile_of(end) + d), "gate {end} is hemmed in");
                }
            }
        }
        // Everyone spawns where they can reach the shop.
        for &s in &m.spawn_points {
            assert!(s.distance(shop) < crate::config::JUMP * 4.0);
        }
        let count = |tile: Tile| m.tiles().filter(|(_, t)| *t == tile).count();
        assert!(count(Tile::Water) > 100 && count(Tile::Chasm) > 50 && count(Tile::Marsh) > 100 && count(Tile::Planks) > 0);
    }

    #[test]
    fn the_bastion_opens_court_by_court() {
        let stages = [MapId::Bastion, MapId::Bastion2, MapId::Bastion3, MapId::Bastion4];
        // A spot well inside each court, in the order the gates open: east, west, north.
        let courts = [Vec2::new(26.5, -24.5), Vec2::new(-26.5, 24.5), Vec2::new(9.5, 26.5)];
        for (stage, id) in stages.into_iter().enumerate() {
            let m = id.map();
            let camp = m.spawn_points[0];
            assert!(m.shop.is_some_and(|shop| m.walkable_at(shop)), "{id:?}");
            assert!(!m.lairs.is_empty() && !m.chests.is_empty(), "{id:?}");
            for &p in m.lairs.iter().chain(&m.chests).chain(m.pickups.iter().map(|(p, _)| p)) {
                assert!(m.walkable_at(p), "{id:?}: {p} isn't on open ground");
                assert!(m.find_path(Map::tile_of(camp), Map::tile_of(p)).is_some(), "{id:?}: {p} is cut off");
            }
            // The sealed courts really are sealed, and every opened one can be walked into.
            for (k, &court) in courts.iter().enumerate() {
                let open = k < stage;
                let path = m.find_path(Map::tile_of(camp), Map::tile_of(court));
                assert_eq!(path.is_some(), open, "{id:?}: the court at {court}");
            }
        }
        assert_eq!(MapId::Bastion.at_stage(1), MapId::Bastion);
        assert_eq!(MapId::Bastion.at_stage(4), MapId::Bastion);
        assert_eq!(MapId::Bastion.at_stage(5), MapId::Bastion2);
        assert_eq!(MapId::Bastion4.at_stage(2), MapId::Bastion);
        assert_eq!(MapId::Bastion.at_stage(13), MapId::Bastion4);
        assert_eq!(MapId::Wilds.at_stage(13), MapId::Wilds);
        assert_eq!(MapId::Bastion2.gate_opened(), Some(Vec2::new(11.5, -0.5)));
    }

    #[test]
    fn a_flow_field_leads_downhill_to_its_sources() {
        let m = MapId::Wilds.map();
        let goal = m.spawn_points[0];
        let field = m.flow_field([goal]);
        for &lair in &m.lairs {
            let mut p = lair;
            let start = m.flow_distance(&field, p).expect("every lair reaches the camp");
            for _ in 0..4000 {
                if m.line_walkable(p, goal) {
                    break;
                }
                let next = m.downhill(&field, p).expect("a way on");
                assert!(m.line_walkable(p, next), "{p} -> {next} goes through something");
                p = p.move_towards(next, 0.25);
            }
            assert!(m.line_walkable(p, goal), "from {lair} ({start}) stuck at {p}");
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
        let m = MapId::Crossing.map();
        let wall = m.tiles().find(|(t, tile)| *tile == Tile::Wall && m.walkable(*t + IVec2::X)).unwrap().0;
        let beside = wall + IVec2::X;
        // Open ground: the very point clicked.
        let open = Map::center(Map::tile_of(m.spawn_points[0])) + Vec2::new(0.2, -0.3);
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

    /// Draws every map in text, to look at: `cargo test -p arena-shared draw_maps -- --ignored
    /// --nocapture`.
    #[test]
    #[ignore]
    fn draw_maps() {
        for m in maps() {
            println!("{:?} ({} x {})", m.id, m.size.x, m.size.y);
            for j in (-m.size.y / 2..m.size.y / 2).rev() {
                let row: String = (-m.size.x / 2..m.size.x / 2)
                    .map(|i| {
                        let t = IVec2::new(i, j);
                        let c = Map::center(t);
                        if m.spawn_points.iter().any(|p| Map::tile_of(*p) == t) {
                            return 'S';
                        }
                        if let Some((_, kind)) = m.pickups.iter().find(|(p, _)| Map::tile_of(*p) == t) {
                            return if *kind == PickupKind::Heal { 'H' } else { 'F' };
                        }
                        if m.gates.iter().flatten().any(|g| Map::tile_of(*g) == t) {
                            return 'O';
                        }
                        if m.torches.iter().any(|p| p.distance(c) < 0.6) {
                            return 'i';
                        }
                        match m.get(t) {
                            Tile::Grass => '.',
                            Tile::Path => ':',
                            Tile::Ford => '=',
                            Tile::Bridge => '#',
                            Tile::Planks => '%',
                            Tile::Marsh => ',',
                            Tile::Water => '~',
                            Tile::Chasm => ' ',
                            Tile::Wall => 'W',
                            Tile::Rock => 'o',
                            Tile::Tree => 'T',
                            Tile::Forest => '^',
                        }
                    })
                    .collect();
                println!("{row}");
            }
            println!();
        }
    }
}
