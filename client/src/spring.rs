//! A value on a damped spring, solved exactly: the same motion at any frame rate. What the
//! fighters' joints move on (`rig::Joints`), and a thrown spear's rubbery wind.

use std::f32::consts::PI;

/// A value on a damped spring: chases its target with a little lag and overshoot, then settles.
#[derive(Default, Clone)]
pub(crate) struct Spring {
    pub(crate) at: f32,
    pub(crate) speed: f32,
    /// Its target last step: it moves on from there to the next one steadily through the step.
    pub(crate) target: Option<f32>,
}

impl Spring {
    pub(crate) fn follow(&mut self, target: f32, dt: f32, stiffness: f32, damping: f32) -> f32 {
        // Solved exactly over the step, its target moving steadily from where it was last step
        // (unless that's a jump right round), so it moves the same at any frame rate; damped at
        // least critically (no overshoot) above 1. Chasing a target moving at `rate`, it trails
        // `lag` behind it; how far it is off that swings out and settles.
        let from = self.target.filter(|from| (target - from).abs() < PI && dt > 0.0).unwrap_or(target);
        let rate = if dt > 0.0 { (target - from) / dt } else { 0.0 };
        let (w, z) = (stiffness, damping);
        let lag = -2.0 * z.min(1.0) * rate / w;
        let (off, v) = (self.at - from - lag, self.speed - rate);
        let (off, v) = if z < 1.0 {
            let wd = w * (1.0 - z * z).sqrt();
            let fade = (-z * w * dt).exp();
            let (sin, cos) = (wd * dt).sin_cos();
            let b = (v + z * w * off) / wd;
            (fade * (off * cos + b * sin), fade * ((b * wd - z * w * off) * cos - (off * wd + z * w * b) * sin))
        } else {
            let fade = (-w * dt).exp();
            let b = v + w * off;
            (fade * (off + b * dt), fade * (b - w * (off + b * dt)))
        };
        self.at = target + lag + off;
        self.speed = v + rate;
        self.target = Some(target);
        // Settled: stop, so a still fighter stops writing transforms.
        if (target - self.at).abs() < 1e-4 && self.speed.abs() < 1e-3 {
            *self = Spring { at: target, speed: 0.0, target: Some(target) };
        }
        self.at
    }
}
