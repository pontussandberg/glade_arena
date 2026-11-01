//! The material everything is drawn with: lit as Bevy's `StandardMaterial` is (its own shader
//! functions, the same inputs), but for flat colors only, with no textures. On WebGL a
//! `StandardMaterial` bound all its textures and samplers anew (fallbacks, as ours have none) at
//! every switch of material, a few GL calls each, which came to about half of a crowded frame's GL
//! calls; this one binds a uniform. Where it can be (not on WebGL), it's bindless, as
//! `StandardMaterial` is: all materials in one bind group, so draws batch across them.
//!
//! It has no prepass shader of its own, so its shadows are drawn depth only, without the material
//! (a see-through one casts its whole shadow, where a `StandardMaterial` leaves out what of it is
//! all but clear).

use bevy::asset::{embedded_asset, embedded_path, AssetPath};
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::{AsBindGroup, AsBindGroupShaderType, Face, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError};
use bevy::render::texture::GpuImage;
use bevy::shader::ShaderRef;

pub struct PlainPlugin;

impl Plugin for PlainPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "plain.wgsl");
        app.add_plugins(MaterialPlugin::<PlainMaterial>::default());
    }
}

/// A flat color, lit (or not, `unlit`) as `StandardMaterial`'s are: its fields mean what that's
/// same-named ones do, and default to what they do. Vertex colors (if any) multiply `base_color`.
#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
#[bind_group_data(PlainKey)]
#[data(0, PlainUniform, binding_array(10))]
#[bindless]
pub struct PlainMaterial {
    pub base_color: Color,
    pub emissive: LinearRgba,
    pub perceptual_roughness: f32,
    pub metallic: f32,
    pub reflectance: f32,
    pub unlit: bool,
    pub double_sided: bool,
    pub fog_enabled: bool,
    pub alpha_mode: AlphaMode,
    pub depth_bias: f32,
    pub cull_mode: Option<Face>,
}

impl Default for PlainMaterial {
    fn default() -> Self {
        PlainMaterial {
            base_color: Color::WHITE,
            emissive: LinearRgba::BLACK,
            perceptual_roughness: 0.5,
            metallic: 0.0,
            reflectance: 0.5,
            unlit: false,
            double_sided: false,
            fog_enabled: true,
            alpha_mode: AlphaMode::Opaque,
            depth_bias: 0.0,
            cull_mode: Some(Face::Back),
        }
    }
}

/// `color`, see-through if it is (as `StandardMaterial::from` a color).
impl From<Color> for PlainMaterial {
    fn from(color: Color) -> Self {
        PlainMaterial { base_color: color, alpha_mode: if color.alpha() < 1.0 { AlphaMode::Blend } else { AlphaMode::Opaque }, ..default() }
    }
}

/// What the shader reads (`plain.wgsl`), laid out as `StandardMaterial`'s flags are.
#[derive(Clone, Default, ShaderType)]
pub struct PlainUniform {
    base_color: Vec4,
    emissive: Vec4,
    reflectance: Vec3,
    perceptual_roughness: f32,
    metallic: f32,
    flags: u32,
    alpha_cutoff: f32,
}

// `StandardMaterial`'s flags (`bevy_pbr::pbr_types`), those a plain material can have.
const DOUBLE_SIDED: u32 = 1 << 4;
const UNLIT: u32 = 1 << 5;
const FOG_ENABLED: u32 = 1 << 8;
const ALPHA_MODE_SHIFT: u32 = 29;

impl AsBindGroupShaderType<PlainUniform> for PlainMaterial {
    fn as_bind_group_shader_type(&self, _images: &RenderAssets<GpuImage>) -> PlainUniform {
        let mut flags = 0;
        for (on, flag) in [(self.double_sided, DOUBLE_SIDED), (self.unlit, UNLIT), (self.fog_enabled, FOG_ENABLED)] {
            if on {
                flags |= flag;
            }
        }
        let (alpha_mode, alpha_cutoff) = match self.alpha_mode {
            AlphaMode::Opaque => (0, 0.5),
            AlphaMode::Mask(cutoff) => (1, cutoff),
            AlphaMode::Blend => (2, 0.5),
            AlphaMode::Premultiplied => (3, 0.5),
            AlphaMode::Add => (4, 0.5),
            AlphaMode::Multiply => (5, 0.5),
            AlphaMode::AlphaToCoverage => (6, 0.5),
        };
        flags |= alpha_mode << ALPHA_MODE_SHIFT;
        PlainUniform {
            base_color: LinearRgba::from(self.base_color).to_vec4(),
            // No exposure weight in the alpha, as `StandardMaterial`'s default.
            emissive: self.emissive.to_vec3().extend(0.0),
            reflectance: Vec3::splat(self.reflectance),
            perceptual_roughness: self.perceptual_roughness,
            metallic: self.metallic,
            flags,
            alpha_cutoff,
        }
    }
}

/// What a plain material's pipeline is made for: which faces it culls, and its depth bias.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlainKey {
    cull_mode: Option<Face>,
    depth_bias: i32,
}

impl From<&PlainMaterial> for PlainKey {
    fn from(material: &PlainMaterial) -> Self {
        PlainKey { cull_mode: material.cull_mode, depth_bias: material.depth_bias as i32 }
    }
}

impl Material for PlainMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Path(AssetPath::from_path_buf(embedded_path!("plain.wgsl")).with_source("embedded"))
    }

    fn alpha_mode(&self) -> AlphaMode {
        self.alpha_mode
    }

    fn depth_bias(&self) -> f32 {
        self.depth_bias
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // As `StandardMaterial` does (no view of ours turns culling inside out).
        descriptor.primitive.cull_mode = key.bind_group_data.cull_mode;
        if let Some(depth_stencil) = descriptor.depth_stencil.as_mut() {
            depth_stencil.bias.constant = key.bind_group_data.depth_bias;
        }
        Ok(())
    }
}
