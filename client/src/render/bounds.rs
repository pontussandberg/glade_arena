//! Each mesh's bounds, worked out once: Bevy works out a new mesh entity's `Aabb` from its
//! vertices, and a fight spawns many of the same (a monster's rig parts), so here a mesh's is kept
//! and given to each entity with it, in Bevy's stead (`NoAutoAabb`). Natively, the meshes are then
//! culled by them on the GPU alone (`cull_on_gpu`). And no mesh's transform from the frame before
//! is kept.

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

/// On a mesh entity whose `Aabb` comes from `give_bounds` (and so not from Bevy, `NoAutoAabb`).
#[derive(Component)]
struct GivenBounds;

/// What Bevy's `calculate_bounds` would give each mesh entity, from what each mesh's is (a changed
/// or dropped mesh's worked out again): to a new one, and again to one given another mesh or whose
/// mesh changed. Bevy is told to leave them be (`NoAutoAabb`), or it would work each new one's out
/// from its vertices all the same. A mesh changed is seen a frame on, as the frame's asset news
/// comes after this: waiting for it would hold up the visibility checks behind it.
#[allow(clippy::type_complexity)]
fn give_bounds(
    mut commands: Commands,
    meshes: Res<Assets<Mesh>>,
    mut events: MessageReader<AssetEvent<Mesh>>,
    mut known: Local<HashMap<AssetId<Mesh>, Aabb>>,
    new: Query<(Entity, &Mesh3d), (Without<Aabb>, Without<NoFrustumCulling>, Without<NoAutoAabb>)>,
    mut changed: Query<(&Mesh3d, &mut Aabb), (With<GivenBounds>, Or<(AssetChanged<Mesh3d>, Changed<Mesh3d>)>)>,
) {
    for event in events.read() {
        if let AssetEvent::Modified { id } | AssetEvent::Removed { id } | AssetEvent::Unused { id } = event {
            known.remove(id);
        }
    }
    let mut bounds = |mesh: &Mesh3d| match known.get(&mesh.id()) {
        Some(aabb) => Some(*aabb),
        None => {
            let aabb = meshes.get(mesh).and_then(MeshAabb::compute_aabb)?;
            known.insert(mesh.id(), aabb);
            Some(aabb)
        }
    };
    for (mesh, mut aabb) in &mut changed {
        if let Some(bounds) = bounds(mesh) {
            aabb.set_if_neq(bounds);
        }
    }
    for (entity, mesh) in &new {
        if let Some(aabb) = bounds(mesh) {
            commands.entity(entity).try_insert((aabb, NoAutoAabb, GivenBounds));
        }
    }
}
