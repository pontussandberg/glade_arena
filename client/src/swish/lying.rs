//! Marks laid on the ground under a lane by the GPU (`lying.wgsl`), with nothing made anew on the
//! GPU for any of them: each kind of mark is drawn on a grid made once (`lying_grid`), the same
//! wherever it lies, by one material (`LyingMaterial`) that says how opaque it is over its grid:
//! from a texture made once (a tear's) or as a shape the shader knows (a ring, a line); what's
//! under a mark's lane is looked up once, into a slot of one texture of heights kept on the GPU
//! (`LyingHeights`). As it plays out (opening, stretching, its cracks closing, fading), its
//! entity's transform says how it's stretched, its tag which slot it lies on, which tile of its
//! texture it's drawn from, how far it's closed and faded (`lying_tag`), and the shader lays each
//! point on what's under it there, dropping what of it comes off the surface (down a step, off an
//! edge). In a browser all that's made on the GPU goes through it, one call at a time: a strike
//! making its own meshes, texture and materials cost more than all the rest.

use bevy::asset::{RenderAssetUsages, embedded_asset, embedded_path, AssetPath};
use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::world::DeferredWorld;
use bevy::mesh::{Indices, MeshTag, MeshVertexBufferLayoutRef, PrimitiveTopology};
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey, MaterialPlugin};
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::{
    AsBindGroup, Extent3d, Origin3d, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError, TexelCopyBufferLayout, TexelCopyTextureInfo, TextureAspect, TextureDimension, TextureFormat,
};
use bevy::render::renderer::RenderQueue;
use bevy::render::texture::GpuImage;
use bevy::render::{ExtractSchedule, MainWorld, Render, RenderApp, RenderSystems};
use bevy::shader::ShaderRef;

use std::f32::consts::SQRT_2;

use arena_shared::map::CLIMB;

use super::*;
use super::lay::LOOK_STEP;

pub(crate) struct LyingPlugin;

impl Plugin for LyingPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "lying.wgsl");
        app.add_plugins(MaterialPlugin::<LyingMaterial>::default());
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
        render_app
            .init_resource::<HeightsWrites>()
            .add_systems(ExtractSchedule, take_heights)
            .add_systems(Render, write_heights.in_set(RenderSystems::PrepareResources));
    }
}

/// How far (meters) a point of a mark may be drawn off what's under it (where the surface steps
/// or drops between the points it's laid at) before it's dropped.
const OFF_SURFACE: f32 = 0.02;

/// How many looks (along a lane, across it) a slot of the heights holds (`LyingHeights`), past a
/// row ahead of them saying where they are (`LyingHeights::keep`): enough for the biggest tear;
/// and how many slots there are (along the texture, down it): enough for every mark lying at once
/// in a big fight, as many as a tag can say (`lying_tag`).
const SLOT: UVec2 = UVec2::new(200, 121);
const SLOTS: UVec2 = UVec2::new(8, 8);
const _: () = assert!(SLOTS.x * SLOTS.y <= 64, "a tag says which slot in six bits");

/// The heights of what a mark lies on under a lane (`Underfoot::drawn`) at its looks
/// (`LOOK_STEP` apart, from look `first`, `size` of them along and across it), as the shader has
/// them: in between, smoothly where the looks round a point are all on one surface, and the
/// nearest look's where they aren't (`surface`).
pub(crate) struct Heights {
    first: IVec2,
    size: IVec2,
    at: Vec<f32>,
}

#[cfg(test)]
impl Heights {
    /// How high the look `k` is (the nearest one there is, past the edge).
    fn get(&self, k: IVec2) -> f32 {
        let k = (k - self.first).clamp(IVec2::ZERO, self.size - 1);
        self.at[(k.y * self.size.x + k.x) as usize]
    }

    /// How high what's under the lane's point `local` is, just as the shader works it out.
    fn surface(&self, local: Vec2) -> f32 {
        let g = local / LOOK_STEP;
        let k = g.floor().as_ivec2();
        let f = g - g.floor();
        let [a, b, c, d] = [k, k + IVec2::X, k + IVec2::Y, k + IVec2::ONE].map(|corner| self.get(corner));
        if a.max(b).max(c).max(d) - a.min(b).min(c).min(d) <= SMOOTH {
            a.lerp(b, f.x).lerp(c.lerp(d, f.x), f.y)
        } else {
            self.get((g + 0.5).floor().as_ivec2())
        }
    }
}

/// How far apart (meters) the four looks round a point may be, for them all to be on one surface
/// (level or sloping, no steeper than a climb).
const SMOOTH: f32 = CLIMB * LOOK_STEP * SQRT_2 + 1e-3;

impl Underfoot {
    /// The heights of what a mark lies on from the lane's point `lo` to `hi` (a box), as many of
    /// them as a slot holds (`SLOT`; past those, the shader takes the last).
    pub(crate) fn heights(&mut self, lo: Vec2, hi: Vec2) -> Heights {
        let first = (lo / LOOK_STEP).floor().as_ivec2();
        let last = (hi / LOOK_STEP).ceil().as_ivec2().min(first + IVec2::new(SLOT.x as i32, SLOT.y as i32 - 1) - 1);
        let at = (first.y..=last.y).flat_map(|y| self.row(y, first.x, last.x)).collect();
        Heights { first, size: last - first + 1, at }
    }
}

/// The heights under every lying mark, on the GPU in one texture, a slot each: those to be
/// written to it this frame (`write_heights`), and the slots no mark lies on.
#[derive(Resource)]
pub(crate) struct LyingHeights {
    image: Handle<Image>,
    free: Vec<u32>,
    /// Each slot written, with how wide (in looks) and what (`R32Float` texels, a row at a time).
    writes: Vec<(u32, u32, Vec<u8>)>,
}

impl LyingHeights {
    pub(crate) fn new(images: &mut Assets<Image>) -> Self {
        let size = Extent3d { width: SLOT.x * SLOTS.x, height: SLOT.y * SLOTS.y, depth_or_array_layers: 1 };
        let image = images.add(Image::new_fill(size, TextureDimension::D2, &0.0f32.to_le_bytes(), TextureFormat::R32Float, RenderAssetUsages::RENDER_WORLD));
        LyingHeights { image, free: (0..SLOTS.x * SLOTS.y).rev().collect(), writes: Vec::new() }
    }

    /// Keeps `heights` in a free slot (none if none is), for a mark stretched from `near` along
    /// its lane, or from `middle` (`LyingMaterial::new`): ahead of them, where they are and how
    /// many there are, and where it's stretched from. The slot's free again once whatever holds
    /// it is gone.
    pub(crate) fn keep(&mut self, heights: &Heights, near: f32, middle: f32) -> Option<HeightsSlot> {
        let slot = self.free.pop()?;
        let width = (heights.size.x as usize).max(6);
        let first = heights.first.as_vec2() * LOOK_STEP;
        let mut texels = vec![0.0; width * (heights.size.y as usize + 1)];
        texels[..6].copy_from_slice(&[first.x, first.y, heights.size.x as f32, heights.size.y as f32, near, middle]);
        for (row, looks) in heights.at.chunks_exact(heights.size.x as usize).enumerate() {
            texels[(row + 1) * width..][..looks.len()].copy_from_slice(looks);
        }
        self.writes.push((slot, width as u32, texels.iter().flat_map(|h| h.to_le_bytes()).collect()));
        Some(HeightsSlot(slot))
    }
}

/// The slot of the heights a mark lies on (`LyingHeights::keep`), free again once it's gone.
#[derive(Component)]
#[component(on_remove = free_slot)]
pub(crate) struct HeightsSlot(pub(crate) u32);

fn free_slot(mut world: DeferredWorld, context: HookContext) {
    let Some(&HeightsSlot(slot)) = world.get::<HeightsSlot>(context.entity) else { return };
    if let Some(mut heights) = world.get_resource_mut::<LyingHeights>() {
        heights.free.push(slot);
    }
}

/// The heights to be written to their slots, as the render world has them.
#[derive(Resource, Default)]
struct HeightsWrites {
    image: AssetId<Image>,
    writes: Vec<(u32, u32, Vec<u8>)>,
}

fn take_heights(mut main: ResMut<MainWorld>, mut writes: ResMut<HeightsWrites>) {
    let Some(mut heights) = main.get_resource_mut::<LyingHeights>() else { return };
    writes.image = heights.image.id();
    writes.writes.append(&mut heights.writes);
}

/// Writes each slot's heights into the texture, in place (kept till it's on the GPU).
fn write_heights(mut writes: ResMut<HeightsWrites>, images: Res<RenderAssets<GpuImage>>, queue: Res<RenderQueue>) {
    let Some(image) = images.get(writes.image) else { return };
    for (slot, width, texels) in writes.writes.drain(..) {
        let at = UVec2::new(slot % SLOTS.x, slot / SLOTS.x) * SLOT;
        let height = texels.len() as u32 / (width * 4);
        queue.write_texture(
            TexelCopyTextureInfo { texture: &image.texture, mip_level: 0, origin: Origin3d { x: at.x, y: at.y, z: 0 }, aspect: TextureAspect::All },
            &texels,
            TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(width * 4), rows_per_image: None },
            Extent3d { width, height, depth_or_array_layers: 1 },
        );
    }
}

/// A grid over the lane's plane from `lo` to `hi` (along it, across it), `cells` of it each way,
/// for a mark to be drawn on (`LyingMaterial`), as finely as it takes to follow what it lies on
/// however it's stretched; and the same standing up from it (across it, upward) if `upright`, for
/// a mark that shows from the side. Its UVs run from 0 to 1 over it, along and across.
pub(crate) fn lying_grid(lo: Vec2, hi: Vec2, cells: UVec2, upright: bool) -> Mesh {
    let (mut positions, mut uvs, mut indices) = (Vec::new(), Vec::new(), Vec::new());
    for stands in [false, true].into_iter().take(1 + upright as usize) {
        let first = positions.len() as u32;
        for y in 0..=cells.y {
            for x in 0..=cells.x {
                let uv = UVec2::new(x, y).as_vec2() / cells.as_vec2();
                let at = lo + (hi - lo) * uv;
                positions.push(if stands { [at.x, at.y, 0.0] } else { [at.x, 0.0, at.y] });
                uvs.push(uv.to_array());
            }
        }
        let corner = |x: u32, y: u32| first + y * (cells.x + 1) + x;
        for y in 0..cells.y {
            for x in 0..cells.x {
                indices.extend([corner(x, y), corner(x, y + 1), corner(x + 1, y), corner(x + 1, y), corner(x, y + 1), corner(x + 1, y + 1)]);
            }
        }
    }
    let indices = match u16::try_from(positions.len()) {
        Ok(_) => Indices::U16(indices.into_iter().map(|k| k as u16).collect()),
        Err(_) => Indices::U32(indices),
    };
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(indices)
}

/// Where a mark lying on a lane is drawn from: the lane's `frame` (`Underfoot::frame`), scaled
/// by how the mark is stretched (the shader stretches it from where it lies, and lays it on
/// what's under it there).
pub(crate) fn lying_at(frame: Transform, stretch: Vec3) -> Transform {
    frame.with_scale(stretch.max(Vec3::splat(1e-4)))
}

/// A lying mark's tag (`MeshTag`), as the shader reads it: the slot of the heights it lies on
/// (`HeightsSlot`), which tile of its texture it's drawn from, whether it's mirrored across its
/// lane, how far its cracks are closed onto their spines (their thickness: 1 as shaped, 0 shut)
/// and how opaque it is (0 to 1).
pub(crate) fn lying_tag((slot, tile, mirrored): (&HeightsSlot, u32, bool), thickness: f32, opaque: f32) -> MeshTag {
    let thickness = ((thickness * 256.0).round() as u32).min(0x3ff);
    MeshTag(((mirrored as u32) << 31) | (slot.0 << 23) | ((tile & 7) << 20) | (thickness << 10) | (opaque.clamp(0.0, 1.0) * 1_023.0).round() as u32)
}

/// How a cracked look's texture says how far each point is from its crack's spine
/// (`LyingShape::Closing`): in the crack's widths there, as far as this, past which it's no crack.
pub(crate) const CLOSING_REACH: f32 = 2.0;

/// How opaque a lying mark is over its grid (`lying_grid`, `lying.wgsl`).
#[derive(Clone, Copy)]
pub(crate) enum LyingShape {
    /// As a channel of its texture says.
    Channel(usize),
    /// Cracks closing onto their spines, as thick as its tag says: a channel of its texture
    /// saying how far each point is from its crack's spine (`CLOSING_REACH`).
    Closing(usize),
    /// A ring 1 across, soft inside, clear at its edge.
    Ring,
    /// A thin line along the grid, tapering to a needle point at its far end and fading out at
    /// its near one.
    Line,
}

/// A see-through, self-lit kind of mark lying on what's under a lane (`lying.wgsl`), wherever it
/// lies: `color`, `lift` over what it lies on, stretched from the near end of its lane or its
/// middle (as its slot of `LyingHeights` says), as opaque over its grid as its `LyingShape` says
/// (from the tile of its texture `look` its tag says); drawn over the see-through layers below it
/// in the same place (a higher `depth_bias`), as `PlainMaterial`'s are.
#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
#[bind_group_data(LyingKey)]
pub(crate) struct LyingMaterial {
    #[uniform(0)]
    lying: LyingUniform,
    #[texture(1, sample_type = "float", filterable = false)]
    heights: Handle<Image>,
    #[texture(2)]
    #[sampler(3)]
    look: Handle<Image>,
    depth_bias: f32,
}

/// What the shader reads (`lying.wgsl`).
#[derive(Clone, Debug, Default, ShaderType)]
struct LyingUniform {
    color: Vec4,
    /// Which channel of its texture it's drawn from (one of them 1).
    pick: Vec4,
    lift: f32,
    /// 1 if it's stretched from its lane's middle, 0 from its near end.
    from_middle: f32,
    /// How far apart the looks are, and the slots (`SLOT`, `SLOTS`).
    look: f32,
    one_surface: f32,
    slot: UVec2,
    /// How many tiles its texture has (along it, down it).
    tiles: UVec2,
    slots_along: u32,
    off_surface: f32,
    /// `StandardMaterial`'s flags, as `PlainMaterial`'s.
    flags: u32,
    /// How it's drawn (`LyingShape`, in order), and `CLOSING_REACH`.
    drawn: u32,
    closing_reach: f32,
}

/// `StandardMaterial`'s flags (`bevy_pbr::pbr_types`): unlit, in the haze, blended.
const FLAGS: u32 = (1 << 5) | (1 << 8) | (2 << 29);

impl LyingMaterial {
    /// See-through `color` (its alpha), `lift` over what it lies on, stretched from its lane's
    /// middle or (`from_middle` false) its near end, on the `heights`, as opaque as it's `drawn`
    /// from its texture `look` (`tiles` of it, along and down), drawn over what's below it by
    /// `depth_bias`.
    pub(crate) fn new(color: Color, lift: f32, from_middle: bool, heights: &LyingHeights, (look, tiles, drawn): (&Handle<Image>, UVec2, LyingShape), depth_bias: f32) -> Self {
        let (kind, channel) = match drawn {
            LyingShape::Channel(channel) => (0, channel),
            LyingShape::Closing(channel) => (1, channel),
            LyingShape::Ring => (2, 0),
            LyingShape::Line => (3, 0),
        };
        let lying = LyingUniform {
            color: LinearRgba::from(color).to_vec4(),
            pick: Vec4::from_array(std::array::from_fn(|k| (k == channel) as u32 as f32)),
            lift,
            from_middle: from_middle as u32 as f32,
            look: LOOK_STEP,
            one_surface: SMOOTH,
            slot: SLOT,
            tiles,
            slots_along: SLOTS.x,
            off_surface: OFF_SURFACE,
            flags: FLAGS,
            drawn: kind,
            closing_reach: CLOSING_REACH,
        };
        LyingMaterial { lying, heights: heights.image.clone(), look: look.clone(), depth_bias }
    }
}

/// What a lying material's pipeline is made for: its depth bias.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct LyingKey {
    depth_bias: i32,
}

impl From<&LyingMaterial> for LyingKey {
    fn from(material: &LyingMaterial) -> Self {
        LyingKey { depth_bias: material.depth_bias as i32 }
    }
}

impl Material for LyingMaterial {
    fn vertex_shader() -> ShaderRef {
        ShaderRef::Path(AssetPath::from_path_buf(embedded_path!("lying.wgsl")).with_source("embedded"))
    }

    fn fragment_shader() -> ShaderRef {
        Self::vertex_shader()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }

    fn depth_bias(&self) -> f32 {
        self.depth_bias
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.vertex.buffers = vec![layout.0.get_layout(&[Mesh::ATTRIBUTE_POSITION.at_shader_location(0), Mesh::ATTRIBUTE_UV_0.at_shader_location(2)])?];
        if let Some(depth_stencil) = descriptor.depth_stencil.as_mut() {
            depth_stencil.bias.constant = key.bind_group_data.depth_bias;
        }
        // Both faces: a mark that stands up from the lane needn't be made twice, and one mirrored
        // across it (`lying_tag`) faces down.
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arena_shared::map::MapId;
    use arena_shared::protocol::ClassId;

    use arena_shared::items::{MAX_LEVEL, gravecaller_rift};

    use crate::swish::{CORE_THICKNESS, FLARE_WIDE, INK_THICKNESS, MARK_HALF_WIDTH, TEAR_BOX, TEAR_CELLS, TEAR_TILE, TEAR_TILES, tear_look, tear_reach};

    /// A mark's grid as the shader lays it (`lying.wgsl`), stretched by `stretch` from `from`:
    /// its triangles in the lane's frame (along, up, across), `lift` over what they lie on, each
    /// corner with where on the grid it is.
    fn laid(mesh: &Mesh, heights: &Heights, from: Vec3, lift: f32, stretch: Vec3) -> Vec<[(Vec3, Vec2); 3]> {
        let Some(positions) = mesh.attribute(Mesh::ATTRIBUTE_POSITION).and_then(|a| a.as_float3()) else { panic!("no positions") };
        let Some(bevy::mesh::VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0) else { panic!("no uvs") };
        let placed: Vec<(Vec3, Vec2)> = positions.iter().zip(uvs).map(|(p, uv)| {
            let at = from + stretch * Vec3::from_array(*p);
            (Vec3::new(at.x, heights.surface(at.xz()) + lift + at.y, at.z), Vec2::from_array(*uv))
        }).collect();
        let indices: Vec<usize> = mesh.indices().expect("no indices").iter().collect();
        indices.as_chunks::<3>().0.iter().map(|t| t.map(|k| placed[k])).collect()
    }

    /// Points spread over each triangle, where on the grid each is, and how much ground each
    /// stands for (square meters, seen from above).
    fn spread(laid: &[[(Vec3, Vec2); 3]]) -> impl Iterator<Item = (Vec3, Vec2, f32)> + '_ {
        const N: usize = 6;
        laid.iter().flat_map(|&[(a, ua), (b, ub), (c, uc)]| {
            let area = (b.xz() - a.xz()).perp_dot(c.xz() - a.xz()).abs() / 2.0;
            let points = (0..N).flat_map(move |i| (0..N - i).map(move |j| (i, j)));
            let count = N * (N + 1) / 2;
            points.map(move |(i, j)| {
                let (u, v) = ((i as f32 + 1.0 / 3.0) / N as f32, (j as f32 + 1.0 / 3.0) / N as f32);
                (a + (b - a) * u + (c - a) * v, ua + (ub - ua) * u + (uc - ua) * v, area / count as f32)
            })
        })
    }

    /// How much of a mark laid by the shader (`laid`, over `heights`) there is, by how opaque it
    /// is over its grid (`opaque`; square meters, seen from above, of it solid): all of it; what
    /// of it is drawn, not off the surface it's laid on; and how much of that is off what's truly
    /// under it (`under`, as it's drawn: `Underfoot::height`).
    fn drawn(laid: &[[(Vec3, Vec2); 3]], heights: &Heights, lift: f32, under: &mut Underfoot, opaque: impl Fn(Vec2) -> f32) -> (f32, f32, f32) {
        let (mut whole, mut drawn, mut off) = (0.0, 0.0, 0.0);
        for (p, uv, area) in spread(laid) {
            let solid = area * opaque(uv);
            if solid <= 0.0 {
                continue;
            }
            whole += solid;
            if (p.y - lift - heights.surface(p.xz())).abs() <= OFF_SURFACE {
                drawn += solid;
                if (p.y - lift - under.height(p.xz())).abs() > 0.03 {
                    off += solid;
                }
            }
        }
        (whole, drawn, off)
    }

    #[test]
    fn the_biggest_tear_and_mark_fit_in_a_slot() {
        let biggest = |class: ClassId| class.def().attack.lane_grown(true, gravecaller_rift(MAX_LEVEL));
        for (class, (near, far, wide)) in ClassId::all().filter_map(|class| Some((class, biggest(class)?))) {
            // The mark along its lane, and the tear's box if it tears the ground (looks round
            // both ends of each).
            let (lo, hi) = tear_reach(near, far, wide);
            let tear = class.def().attack.rift.map(|_| ("tear", hi - lo));
            for (what, size) in [("mark", Vec2::new(far - near, 2.0 * MARK_HALF_WIDTH))].into_iter().chain(tear) {
                let looks = (size / LOOK_STEP).ceil() + 2.0;
                assert!(looks.x <= SLOT.x as f32 && looks.y <= (SLOT.y - 1) as f32, "{class:?}: the biggest {what} needs {looks} looks, more than a slot holds");
            }
        }
    }

    #[test]
    fn a_tear_lies_on_what_is_under_it_however_it_plays_out_wherever_it_is_struck() {
        let (from, range, width) = ClassId::by_key("revenant").unwrap().def().attack.lane(false).unwrap();
        let yard = MapId::Yard.map();
        // North onto the lowest crate, the lane ending on its top.
        let (t, _) = *yard.crates.iter().find(|&&(_, high)| high == 1).unwrap();
        let onto_crate = Vec2::new(Map::center(t).x, Map::center(t).y + 0.4 - range);
        let places = [
            ("level ground", Vec2::new(-34.5, -0.5), Vec2::X, 0.0),
            ("up the stairs", Vec2::new(-9.5, 5.0), Vec2::Y, 0.0),
            ("across the stairs", Vec2::new(-10.5, 5.0), Vec2::new(0.6, 0.8), 0.0),
            ("onto a crate", onto_crate, Vec2::Y, 0.0),
            ("off a terrace", Vec2::new(2.5, -11.8), Vec2::Y, 1.5),
        ];
        // One of the ways it breaks, as the shader samples its texture (the nearest texel).
        let (look, tile) = (tear_look(), UVec2::new(1, 3));
        let texel = |uv: Vec2, channel: usize| {
            let at = tile * TEAR_TILE + (uv * TEAR_TILE.as_vec2()).as_uvec2().min(TEAR_TILE - 1);
            look.data.as_ref().unwrap()[((at.y * TEAR_TILE.x * TEAR_TILES.x + at.x) * 4) as usize + channel] as f32 / 255.0
        };
        let cracks = |thickness: f32| move |uv: Vec2| (1.0 - texel(uv, 2) * CLOSING_REACH / thickness).max(0.0);
        let grid = lying_grid(TEAR_BOX.0, TEAR_BOX.1, TEAR_CELLS, false);
        let (start, lane) = (Vec3::X * from, Vec3::new(range - from, 1.0, width));
        let flared = lane * Vec3::new(1.0, 1.0, 1.0 + FLARE_WIDE);
        for (place, pos, dir, feet) in places {
            let mut under = Underfoot::new(yard, pos, dir, feet);
            let heights = under.heights(start.xz() + TEAR_BOX.0 * flared.xz(), start.xz() + TEAR_BOX.1 * flared.xz());
            // Split open, its light flaring and settling, its cracks closing.
            let layers: [(&str, Vec3, &dyn Fn(Vec2) -> f32); 4] = [
                ("void", lane, &|uv| texel(uv, 0)),
                ("flare", flared, &cracks(CORE_THICKNESS)),
                ("ink", lane, &cracks(INK_THICKNESS)),
                ("closing ink", lane, &cracks(INK_THICKNESS / 2.0)),
            ];
            for (layer, stretch, opaque) in layers {
                let laid = laid(&grid, &heights, start, 0.03, stretch);
                let (whole, drawn, off) = drawn(&laid, &heights, 0.03, &mut under, opaque);
                assert!(drawn > whole * 0.9, "{place}: {layer}: only {drawn} m2 of {whole} m2 drawn");
                assert!(off < whole * 0.025, "{place}: {layer}: {off} m2 of {whole} m2 off what's under it");
            }
        }
    }
}
