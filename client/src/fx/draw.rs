//! The effects' parts on screen: the corners and triangles `draw_parts` lays out every frame go
//! straight into the same buffers on the GPU, frame after frame, and are drawn in one go among
//! the see-through things. Made over as a mesh every frame instead, Bevy found it room and
//! copied it in anew each time, which cost more than drawing it.

use bevy::asset::{embedded_asset, load_embedded_asset};
use bevy::camera::visibility::RenderLayers;
use bevy::core_pipeline::core_3d::{Transparent3d, TransparentSortingInfo3d, CORE_3D_DEPTH_FORMAT};
use bevy::ecs::query::ROQueryItem;
use bevy::ecs::system::lifetimeless::SRes;
use bevy::ecs::system::SystemParamItem;
use bevy::mesh::VertexBufferLayout;
use bevy::pbr::{MeshPipeline, MeshPipelineKey, MeshPipelineSystems, SetMeshViewBindGroup, ViewKeyCache};
use bevy::prelude::*;
use bevy::render::render_phase::{AddRenderCommand, DrawFunctions, PhaseItem, PhaseItemExtraIndex, RenderCommand, RenderCommandResult, SetItemPipeline, TrackedRenderPass, ViewSortedRenderPhases};
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::sync_world::MainEntity;
use bevy::render::view::ExtractedView;
use bevy::render::{ExtractSchedule, GpuResourceAppExt, MainWorld, Render, RenderApp, RenderStartup, RenderSystems};
use bevy::shader::Shader;

/// A corner as drawn: where it is in the world, then its color (linear, and not multiplied by
/// how opaque it is).
pub(super) type Corner = [f32; 7];

/// What `draw_parts` laid out this frame, taken by the render world as it is (`take_drawn`).
#[derive(Resource, Default)]
pub(crate) struct Drawn {
    pub(super) corners: Vec<Corner>,
    pub(super) triangles: Vec<u32>,
    /// About where they are: they're sorted among the other see-through things as one, from here.
    pub(super) center: Vec3,
}

impl Drawn {
    /// How many corners there are to draw this frame (for the perf mode).
    pub(crate) fn corners(&self) -> usize {
        self.corners.len()
    }
}

/// How much of the haze lies over what's `distance` from the eye, as Bevy's shaders work it out
/// for everything else (the scene's haze falls off linearly; the other kinds aren't worked in).
pub(super) fn haze(fog: &DistanceFog, distance: f32) -> f32 {
    let FogFalloff::Linear { start, end } = fog.falloff else { return 0.0 };
    fog.color.alpha() * (1.0 - ((end - distance) / (end - start)).clamp(0.0, 1.0))
}

pub(super) fn plugin(app: &mut App) {
    embedded_asset!(app, "parts.wgsl");
    app.init_resource::<Drawn>();
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
    render_app
        .init_resource::<Buffers>()
        .init_gpu_resource::<SpecializedRenderPipelines<PartsPipeline>>()
        .add_render_command::<Transparent3d, DrawParts>()
        .add_systems(RenderStartup, init_pipeline.after(MeshPipelineSystems))
        .add_systems(ExtractSchedule, take_drawn)
        .add_systems(Render, (write_buffers.in_set(RenderSystems::PrepareResources), queue_parts.in_set(RenderSystems::Queue)));
}

/// The parts' corners and triangles on the GPU, kept from frame to frame.
#[derive(Resource)]
struct Buffers {
    corners: RawBufferVec<Corner>,
    triangles: RawBufferVec<u32>,
    center: Vec3,
}

impl Default for Buffers {
    fn default() -> Self {
        Buffers { corners: RawBufferVec::new(BufferUsages::VERTEX), triangles: RawBufferVec::new(BufferUsages::INDEX), center: Vec3::ZERO }
    }
}

#[derive(Resource)]
struct PartsPipeline {
    mesh_pipeline: MeshPipeline,
    shader: Handle<Shader>,
    /// What the draw is filed under in the render world (it's no one thing's).
    entity: Entity,
}

fn init_pipeline(mut commands: Commands, mesh_pipeline: Res<MeshPipeline>, asset_server: Res<AssetServer>) {
    let entity = commands.spawn_empty().id();
    commands.insert_resource(PartsPipeline { mesh_pipeline: mesh_pipeline.clone(), shader: load_embedded_asset!(asset_server.as_ref(), "parts.wgsl"), entity });
}

/// Drawn as Bevy draws a see-through, unlit, two-sided material for the view (`key`): blended
/// over what's behind, behind what's nearer, and leaving the depth as it was.
impl SpecializedRenderPipeline for PartsPipeline {
    type Key = MeshPipelineKey;

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        RenderPipelineDescriptor {
            label: Some("effect parts".into()),
            layout: vec![self.mesh_pipeline.get_view_layout(key.into()).main_layout.clone()],
            vertex: VertexState {
                shader: self.shader.clone(),
                entry_point: Some("vertex".into()),
                buffers: vec![VertexBufferLayout::from_vertex_formats(VertexStepMode::Vertex, [VertexFormat::Float32x3, VertexFormat::Float32x4])],
                ..default()
            },
            fragment: Some(FragmentState {
                shader: self.shader.clone(),
                entry_point: Some("fragment".into()),
                targets: vec![Some(ColorTargetState { format: key.target_format(), blend: Some(BlendState::ALPHA_BLENDING), write_mask: ColorWrites::ALL })],
                ..default()
            }),
            primitive: PrimitiveState { cull_mode: None, ..default() },
            depth_stencil: Some(DepthStencilState {
                format: CORE_3D_DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(CompareFunction::GreaterEqual),
                stencil: StencilState::default(),
                bias: DepthBiasState::default(),
            }),
            multisample: MultisampleState { count: key.msaa_samples(), ..default() },
            ..default()
        }
    }
}

/// The frame's corners and triangles, swapped for the ones already drawn, which are cleared for
/// `draw_parts` to fill again (nothing's copied, nor made anew): a frame it doesn't run, none
/// are drawn.
fn take_drawn(mut main: ResMut<MainWorld>, mut buffers: ResMut<Buffers>) {
    let Some(mut drawn) = main.get_resource_mut::<Drawn>() else { return };
    std::mem::swap(buffers.corners.values_mut(), &mut drawn.corners);
    std::mem::swap(buffers.triangles.values_mut(), &mut drawn.triangles);
    drawn.corners.clear();
    drawn.triangles.clear();
    buffers.center = drawn.center;
}

fn write_buffers(mut buffers: ResMut<Buffers>, device: Res<RenderDevice>, queue: Res<RenderQueue>) {
    let Buffers { corners, triangles, .. } = &mut *buffers;
    // Room to spare, so a fight growing doesn't make new buffers every frame.
    corners.reserve(corners.len().next_power_of_two(), &device);
    triangles.reserve(triangles.len().next_power_of_two(), &device);
    corners.write_buffer(&device, &queue);
    triangles.write_buffer(&device, &queue);
}

#[allow(clippy::too_many_arguments)]
fn queue_parts(
    draw_functions: Res<DrawFunctions<Transparent3d>>,
    pipeline: Res<PartsPipeline>,
    mut pipelines: ResMut<SpecializedRenderPipelines<PartsPipeline>>,
    pipeline_cache: Res<PipelineCache>,
    buffers: Res<Buffers>,
    mut phases: ResMut<ViewSortedRenderPhases<Transparent3d>>,
    views: Query<(&ExtractedView, Option<&RenderLayers>)>,
    view_keys: Res<ViewKeyCache>,
) {
    let Some(draw_function) = draw_functions.read().get_id::<DrawParts>() else { return };
    if buffers.triangles.is_empty() {
        return;
    }
    for (view, layers) in &views {
        let (Some(phase), Some(&key)) = (phases.get_mut(&view.retained_view_entity), view_keys.get(&view.retained_view_entity)) else { continue };
        if !layers.unwrap_or_default().intersects(&RenderLayers::default()) {
            continue;
        }
        phase.add_transient(Transparent3d {
            sorting_info: TransparentSortingInfo3d::Sorted { mesh_center: buffers.center, depth_bias: 0.0 },
            distance: 0.0,
            pipeline: pipelines.specialize(&pipeline_cache, &pipeline, key),
            entity: (pipeline.entity, MainEntity::from(Entity::PLACEHOLDER)),
            draw_function,
            batch_range: 0..1,
            extra_index: PhaseItemExtraIndex::None,
            indexed: true,
        });
    }
}

type DrawParts = (SetItemPipeline, SetMeshViewBindGroup<0>, DrawCorners);

struct DrawCorners;

impl<P: PhaseItem> RenderCommand<P> for DrawCorners {
    type Param = SRes<Buffers>;
    type ViewQuery = ();
    type ItemQuery = ();

    fn render<'w>(
        _item: &P,
        _view: ROQueryItem<'w, '_, Self::ViewQuery>,
        _entity: Option<ROQueryItem<'w, '_, Self::ItemQuery>>,
        buffers: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let buffers = buffers.into_inner();
        let (Some(corners), Some(triangles)) = (buffers.corners.buffer(), buffers.triangles.buffer()) else { return RenderCommandResult::Skip };
        pass.set_vertex_buffer(0, corners.slice(..));
        pass.set_index_buffer(triangles.slice(..), IndexFormat::Uint32);
        pass.draw_indexed(0..buffers.triangles.len() as u32, 0, 0..1);
        RenderCommandResult::Success
    }
}
