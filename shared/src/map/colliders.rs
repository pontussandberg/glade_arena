//! What stands in the way on a map, as boxes: every wall, rock, crate, house and roof, fence,
//! parapet, post and chimney is one or a few (`Collider`), worked out once from the map
//! (`build`) and listed by the tiles they're near, so a move only looks at what's around it.
//! A box stops a body from the side up to its top, holds it up from above, and lets it under if
//! the body fits beneath (`Map::colliders_block`, `Map::floor_at`, `Map::ceiling_at`). The scene
//! draws each from the same numbers (`ruin`, `boulder`), so what's drawn is what's hit.

use bevy::math::{IVec2, Vec2, Vec3};

use super::{CRATE, FENCE_TOP, House, Map, NAV_RADIUS, PARAPET_DROP, PARAPET_HEIGHT, PARAPET_IN, PARAPET_THICK, STEP_UP, TORCH_POST, Tile};
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
    fn block(lo: Vec2, hi: Vec2, bottom: f32, top: f32) -> Collider {
        Collider { lo: lo.min(hi), hi: lo.max(hi), bottom, top, roof: None }
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

    /// Whether it keeps a body with its feet `feet` high from moving straight from `a` to `b`:
    /// the move comes within the body's reach of it, it isn't under the body (its top no more
    /// than `STEP_UP` over the feet) nor over its head (its bottom no lower than `BODY_HEIGHT`
    /// over them), and the body isn't already reaching into it and getting no closer (it came
    /// down beside it: it may always move off).
    fn blocks(&self, a: Vec2, b: Vec2, feet: f32) -> bool {
        let grown = Vec2::splat(NAV_RADIUS);
        if self.bottom >= feet + BODY_HEIGHT || self.top_at(b.clamp(self.lo, self.hi)) <= feet + STEP_UP {
            return false;
        }
        if !super::segment_hits_box(a, b, self.lo - grown, self.hi + grown) {
            return false;
        }
        !(self.reach(a) < NAV_RADIUS && self.reach(b) >= self.reach(a))
    }
}

/// How far past its edge a box still holds a body up: a body stands on a fence's rail or a
/// parapet's top while its middle is over it or a little beyond.
const SUPPORT: f32 = 0.15;

/// A number in [0, 1) for tile `t`, the same everywhere (client and server, every run): a
/// different one for each `salt`.
pub fn jitter(t: IVec2, salt: u32) -> f32 {
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
const WELL_ROOF: (Vec2, f32, f32) = (Vec2::new(1.0, 0.75), 2.2, 2.7);

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
        let slope = House::PITCH.tan();
        let reach = if house.long_x() { Vec2::new(House::GABLE_EAVES, House::EAVES) } else { Vec2::new(House::EAVES, House::GABLE_EAVES) };
        let ridge = if house.long_x() { house.center().y } else { house.center().x };
        out.push(Collider {
            lo: lo - reach,
            hi: hi + reach,
            bottom: g + House::WALLS - House::EAVES * slope - 0.05,
            top: g + house.roof_at(house.center()),
            roof: Some(Roof { along_x: house.long_x(), ridge, slope }),
        });
        let chimney = house.chimney();
        out.push(Collider::block(chimney - Vec2::splat(0.28), chimney + Vec2::splat(0.28), g + House::WALLS, g + House::WALLS + house.rise() + 0.45));
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
    let taken = |t: IVec2| {
        map.houses.iter().any(|house| house.covers(t)) || map.crates.iter().any(|(at, _)| *at == t) || map.counter.contains(&t) || map.well.is_some_and(|well| Map::tile_of(well) == t)
    };
    for (t, tile) in map.tiles() {
        if taken(t) {
            continue;
        }
        let c = Map::center(t);
        let g = ground(c);
        match tile {
            Tile::Wall => {
                let ruin = ruin(t);
                out.push(tile_box(t, g, g + ruin.height));
                if let Some((size, off)) = ruin.cap {
                    out.push(Collider::block(c + off - Vec2::splat(size / 2.0), c + off + Vec2::splat(size / 2.0), g + ruin.height, g + ruin.height + size));
                }
            }
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
    let sides = [IVec2::X, IVec2::NEG_X, IVec2::Y, IVec2::NEG_Y];
    for (t, tile) in map.tiles() {
        if tile == Tile::Forest {
            continue;
        }
        let c = Map::center(t);
        let here = ground(c);
        for d in sides {
            if here - ground(Map::center(t + d)) < PARAPET_DROP || map.scene_tile(t + d) == Tile::Forest {
                continue;
            }
            let mid = c + d.as_vec2() * (0.5 - PARAPET_IN);
            let half = d.as_vec2().abs() * (PARAPET_THICK / 2.0) + d.as_vec2().perp().abs() * 0.5;
            out.push(Collider::block(mid - half, mid + half, here, here + PARAPET_HEIGHT));
        }
    }
    for &p in &map.torches {
        let t = Map::tile_of(p);
        let (base, post) = if map.get(t) == Tile::Wall && !taken(t) { (ruin(t).top(), 0.6) } else { (0.0, TORCH_POST) };
        let g = ground(p) + base;
        out.push(Collider::block(p - Vec2::splat(0.07), p + Vec2::splat(0.07), g, g + post));
    }
    out
}

impl Map {
    /// The colliders near the stretch from `lo` to `hi` (on the plane), each once.
    fn colliders_near(&self, lo: Vec2, hi: Vec2) -> Vec<&Collider> {
        let (a, b) = (Map::tile_of(lo), Map::tile_of(hi));
        let mut found: Vec<u16> = Vec::new();
        for y in a.y..=b.y {
            for x in a.x..=b.x {
                if let Some(i) = self.index(IVec2::new(x, y)) {
                    found.extend(self.near.get(i).map_or(&[][..], |near| &near[..]));
                }
            }
        }
        found.sort_unstable();
        found.dedup();
        found.into_iter().map(|i| &self.colliders[i as usize]).collect()
    }

    /// Lists each collider on the tiles it reaches onto, grown by a body's reach, and marks the
    /// tiles a body can't stand in the middle of for something on the ground there (`walkable`):
    /// a post, a parapet, a fence's rail.
    pub(super) fn index_colliders(&mut self) {
        let mut near = vec![Vec::new(); self.tiles.len()];
        let mut crowded = vec![false; self.tiles.len()];
        let grown = Vec2::splat(NAV_RADIUS + 0.01);
        for (i, collider) in self.colliders.iter().enumerate() {
            let (a, b) = (Map::tile_of(collider.lo - grown), Map::tile_of(collider.hi + grown));
            for y in a.y..=b.y {
                for x in a.x..=b.x {
                    let t = IVec2::new(x, y);
                    let Some(k) = self.index(t) else { continue };
                    near[k].push(i as u16);
                    let c = Map::center(t);
                    let g = self.height_at(c);
                    let on_ground = collider.bottom < g + BODY_HEIGHT && collider.top_at(c) > g + STEP_UP;
                    if on_ground && collider.reach(c) < NAV_RADIUS {
                        crowded[k] = true;
                    }
                }
            }
        }
        self.near = near;
        self.crowded = crowded;
    }

    /// Whether a collider keeps a body with its feet `feet` high from moving straight from `a`
    /// to `b` (`Collider::blocks`).
    pub fn colliders_block(&self, a: Vec2, b: Vec2, feet: f32) -> bool {
        let grown = Vec2::splat(NAV_RADIUS);
        self.colliders_near(a.min(b) - grown, a.max(b) + grown).iter().any(|collider| collider.blocks(a, b, feet))
    }

    /// What a body with its feet `feet` high stands on at `p`: the ground, or the top of a
    /// collider under it (no more than `STEP_UP` over its feet, its middle over it or `SUPPORT`
    /// beyond), whichever is highest.
    pub fn floor_at(&self, p: Vec2, feet: f32) -> f32 {
        let grown = Vec2::splat(SUPPORT);
        self.colliders_near(p - grown, p + grown)
            .iter()
            .filter(|collider| collider.reach(p) <= SUPPORT)
            .map(|collider| collider.top_at(p.clamp(collider.lo, collider.hi)))
            .filter(|&top| top <= feet + STEP_UP)
            .fold(self.height_at(p), f32::max)
    }

    /// How high a body with its feet `feet` high at `p` may rise before its head meets the
    /// underside of something over it (`f32::INFINITY`: nothing is).
    pub fn ceiling_at(&self, p: Vec2, feet: f32) -> f32 {
        let grown = Vec2::splat(NAV_RADIUS);
        self.colliders_near(p - grown, p + grown)
            .iter()
            .filter(|collider| collider.reach(p) < NAV_RADIUS && collider.bottom >= feet + BODY_HEIGHT - 1e-3)
            .map(|collider| collider.bottom - BODY_HEIGHT)
            .fold(f32::INFINITY, f32::min)
    }

    /// Every collider (for showing them: the "3D hitboxes" switch).
    pub fn colliders(&self) -> &[Collider] {
        &self.colliders
    }
}
