//! Each mesh's bounds, worked out once: Bevy works out a new mesh entity's `Aabb` from its
//! vertices, and a fight spawns many of the same (a monster's rig parts, effects' parts), so
//! here a mesh's is kept and given to each entity with it, before Bevy would. Natively, the meshes
//! are then culled by them on the GPU alone (`cull_on_gpu`). And no mesh's transform from the frame
//! before is kept.

use bevy::camera::primitives::{Aabb, MeshAabb};
use bevy::camera::visibility::{NoAutoAabb, NoFrustumCulling, VisibilitySystems};
#[cfg(not(target_family = "wasm"))]
use bevy::camera::visibility::{NoCpuCulling, RenderLayers};
use bevy::ecs::schedule::ScheduleCleanupPolicy;
use bevy::pbr::update_mesh_previous_global_transforms;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

pub(super) fn plugin(app: &mut App) {
    app.add_systems(PostUpdate, give_bounds.before(VisibilitySystems::CalculateBounds));
    // Bevy keeps each mesh's transform from the frame before, for motion vectors (motion blur,
    // temporal anti-aliasing), which nothing here draws: a copy of every moving mesh's a frame,
    // which then has it sent to the GPU again the frame after it stops, and an insert on each new
    // one.
    let _ = app.remove_systems_in_set(PreUpdate, update_mesh_previous_global_transforms, ScheduleCleanupPolicy::RemoveSystemsOnly);
    #[cfg(not(target_family = "wasm"))]
    app.add_observer(cull_on_gpu);
}

/// Natively, each mesh is culled on the GPU alone, as it's drawn (`NoCpuCulling`): Bevy then
/// leaves it out of the visibility checks it makes on the CPU for every view every frame, which a
/// crowd's many body parts made a good share of the frame. (WebGL can't cull on the GPU.) Its
/// layer is made the default's outright, as a view with layers of its own (the practice row's
/// faces) takes in what's culled on the GPU and has none.
#[cfg(not(target_family = "wasm"))]
fn cull_on_gpu(trigger: On<Add, Mesh3d>, mut commands: Commands, layered: Query<(), With<RenderLayers>>) {
    if !layered.contains(trigger.entity) {
        commands.entity(trigger.entity).try_insert((NoCpuCulling, RenderLayers::default()));
    }
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
