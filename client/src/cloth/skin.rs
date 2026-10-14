//! Skinning a drape to its chains: each vertex following its nearest chain (blended with the
//! nearer of its neighbours, for joined cloth), and the most joints a skin holds.

use super::*;

use bevy::mesh::VertexAttributeValues;

/// The joint of point `k` of chain `c` (joint 0 is the body itself).
pub(super) fn joint(chains: &[Vec<Vec3>], c: usize, k: usize) -> u16 {
    (1 + chains[..c].iter().map(Vec::len).sum::<usize>() + k) as u16
}

/// The nearest point to `v` on `chain`: how far, on which stretch, and how far along it (below 0
/// only above the top).
pub(super) fn nearest(chain: &[Vec3], v: Vec3) -> (f32, usize, f32) {
    let mut best = (f32::INFINITY, 0, 0.0);
    for k in 0..chain.len() - 1 {
        let (a, b) = (chain[k], chain[k + 1]);
        let t = ((v - a).dot(b - a) / (b - a).length_squared()).min(1.0);
        let t = if k == 0 { t } else { t.max(0.0) };
        let d = v.distance(a + (b - a) * t.max(0.0));
        if d < best.0 {
            best = (d, k, t);
        }
    }
    best
}

/// Gives every vertex of `mesh` its joints and weights: along its nearest chain (blended by
/// distance with the nearer of its neighbours, if `joined`) between the joints either side of it,
/// or the body above the top.
pub(super) fn skin(mesh: &mut Mesh, chains: &[Vec<Vec3>], joined: bool) {
    let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else { return };
    let (mut indices, mut weights) = (Vec::with_capacity(positions.len()), Vec::with_capacity(positions.len()));
    for p in positions {
        let v = Vec3::from(*p);
        let all: Vec<(usize, (f32, usize, f32))> = chains.iter().enumerate().map(|(c, chain)| (c, nearest(chain, v))).collect();
        let first = *all.iter().min_by(|a, b| a.1.0.total_cmp(&b.1.0)).expect("a drape has chains");
        // Blended with the nearer of the chains either side of its nearest, not just the next
        // nearest: that may be across a slit (the two sides of a cut, close at rest).
        let mut near = vec![first];
        if joined {
            let beside = [first.0.wrapping_sub(1), first.0 + 1].into_iter().filter_map(|c| all.get(c).copied());
            near.extend(beside.min_by(|a, b| a.1.0.total_cmp(&b.1.0)));
        }
        let total: f32 = near.iter().map(|(_, (d, _, _))| 1.0 / (d * d + 1e-4)).sum();
        let mut influence: Vec<(u16, f32)> = Vec::new();
        let mut add = |j: u16, w: f32| match influence.iter_mut().find(|(i, _)| *i == j) {
            Some((_, sum)) => *sum += w,
            None => influence.push((j, w)),
        };
        for (c, (d, k, t)) in near {
            let share = 1.0 / (d * d + 1e-4) / total;
            if t < 0.0 {
                add(0, share);
            } else {
                add(joint(chains, c, k), share * (1.0 - t));
                add(joint(chains, c, k + 1), share * t);
            }
        }
        influence.resize(4, (0, 0.0));
        indices.push([0, 1, 2, 3].map(|i| influence[i].0));
        weights.push([0, 1, 2, 3].map(|i| influence[i].1));
    }
    mesh.insert_attribute(Mesh::ATTRIBUTE_JOINT_INDEX, VertexAttributeValues::Uint16x4(indices));
    mesh.insert_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, weights);
}

/// The most joints a skinned mesh can have (one for the body, one per chain point): what the web
/// build's skinning holds.
pub(crate) const MAX_JOINTS: usize = 256;
