//! Easing between 0 and 1: how things ramp in and out over time or space.

/// `t` clamped to 0..1, eased in and out (smoothstep): slow at either end.
pub(crate) fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// 0 below `from`, 1 above `to`, smooth in between.
pub(crate) fn ease(from: f32, to: f32, x: f32) -> f32 {
    smooth((x - from) / (to - from))
}

/// 0 below `from`, 1 above `to`: fast at first, then slowing into place. A committed strike.
pub(crate) fn ease_out(from: f32, to: f32, x: f32) -> f32 {
    let t = 1.0 - ((x - from) / (to - from)).clamp(0.0, 1.0);
    1.0 - t * t * t
}

/// 0 below `from`, 1 above `to`: slow at first, faster and faster. A heavy blow.
pub(crate) fn ease_in(from: f32, to: f32, x: f32) -> f32 {
    ((x - from) / (to - from)).clamp(0.0, 1.0).powi(3)
}
