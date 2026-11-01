// The effects' parts (`draw.rs`): corners already in the world and already colored, drawn as they
// are (unlit, with the haze worked in by `draw_parts`).
#import bevy_render::view::View

@group(0) @binding(0) var<uniform> view: View;

struct Corner {
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
};

struct Drawn {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vertex(corner: Corner) -> Drawn {
    return Drawn(view.clip_from_world * vec4(corner.position, 1.0), corner.color);
}

// The opacity comes with how many times over the triangle is laid (`draw::layered`): blended as
// if drawn that many times, one over the other.
@fragment
fn fragment(drawn: Drawn) -> @location(0) vec4<f32> {
    let over = floor(drawn.color.a);
    let opacity = clamp((drawn.color.a - over - 0.25) * 2.0, 0.0, 1.0);
    return vec4(drawn.color.rgb, 1.0 - pow(1.0 - opacity, over + 1.0));
}
