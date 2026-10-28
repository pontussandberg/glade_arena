//! What a strike's marks lie on, and laying them there: the ground and whatever stands on it
//! under the strike's lane, followed out from the striker (`struck_surface`): on up a slope
//! however high it goes, short of a step too high for its blade, and down off a drop onto
//! whatever's below. Each mark is laid over it in pieces, one on each surface it comes over,
//! none of it out in the air.

use std::f32::consts::SQRT_2;

use arena_shared::map::CLIMB;
use arena_shared::sim::{STRUCK_STEP, surface_on};
use bevy::platform::collections::HashMap;

use super::*;

use crate::arena::{FlatMesh, to_world};

/// How far apart (meters) the surfaces under a lane are looked up, going out from the striker
/// as the sim does (`struck_surface`): in between, smoothly where the looks round a point are
/// all on one surface, afresh at the point itself where they aren't.
const LOOK_STEP: f32 = STRUCK_STEP;

/// The surfaces under a strike's lane, from `origin` (in the world's plane) along `dir` and
/// across it to its right, for a striker with its feet `feet` high: followed out along the lane
/// from beside the striker (`surface_on`), looked up as they're needed, each look kept.
#[derive(Clone)]
pub(crate) struct Underfoot {
    map: &'static Map,
    origin: Vec2,
    dir: Vec2,
    feet: f32,
    /// How high the surface is at each grid point looked at (`LOOK_STEP` apart).
    looked: HashMap<IVec2, f32>,
}

impl Underfoot {
    pub(crate) fn new(map: &'static Map, origin: Vec2, dir: Vec2, feet: f32) -> Self {
        Underfoot { map, origin, dir: dir.normalize_or(Vec2::X), feet, looked: HashMap::default() }
    }

    /// How high the striker's feet are.
    pub(crate) fn feet(&self) -> f32 {
        self.feet
    }

    /// The lane's point `local` (along it, and across it to its right) in the world's plane.
    pub(crate) fn at(&self, local: Vec2) -> Vec2 {
        self.origin + self.dir * local.x + Vec2::new(self.dir.y, -self.dir.x) * local.y
    }

    /// How high the surface is at the grid point `k`: followed out to it along the lane from the
    /// last point looked at before it (or from beside the striker).
    fn look(&mut self, k: IVec2) -> f32 {
        if let Some(&h) = self.looked.get(&k) {
            return h;
        }
        let mut from = k.x;
        while from > 0 && !self.looked.contains_key(&IVec2::new(from - 1, k.y)) {
            from -= 1;
        }
        let mut before = if from > 0 { self.looked[&IVec2::new(from - 1, k.y)] } else { f32::NEG_INFINITY };
        for i in from..=k.x {
            let at = IVec2::new(i, k.y);
            before = surface_on(self.map, self.at(at.as_vec2() * LOOK_STEP), self.feet, before, LOOK_STEP);
            self.looked.insert(at, before);
        }
        before
    }

    /// How high the surface under the lane's point `local` is.
    pub(crate) fn height(&mut self, local: Vec2) -> f32 {
        let g = local / LOOK_STEP;
        let k = g.floor().as_ivec2();
        let f = g - k.as_vec2();
        let [a, b, c, d] = [k, k + IVec2::X, k + IVec2::Y, k + IVec2::ONE].map(|corner| self.look(corner));
        // All on one surface, level or sloping (no steeper than a climb).
        if a.max(b).max(c).max(d) - a.min(b).min(c).min(d) <= CLIMB * LOOK_STEP * SQRT_2 + 1e-3 {
            return a.lerp(b, f.x).lerp(c.lerp(d, f.x), f.y);
        }
        // Afresh, on out from the looks just before it along the lane.
        surface_on(self.map, self.at(local), self.feet, a.max(c), LOOK_STEP * SQRT_2)
    }
}

/// A mark's `shape` laid over the surfaces `under` a strike's lane, `lift` over them, `place`
/// putting each of its points in the lane's frame (along it +X, up +Y, across it to its right
/// +Z; how far up is over the surface): cut into triangles no more than `edge` a side, each
/// lying on what's under it, and dropped where it runs across an edge from one surface onto
/// another, so the mark breaks there into pieces.
/// In the world.
pub(crate) fn lay(shape: &FlatMesh, under: &mut Underfoot, lift: f32, edge: f32, place: impl Fn(Vec3) -> Vec3) -> Mesh {
    let mut laid = FlatMesh::default();
    let mut left: Vec<([Vec3; 3], [f32; 3])> = shape.faded_triangles().map(|(corners, alphas)| (corners.map(&place), alphas)).collect();
    while let Some((corners, alphas)) = left.pop() {
        let flat = corners.map(|p| p.xz());
        let (longest, i) = (0..3).map(|i| (flat[i].distance(flat[(i + 1) % 3]), i)).fold((0.0, 0), |most, side| if side.0 > most.0 { side } else { most });
        // Halved across its longest side until small enough to follow what it's on.
        if longest > edge {
            let j = (i + 1) % 3;
            let (mid, alpha) = (corners[i].lerp(corners[j], 0.5), (alphas[i] + alphas[j]) / 2.0);
            let (mut first, mut first_alphas, mut second, mut second_alphas) = (corners, alphas, corners, alphas);
            (first[j], first_alphas[j]) = (mid, alpha);
            (second[i], second_alphas[i]) = (mid, alpha);
            left.push((first, first_alphas));
            left.push((second, second_alphas));
            continue;
        }
        let [a, b, c] = flat.map(|p| under.height(p));
        if a.max(b).max(c) - a.min(b).min(c) > CLIMB * longest + 1e-3 {
            continue;
        }
        let heights = [a, b, c];
        laid.tri_faded([0, 1, 2].map(|k| to_world(under.at(flat[k]), heights[k] + lift + corners[k].y)), alphas);
    }
    laid.build()
}

/// How a laid mark was last laid (`Laid`): which of its shapes, stretched how (to a fortieth).
type LaidAs = (usize, IVec3);

/// A mark lying on the surfaces under a strike's lane (`lay`), one of `shapes` (by step: a
/// thinner one as it closes, say) stretched as it plays out, laid again into `mesh` when what's
/// drawn has changed enough to show.
pub(crate) struct Laid {
    pub(crate) mesh: Handle<Mesh>,
    shapes: Vec<FlatMesh>,
    lift: f32,
    edge: f32,
    laid: Option<LaidAs>,
}

impl Laid {
    pub(crate) fn new(meshes: &mut Assets<Mesh>, shapes: Vec<FlatMesh>, lift: f32, edge: f32) -> Self {
        Laid { mesh: meshes.add(FlatMesh::default().build()), shapes, lift, edge, laid: None }
    }

    /// Its shape `step` laid with each point at `from + stretch * point` in the lane's frame (if
    /// that's not how it lies already).
    pub(crate) fn lay(&mut self, meshes: &mut Assets<Mesh>, under: &mut Underfoot, step: usize, from: Vec3, stretch: Vec3) {
        let step = step.min(self.shapes.len() - 1);
        let laid_as = (step, (stretch * 40.0).round().as_ivec3());
        if self.laid == Some(laid_as) {
            return;
        }
        self.laid = Some(laid_as);
        if let Some(mut mesh) = meshes.get_mut(&self.mesh) {
            *mesh = lay(&self.shapes[step], under, self.lift, self.edge, |p| from + stretch * p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arena_shared::map::{CRATE, House, MapId};
    use arena_shared::protocol::ClassId;

    use crate::arena::to_gameplay;
    use crate::swish::{CORE_THICKNESS, rift_shape, rubble_shape};

    /// The revenant's plain strike's tear (the broken ground and the light in its cracks), struck
    /// on the yard from `pos` along `dir` with its feet `feet` high, laid on what's under its
    /// lane: its triangles in the world.
    fn torn(pos: Vec2, dir: Vec2, feet: f32) -> Vec<[Vec3; 3]> {
        let (from, range, width) = ClassId::by_key("revenant").unwrap().def().attack.lane(false).unwrap();
        let mut under = Underfoot::new(MapId::Yard.map(), pos, dir, feet);
        let mut laid = Vec::new();
        for shape in tear(range - from, width) {
            let mesh = lay(&shape, &mut under, 0.0, 0.05, |p| Vec3::X * from + p);
            let Some(positions) = mesh.attribute(Mesh::ATTRIBUTE_POSITION).and_then(|a| a.as_float3()) else { panic!("no positions") };
            laid.extend(positions.chunks_exact(3).map(|t| [0, 1, 2].map(|i| Vec3::from_array(t[i]))));
        }
        laid
    }

    /// The tear's shapes, as long and wide as a lane.
    fn tear(length: f32, width: f32) -> [FlatMesh; 2] {
        [rubble_shape(length, width, 7), rift_shape(length, width, 7, CORE_THICKNESS)]
    }

    /// Every bit of it lies on what's under it, struck from `pos`: its corners and its middle on
    /// the surface the blade meets there (to within how much a crease, a ridge or a ramp's foot,
    /// is rounded off between the looks), none of it in the air or sunk into anything.
    fn lies_on_what_is_under_it(laid: &[[Vec3; 3]], pos: Vec2, feet: f32, place: &str) {
        let yard = MapId::Yard.map();
        for t in laid {
            for p in [t[0], t[1], t[2], (t[0] + t[1] + t[2]) / 3.0] {
                let under = struck_surface(yard, pos, to_gameplay(p), feet);
                assert!((p.y - under).abs() < 0.03, "{place}: laid at {} m over {} where the surface is {under} m", p.y, to_gameplay(p));
            }
        }
    }

    /// How much ground (square meters, seen from above) triangles cover.
    fn area(laid: &[[Vec3; 3]]) -> f32 {
        laid.iter().map(|t| (t[1].xz() - t[0].xz()).perp_dot(t[2].xz() - t[0].xz()).abs() / 2.0).sum()
    }

    /// As much as the whole tear's shapes cover.
    fn whole() -> f32 {
        let (from, range, width) = ClassId::by_key("revenant").unwrap().def().attack.lane(false).unwrap();
        tear(range - from, width).iter().map(|shape| area(&shape.faded_triangles().map(|(t, _)| t).collect::<Vec<_>>())).sum()
    }

    #[test]
    fn on_level_ground_the_whole_tear_lies_flat_on_it() {
        let pos = Vec2::new(-34.5, -0.5);
        let laid = torn(pos, Vec2::X, 0.0);
        lies_on_what_is_under_it(&laid, pos, 0.0, "level ground");
        assert!((area(&laid) - whole()).abs() < whole() * 0.01, "laid {} m2 of {} m2", area(&laid), whole());
    }

    #[test]
    fn over_a_crate_the_tear_breaks_into_pieces_on_its_top_and_on_the_ground_round_it() {
        // North onto the lowest crate (y 1 to 2) and off its back onto the ground again.
        let yard = MapId::Yard.map();
        let (t, _) = *yard.crates.iter().find(|&&(_, high)| high == 1).unwrap();
        let pos = Vec2::new(Map::center(t).x, -1.0);
        let laid = torn(pos, Vec2::Y, 0.0);
        lies_on_what_is_under_it(&laid, pos, 0.0, "over a crate");
        let on_top = laid.iter().filter(|t| t.iter().all(|p| (p.y - CRATE).abs() < 1e-3)).count();
        let on_ground = laid.iter().filter(|t| t.iter().all(|p| p.y.abs() < 1e-3)).count();
        assert!(on_top > 100 && on_ground > 100, "{on_top} triangles on the crate, {on_ground} on the ground");
        assert_eq!(on_top + on_ground, laid.len(), "some of it between the crate's top and the ground");
    }

    #[test]
    fn off_a_terraces_edge_the_tear_falls_on_down_onto_the_square_below() {
        // North off the south terrace (y = -9), its parapet along the edge, the square below.
        let pos = Vec2::new(2.5, -11.0);
        let laid = torn(pos, Vec2::Y, 1.5);
        lies_on_what_is_under_it(&laid, pos, 1.5, "off the terrace");
        let below = laid.iter().filter(|t| t.iter().all(|p| p.y < 0.05 && to_gameplay(*p).y > -8.95)).count();
        assert!(below > 100, "only {below} triangles on the square past the terrace's edge");
        assert!(area(&laid) > whole() * 0.9, "laid {} m2 of {} m2", area(&laid), whole());
    }

    #[test]
    fn up_a_ramp_the_tear_lies_along_its_slope() {
        let pos = Vec2::new(-9.5, 5.0);
        let laid = torn(pos, Vec2::Y, 0.0);
        lies_on_what_is_under_it(&laid, pos, 0.0, "up a ramp");
        let highest = laid.iter().flatten().map(|p| p.y).fold(0.0, f32::max);
        assert!(highest > 0.5, "the tear only comes up to {highest} m on the ramp");
    }

    #[test]
    fn up_a_roof_the_tear_follows_it_on_up_over_the_ridge_past_the_blades_reach() {
        // From a roof's eaves across its ridge: a house whose ridge runs along y.
        let yard = MapId::Yard.map();
        let house = yard.houses.iter().find(|h| !h.long_x() && h.size().x == 4.0).expect("a house with its ridge along y");
        let g = yard.height_at(house.center());
        let pos = Vec2::new(house.lo.x as f32 - House::EAVES + 0.1, house.center().y - 1.0);
        let feet = yard.floor_at(pos, g + House::WALLS + 0.5);
        assert!(feet > g + House::WALLS - House::EAVES * House::SLOPE, "standing at {pos} on {feet} m, not on the roof");
        let laid = torn(pos, Vec2::X, feet);
        lies_on_what_is_under_it(&laid, pos, feet, "up a roof");
        assert!(laid.iter().flatten().all(|p| p.y > g + House::WALLS), "some of it under the roof");
        let highest = laid.iter().flatten().map(|p| p.y).fold(0.0, f32::max);
        assert!(highest > feet + REACH_UP, "the tear only comes up to {highest} m on a roof stood on at {feet} m");
        assert!(area(&laid) > whole() * 0.95, "laid {} m2 of {} m2", area(&laid), whole());
    }

    #[test]
    fn off_a_high_place_the_tear_falls_all_the_way_onto_the_ground() {
        // West off the watchtower's top, onto the ground far below.
        let pos = Vec2::new(-11.0, -11.0);
        let laid = torn(pos, Vec2::NEG_X, 6.0);
        lies_on_what_is_under_it(&laid, pos, 6.0, "off the tower");
        assert!(laid.iter().flatten().all(|p| p.y < 0.5), "some of it up in the air");
        assert!((area(&laid) - whole()).abs() < whole() * 0.01, "laid {} m2 of {} m2", area(&laid), whole());
    }

    #[test]
    fn the_surface_is_followed_between_looks_and_looked_up_afresh_at_an_edge() {
        let yard = MapId::Yard.map();
        // Across the crate's front edge, point by point, finer than the looks.
        let (t, _) = *yard.crates.iter().find(|&&(_, high)| high == 1).unwrap();
        let pos = Vec2::new(Map::center(t).x, 0.0);
        let mut under = Underfoot::new(yard, pos, Vec2::Y, 0.0);
        for i in 0..200 {
            let local = Vec2::new(0.5 + i as f32 * 0.005, 0.013 * (i % 7) as f32);
            let expected = struck_surface(yard, pos, under.at(local), 0.0);
            assert_eq!(under.height(local), expected, "{local} along the lane");
        }
    }
}
