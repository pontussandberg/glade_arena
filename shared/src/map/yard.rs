//! Practice Yard: a flat field of grass, nothing on it to hide behind or trip over, with two
//! target dummies standing on a paved square in the middle from the start and the merchant's
//! stall at its north end. For trying a hero's kit, aim and movement out, not for fighting over.

use bevy::math::{IVec2, Vec2};

use super::{Map, MapId, Tile, squircle};

const SIZE: IVec2 = IVec2::new(40, 28);

/// Meters inside the field's edge (negative outside): a rounded rectangle, no wobble.
fn edge(p: Vec2) -> f32 {
    (1.0 - squircle(Vec2::new(p.x / 16.5, p.y / 10.5))) * 12.0
}

fn beyond(_: Vec2) -> Tile {
    Tile::Forest
}

/// How far the paving reaches from the middle, each way.
const PAVING: f32 = 4.0;

/// Spawn points along the west side; each is mirrored onto the east one, so whichever side a
/// fighter comes in on, the dummies stand in front of them.
const WEST_SPAWNS: &[Vec2] = &[Vec2::new(-10.5, -6.5), Vec2::new(-10.5, -2.5), Vec2::new(-10.5, 1.5), Vec2::new(-10.5, 5.5)];

/// The two dummies, on the paving.
const DUMMIES: &[Vec2] = &[Vec2::new(0.5, 2.5), Vec2::new(0.5, -2.5)];

const SHOP: Vec2 = Vec2::new(0.5, 8.5);

pub(super) fn build() -> Map {
    let mut map = Map::blank(MapId::Yard, SIZE, edge, beyond);
    map.paint(|c, tile| if c.x.abs() < PAVING && c.y.abs() < PAVING { Tile::Path } else { tile });
    map.mirror_spots(WEST_SPAWNS, &[], &[]);
    map.dummies.extend(DUMMIES);
    // The merchant's stall at the north end, facing the yard: a counter behind them with a torch
    // at each end, the only light out here.
    map.shop = Some(SHOP);
    let counter = SHOP + Vec2::Y;
    map.line(counter - Vec2::X, counter + Vec2::X, Tile::Wall);
    map.counter = [-1.0, 0.0, 1.0].map(|x| Map::tile_of(counter + Vec2::X * x)).to_vec();
    map.torches.extend([counter - Vec2::X * 2.0, counter + Vec2::X * 2.0]);
    map
}
