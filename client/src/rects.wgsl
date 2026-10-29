// Flat colored rectangles over the UI (`rects.rs`): each one drawn as two triangles, its six
// corners picked out of its left, top, right and bottom by which corner of them it is; one with
// round corners cut to them, its edge smoothed over a pixel.
#import bevy_render::view::View

@group(0) @binding(0) var<uniform> view: View;

struct Quad {
    @location(0) sides: vec4<f32>,
    @location(1) color: vec4<f32>,
    @location(2) radius: f32,
};

struct Drawn {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    // Where it is from the rectangle's middle, its half size, and its corners' radius.
    @location(1) from_middle: vec2<f32>,
    @location(2) half: vec2<f32>,
    @location(3) radius: f32,
};

@vertex
fn vertex(@builtin(vertex_index) corner: u32, quad: Quad) -> Drawn {
    // Left top, right top, left bottom; left bottom, right top, right bottom: one bit each.
    let right = f32((0x32u >> corner) & 1u);
    let bottom = f32((0x2Cu >> corner) & 1u);
    let at = mix(quad.sides.xy, quad.sides.zw, vec2(right, bottom));
    let half = (quad.sides.zw - quad.sides.xy) * 0.5;
    let radius = min(quad.radius, min(half.x, half.y));
    return Drawn(view.clip_from_world * vec4(at, 0.0, 1.0), quad.color, at - quad.sides.xy - half, half, radius);
}

@fragment
fn fragment(drawn: Drawn) -> @location(0) vec4<f32> {
    if drawn.radius <= 0.0 {
        return drawn.color;
    }
    // How far out of the rounded rectangle this is (negative inside).
    let corner = abs(drawn.from_middle) - drawn.half + drawn.radius;
    let out = length(max(corner, vec2(0.0))) + min(max(corner.x, corner.y), 0.0) - drawn.radius;
    return vec4(drawn.color.rgb, drawn.color.a * clamp(0.5 - out, 0.0, 1.0));
}
