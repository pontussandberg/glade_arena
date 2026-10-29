//! Flat colored rectangles on screen (their corners rounded or not), drawn with the UI, each run
//! of them in one go just over one of its nodes (`Rects::over`): for what there's a lot of and
//! moves every frame, every fighter's health bar and dot on the minimap. As nodes of their own
//! they made Bevy go over every one of them every frame (laying them out, stacking them,
//! gathering each to draw), which cost more than the rest of the UI put together.

use bevy::asset::{embedded_asset, load_embedded_asset};
use bevy::ecs::query::ROQueryItem;
use bevy::ecs::system::lifetimeless::SRes;
use bevy::ecs::system::SystemParamItem;
use bevy::math::FloatOrd;
use bevy::mesh::VertexBufferLayout;
use bevy::prelude::*;
use bevy::render::render_phase::{AddRenderCommand, DrawFunctions, PhaseItemExtraIndex, RenderCommand, RenderCommandResult, SetItemPipeline, TrackedRenderPass, ViewSortedRenderPhases};
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::sync_world::MainEntity;
use bevy::render::view::ExtractedView;
use bevy::render::{ExtractSchedule, GpuResourceAppExt, MainWorld, Render, RenderApp, RenderStartup, RenderSystems};
use bevy::shader::Shader;
use bevy::ui_render::{SetUiViewBindGroup, TransparentUi, UiCameraView, UiPipeline, init_ui_pipeline};

/// A rectangle as drawn: its left, top, right and bottom (in the camera's pixels, the physical
/// ones, from the top left of its view), then its color (linear, and not multiplied by how
/// opaque it is), then how round its corners are (their radius, in the same pixels; none for
/// square ones, whose edges are as sharp as a node's).
pub type Quad = [f32; 9];

/// This frame's rectangles, in the order they're drawn (later over earlier), and where: each run
/// of them over which node (`over`), on which camera (its render world entity). Laid out anew
/// every frame, by whatever draws them: what isn't is gone.
#[derive(Resource, Default)]
pub struct Rects {
    quads: Vec<Quad>,
    /// Each run's node (its `ComputedStackIndex`), and its first rectangle.
    runs: Vec<(u32, u32)>,
    pub camera: Option<Entity>,
}

impl Rects {
    /// What's pushed from now on is drawn over the node `stack` (its `ComputedStackIndex`): over
    /// its own background, border and picture, under what's in it.
    pub fn over(&mut self, stack: u32) {
        self.runs.push((stack, self.quads.len() as u32));
    }

    /// `color` over `left`..`right`, `top`..`bottom`. A color drawn many times a frame is best
    /// passed linear (a `Color` is made linear at every rectangle).
    pub fn push(&mut self, from: (f32, f32), to: (f32, f32), color: impl Into<LinearRgba>) {
        self.round(from, to, 0.0, color);
    }

    /// `push`, its corners rounded by `radius` (as round as it goes, a circle in a square, at
    /// most).
    pub fn round(&mut self, (left, top): (f32, f32), (right, bottom): (f32, f32), radius: f32, color: impl Into<LinearRgba>) {
        let [r, g, b, a] = color.into().to_f32_array();
        self.quads.push([left, top, right, bottom, r, g, b, a, radius]);
    }
}

pub struct RectsPlugin;

impl Plugin for RectsPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "rects.wgsl");
        app.init_resource::<Rects>();
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
        render_app
            .init_resource::<RectsBuffer>()
            .init_gpu_resource::<SpecializedRenderPipelines<RectsPipeline>>()
            .add_render_command::<TransparentUi, DrawRects>()
            .add_systems(RenderStartup, init_pipeline.after(init_ui_pipeline))
            .add_systems(ExtractSchedule, take_rects)
            .add_systems(Render, (write_buffer.in_set(RenderSystems::PrepareResources), queue_rects.in_set(RenderSystems::Queue)));
    }
}

/// The rectangles on the GPU, kept from frame to frame, and where they're drawn: each run's node
/// and its rectangles.
#[derive(Resource)]
struct RectsBuffer {
    quads: RawBufferVec<Quad>,
    runs: Vec<(u32, std::ops::Range<u32>)>,
    camera: Option<Entity>,
}

impl Default for RectsBuffer {
    fn default() -> Self {
        RectsBuffer { quads: RawBufferVec::new(BufferUsages::VERTEX), runs: Vec::new(), camera: None }
    }
}

#[derive(Resource)]
struct RectsPipeline {
    view_layout: BindGroupLayoutDescriptor,
    shader: Handle<Shader>,
    /// What the draw is filed under in the render world (it's no one node's).
    entity: Entity,
}

fn init_pipeline(mut commands: Commands, ui_pipeline: Res<UiPipeline>, asset_server: Res<AssetServer>) {
    let entity = commands.spawn_empty().id();
    commands.insert_resource(RectsPipeline { view_layout: ui_pipeline.view_layout.clone(), shader: load_embedded_asset!(asset_server.as_ref(), "rects.wgsl"), entity });
}

/// Drawn as the UI draws, onto a target of the `key` format: each rectangle two triangles,
/// blended over what's under it.
impl SpecializedRenderPipeline for RectsPipeline {
    type Key = TextureFormat;

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        RenderPipelineDescriptor {
            label: Some("rects".into()),
            layout: vec![self.view_layout.clone()],
            vertex: VertexState {
                shader: self.shader.clone(),
                entry_point: Some("vertex".into()),
                buffers: vec![VertexBufferLayout::from_vertex_formats(VertexStepMode::Instance, [VertexFormat::Float32x4, VertexFormat::Float32x4, VertexFormat::Float32])],
                ..default()
            },
            fragment: Some(FragmentState {
                shader: self.shader.clone(),
                entry_point: Some("fragment".into()),
                targets: vec![Some(ColorTargetState { format: key, blend: Some(BlendState::ALPHA_BLENDING), write_mask: ColorWrites::ALL })],
                ..default()
            }),
            ..default()
        }
    }
}

/// The frame's rectangles, swapped for the ones already drawn, which are cleared for the next
/// frame's: nothing's copied, nor made anew.
fn take_rects(mut main: ResMut<MainWorld>, mut buffer: ResMut<RectsBuffer>) {
    let Some(mut rects) = main.get_resource_mut::<Rects>() else { return };
    std::mem::swap(buffer.quads.values_mut(), &mut rects.quads);
    rects.quads.clear();
    let end = buffer.quads.len() as u32;
    let starts = rects.runs.iter().map(|&(_, start)| start).skip(1).chain([end]);
    buffer.runs = rects.runs.iter().zip(starts).map(|(&(over, start), end)| (over, start..end)).filter(|(_, run)| !run.is_empty()).collect();
    rects.runs.clear();
    buffer.camera = rects.camera;
}

fn write_buffer(mut buffer: ResMut<RectsBuffer>, device: Res<RenderDevice>, queue: Res<RenderQueue>) {
    // Room to spare, so a crowd coming into view doesn't make a new buffer every frame.
    let room = buffer.quads.len().next_power_of_two();
    buffer.quads.reserve(room, &device);
    buffer.quads.write_buffer(&device, &queue);
}

#[allow(clippy::too_many_arguments)]
fn queue_rects(
    draw_functions: Res<DrawFunctions<TransparentUi>>,
    pipeline: Res<RectsPipeline>,
    mut pipelines: ResMut<SpecializedRenderPipelines<RectsPipeline>>,
    pipeline_cache: Res<PipelineCache>,
    buffer: Res<RectsBuffer>,
    mut phases: ResMut<ViewSortedRenderPhases<TransparentUi>>,
    cameras: Query<&UiCameraView>,
    views: Query<&ExtractedView>,
) {
    let Some(camera) = buffer.camera else { return };
    if buffer.runs.is_empty() {
        return;
    }
    let Some(draw_function) = draw_functions.read().get_id::<DrawRects>() else { return };
    let Some(view) = cameras.get(camera).ok().and_then(|ui| views.get(ui.0).ok()) else { return };
    let Some(phase) = phases.get_mut(&view.retained_view_entity) else { return };
    let pipeline_id = pipelines.specialize(&pipeline_cache, &pipeline, view.target_format);
    for (run, (over, _)) in buffer.runs.iter().enumerate() {
        phase.add_transient(TransparentUi {
            // Over the node's own background, border and all, and under the first thing in it.
            sort_key: FloatOrd(*over as f32 + 0.4),
            // A key of its own (the phase keeps one item a key): no main world entity's, but
            // the run's.
            entity: (pipeline.entity, MainEntity::from(Entity::from_raw_u32(run as u32).unwrap_or(Entity::PLACEHOLDER))),
            pipeline: pipeline_id,
            draw_function,
            // Just this (the phase goes on past as many items as it says).
            batch_range: 0..1,
            extra_index: PhaseItemExtraIndex::None,
            // Which run it is, counted down from the top: no node of the UI's own (it looks its
            // nodes up by this), so it doesn't batch this with any.
            index: usize::MAX - run,
            indexed: false,
        });
    }
}

type DrawRects = (SetItemPipeline, SetUiViewBindGroup<0>, DrawQuads);

struct DrawQuads;

impl RenderCommand<TransparentUi> for DrawQuads {
    type Param = SRes<RectsBuffer>;
    type ViewQuery = ();
    type ItemQuery = ();

    fn render<'w>(
        item: &TransparentUi,
        _view: ROQueryItem<'w, '_, Self::ViewQuery>,
        _entity: Option<ROQueryItem<'w, '_, Self::ItemQuery>>,
        buffer: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let buffer = buffer.into_inner();
        let Some(quads) = buffer.quads.buffer() else { return RenderCommandResult::Skip };
        let Some((_, run)) = buffer.runs.get(usize::MAX - item.index) else { return RenderCommandResult::Skip };
        pass.set_vertex_buffer(0, quads.slice(..));
        pass.draw(0..6, run.clone());
        RenderCommandResult::Success
    }
}
