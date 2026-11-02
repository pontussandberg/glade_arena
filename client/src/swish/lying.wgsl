// A mark lying on the ground under a lane (`lying.rs`): its grid mirrored across the lane if its
// tag says so, stretched as its transform is, and laid on the heights in its slot of the heights
// texture, as `Heights::surface` works them out; what of it comes off that surface, between
// points laid on either side of a step or an edge, isn't drawn. As opaque over its grid as its
// shape says (`LyingShape`): from the tile of its texture its tag says, or a shape of its own.
// Self-lit and see-through, as a `PlainMaterial` that's unlit.
#import bevy_pbr::{
    mesh_functions,
    pbr_types,
    pbr_functions::main_pass_post_lighting_processing,
    view_transformations::position_world_to_clip,
}

struct Lying {
    color: vec4<f32>,
    pick: vec4<f32>,
    lift: f32,
    from_middle: f32,
    look: f32,
    one_surface: f32,
    slot: vec2<u32>,
    tiles: vec2<u32>,
    slots_along: u32,
    off_surface: f32,
    flags: u32,
    drawn: u32,
    closing_reach: f32,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> lying: Lying;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var heights: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var look_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var look_sampler: sampler;

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct Laid {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec4<f32>,
    // In the lane's plane, and how high it's laid there over what's under it (with its lift).
    @location(1) lane: vec3<f32>,
    // Where on its grid it is.
    @location(2) uv: vec2<f32>,
    // Its slot of the heights (`Slot`), the tile of its texture, how thick its cracks are, how
    // opaque it is.
    @location(3) @interpolate(flat) looks: vec2<i32>,
    @location(4) @interpolate(flat) first: vec2<f32>,
    @location(5) @interpolate(flat) last: vec2<i32>,
    @location(6) @interpolate(flat) tile: u32,
    @location(7) @interpolate(flat) thickness: f32,
    @location(8) @interpolate(flat) opaque: f32,
};

// A slot of the heights, as its header (the row ahead of its looks, `LyingHeights::keep`) says,
// read once for a vertex and handed on to its fragments: where in the texture its looks start,
// where in the lane the first of them is, and which is the last.
struct Slot {
    looks: vec2<i32>,
    first: vec2<f32>,
    last: vec2<i32>,
};

// Where slot `slot` of the heights starts in the texture: its header, then its looks.
fn slot_at(slot: u32) -> vec2<i32> {
    return vec2<i32>(vec2(slot % lying.slots_along, slot / lying.slots_along) * lying.slot);
}

fn header(at: vec2<i32>, i: i32) -> f32 {
    return textureLoad(heights, at + vec2(i, 0), 0).r;
}

fn slot_of(at: vec2<i32>) -> Slot {
    return Slot(at + vec2(0, 1), vec2(header(at, 0), header(at, 1)), vec2<i32>(vec2(header(at, 2), header(at, 3))) - 1);
}

// How high look `k` of the slot is (the nearest one there is, past the edge).
fn look(s: Slot, k: vec2<i32>) -> f32 {
    return textureLoad(heights, s.looks + clamp(k, vec2<i32>(0), s.last), 0).r;
}

// How high what's under the lane's point `at` is: in between the looks round it, where they're
// all on one surface, and the nearest one's where they aren't.
fn surface(slot: Slot, at: vec2<f32>) -> f32 {
    let g = (at - slot.first) / lying.look;
    let k = vec2<i32>(floor(g));
    let f = g - floor(g);
    let a = look(slot, k);
    let b = look(slot, k + vec2(1, 0));
    let c = look(slot, k + vec2(0, 1));
    let d = look(slot, k + vec2(1, 1));
    if max(max(a, b), max(c, d)) - min(min(a, b), min(c, d)) <= lying.one_surface {
        return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
    }
    return look(slot, vec2<i32>(floor(g + 0.5)));
}

@vertex
fn vertex(in: Vertex) -> Laid {
    let world_from_local = mesh_functions::get_world_from_local(in.instance_index);
    let stretch = vec3(length(world_from_local[0].xyz), length(world_from_local[1].xyz), length(world_from_local[2].xyz));
    // Its slot, whether it's mirrored, its tile, how far its cracks are closed, how opaque it is
    // (`lying_tag`).
    let tag = mesh_functions::get_tag(in.instance_index);
    let at_slot = slot_at((tag >> 23u) & 0x3fu);
    let slot = slot_of(at_slot);
    let mirror = select(1.0, -1.0, (tag >> 31u) == 1u);
    let start = vec3(mix(header(at_slot, 4), header(at_slot, 5), lying.from_middle), 0.0, 0.0);
    let at = start + stretch * in.position * vec3(1.0, 1.0, mirror);
    let over = surface(slot, at.xz) + lying.lift;
    // Its transform stretches it (`lying_at`): what's laid in the lane's frame, unstretched.
    let world = world_from_local * vec4(vec3(at.x, over + at.y, at.z) / stretch, 1.0);
    var out: Laid;
    out.position = position_world_to_clip(world.xyz);
    out.world_position = world;
    out.lane = vec3(at.x, over, at.z);
    out.uv = in.uv;
    out.looks = slot.looks;
    out.first = slot.first;
    out.last = slot.last;
    out.tile = (tag >> 20u) & 0x7u;
    out.thickness = f32((tag >> 10u) & 0x3ffu) / 256.0;
    out.opaque = f32(tag & 0x3ffu) / 1023.0;
    return out;
}

// How opaque it is where it is on its grid (`LyingShape`).
fn drawn(in: Laid) -> f32 {
    let tile = vec2(f32(in.tile % lying.tiles.x), f32(in.tile / lying.tiles.x));
    let picked = dot(textureSampleLevel(look_texture, look_sampler, (tile + in.uv) / vec2<f32>(lying.tiles), 0.0), lying.pick);
    switch lying.drawn {
        case 0u: {
            return picked;
        }
        // Each crack as thick as it is now, from its spine (solid) out to its edges (clear).
        case 1u: {
            return 1.0 - picked * lying.closing_reach / max(in.thickness, 1e-4);
        }
        // Soft inside, solid a little in from its edge, clear at it.
        case 2u: {
            let r = length(in.uv - 0.5);
            return select((0.5 - r) / 0.08, (r - 0.3) / 0.12, r < 0.42);
        }
        // Widest a little short of its tip, a needle point at it, clear at its near end; bright
        // along its spine, clear at its edges.
        default: {
            let t = in.uv.x;
            let wide = min(t / 0.17, 1.0) * pow(clamp((1.0 - t) / 0.23, 0.0, 1.0), 0.6);
            return pow(min(t / 0.35, 1.0), 1.5) * (1.0 - abs(in.uv.y * 2.0 - 1.0) / max(wide, 1e-4));
        }
    }
}

@fragment
fn fragment(in: Laid) -> @location(0) vec4<f32> {
    let opaque = clamp(drawn(in), 0.0, 1.0) * in.opaque;
    if opaque <= 0.0 || abs(in.lane.y - lying.lift - surface(Slot(in.looks, in.first, in.last), in.lane.xz)) > lying.off_surface {
        discard;
    }
    var pbr_input = pbr_types::pbr_input_new();
    pbr_input.material.flags = lying.flags;
    pbr_input.world_position = in.world_position;
    pbr_input.frag_coord = in.position;
    return main_pass_post_lighting_processing(pbr_input, lying.color * vec4(1.0, 1.0, 1.0, opaque));
}
