// A plain material (`plain.rs`): `StandardMaterial`'s fragment shader (`bevy_pbr`'s pbr.wgsl and
// pbr_fragment.wgsl) for a flat color, no textures: the same lighting from the same inputs.
#import bevy_pbr::{
    pbr_types,
    pbr_functions,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
    mesh_bindings::mesh,
    mesh_view_bindings::view,
}

struct Plain {
    base_color: vec4<f32>,
    emissive: vec4<f32>,
    reflectance: vec3<f32>,
    perceptual_roughness: f32,
    metallic: f32,
    flags: u32,
    alpha_cutoff: f32,
};

#ifdef BINDLESS
// Each material's place in the one bind group: where its data is in `plains`.
struct PlainSlot {
    plain: u32,
};
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<storage> plain_slots: array<PlainSlot>;
@group(#{MATERIAL_BIND_GROUP}) @binding(10) var<storage> plains: array<Plain>;
#else
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> plain_bound: Plain;
#endif

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
#ifdef BINDLESS
    let plain = plains[plain_slots[mesh[in.instance_index].material_and_lightmap_bind_group_slot & 0xffffu].plain];
#else
    let plain = plain_bound;
#endif
    let flags = plain.flags;
    let double_sided = (flags & pbr_types::STANDARD_MATERIAL_FLAGS_DOUBLE_SIDED_BIT) != 0u;
    let unlit = (flags & pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT) != 0u;

    var pbr_input = pbr_types::pbr_input_new();
    pbr_input.flags = mesh[in.instance_index].flags;
    pbr_input.is_orthographic = view.clip_from_view[3].w == 1.0;
    pbr_input.V = pbr_functions::calculate_view(in.world_position, pbr_input.is_orthographic);
    pbr_input.frag_coord = in.position;
    pbr_input.world_position = in.world_position;
#ifdef VERTEX_COLORS
    pbr_input.material.base_color = in.color;
#endif
    pbr_input.world_normal = pbr_functions::prepare_world_normal(in.world_normal, double_sided, is_front);
    pbr_input.N = normalize(pbr_input.world_normal);

    pbr_input.material.flags = flags;
    pbr_input.material.base_color *= plain.base_color;
    pbr_input.material.alpha_cutoff = plain.alpha_cutoff;
    if !unlit {
        pbr_input.material.reflectance = plain.reflectance;
        pbr_input.material.emissive = plain.emissive;
        pbr_input.material.metallic = plain.metallic;
        pbr_input.material.perceptual_roughness = plain.perceptual_roughness;
        pbr_input.clearcoat_N = pbr_input.N;
    }
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

    var out: FragmentOutput;
    if unlit {
        out.color = pbr_input.material.base_color;
    } else {
        out.color = apply_pbr_lighting(pbr_input);
    }
    // Haze, and the rest done after lighting in the shader (as for any `StandardMaterial`).
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
