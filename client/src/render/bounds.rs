//! Each mesh's bounds, worked out once: Bevy works out a new mesh entity's `Aabb` from its
//! vertices, and a fight spawns many of the same (a monster's rig parts, effects' parts), so
//! here a mesh's is kept and given to each entity with it, before Bevy would.

use bevy::camera::primitives::{Aabb, MeshAabb};
use bevy::camera::visibility::{NoAutoAabb, NoFrustumCulling, VisibilitySystems};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

pub(super) fn plugin(app: &mut App) {
    app.add_systems(PostUpdate, give_bounds.before(VisibilitySystems::CalculateBounds));
}

/// What Bevy's `calculate_bounds` would give each new mesh entity, from what each mesh's is (a
/// changed or dropped mesh's worked out again).
#[allow(clippy::type_complexity)]
fn give_bounds(
    mut commands: Commands,
    meshes: Res<Assets<Mesh>>,
    mut events: MessageReader<AssetEvent<Mesh>>,
    mut known: Local<HashMap<AssetId<Mesh>, Aabb>>,
    new: Query<(Entity, &Mesh3d), (Without<Aabb>, Without<NoFrustumCulling>, Without<NoAutoAabb>)>,
) {
    for event in events.read() {
        if let AssetEvent::Modified { id } | AssetEvent::Removed { id } | AssetEvent::Unused { id } = event {
            known.remove(id);
        }
    }
    for (entity, mesh) in &new {
        let aabb = match known.get(&mesh.id()) {
            Some(aabb) => *aabb,
            None => {
                let Some(aabb) = meshes.get(mesh).and_then(MeshAabb::compute_aabb) else { continue };
                *known.entry(mesh.id()).insert(aabb).get()
            }
        };
        commands.entity(entity).try_insert(aabb);
    }
}
