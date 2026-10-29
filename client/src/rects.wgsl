// Flat colored rectangles over the UI (`rects.rs`): each one drawn as two triangles, its six
// corners picked out of its left, top, right and bottom by which corner of them it is.
#import bevy_render::view::View

@group(0) @binding(0) var<uniform> view: View;

struct Quad {
    @location(0) sides: vec4<f32>,
    @location(1) color: vec4<f32>,
};

struct Drawn {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vertex(@builtin(vertex_index) corner: u32, quad: Quad) -> Drawn {
    // Left top, right top, left bottom; left bottom, right top, right bottom: one bit each.
    let right = f32((0x32u >> corner) & 1u);
    let bottom = f32((0x2Cu >> corner) & 1u);
    let at = mix(quad.sides.xy, quad.sides.zw, vec2(right, bottom));
    return Drawn(view.clip_from_world * vec4(at, 0.0, 1.0), quad.color);
}

@fragment
fn fragment(drawn: Drawn) -> @location(0) vec4<f32> {
    return drawn.color;
}
