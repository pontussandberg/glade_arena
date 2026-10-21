//! Sound, synthesized from nothing: there are no audio files. Every effect is built at the
//! moment it plays out of WebAudio oscillators and filtered noise — a blow is a thump and a
//! slap of noise, a kill a deeper thock with a chiff off the top, a horn two detuned saws
//! swelling through a lowpass. Gameplay systems push `Sfx` onto the `SoundQueue` with a gain
//! (loudness, usually by how close it is); `play_sounds` drains it each frame.
//!
//! The browser keeps an `AudioContext` suspended until the page is interacted with, so the
//! queue is dropped (and a resume asked for) until it's running: nothing can sound before the
//! first click anyway. Only the web client sounds; native builds drain the queue silently.

use bevy::prelude::*;

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SoundQueue>();
        app.add_systems(Update, swing_sounds);
        #[cfg(target_family = "wasm")]
        {
            app.init_non_send::<web::AudioLazy>();
            app.add_systems(Update, web::play_sounds);
        }
        #[cfg(not(target_family = "wasm"))]
        app.add_systems(Update, |mut queue: ResMut<SoundQueue>| queue.sounds.clear());
    }
}

/// The sounds there are.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Sfx {
    /// A blade (or claw) cutting the air: any melee swing nearby.
    Swing,
    /// A blow landing on someone else.
    Hit,
    /// A crit landing.
    Crit,
    /// A blow we take.
    Hurt,
    /// A monster dying.
    Kill,
    /// A boss dying.
    BossKill,
    /// A heal reaching us.
    Heal,
    /// Gold flying.
    Coin,
    /// Lightning striking.
    Thunder,
    /// A keg, a meteor: a blast.
    Explosion,
    /// A wave starting.
    Horn,
    /// A boss wave starting.
    BossHorn,
    /// The run is over.
    Dirge,
}

/// A sound asked for: played next frame (or `delay` seconds later, for one whose cause is yet
/// to land, like a meteor). Only the web client reads it; native drains the queue unread.
#[cfg_attr(not(target_family = "wasm"), allow(dead_code))]
struct Sound {
    kind: Sfx,
    gain: f32,
    delay: f32,
}

/// What wants to sound this frame. Push with a gain of 0 to 1 — usually how close the cause is;
/// a gain of nothing is dropped here.
#[derive(Resource, Default)]
pub struct SoundQueue {
    sounds: Vec<Sound>,
}

impl SoundQueue {
    pub fn push(&mut self, kind: Sfx, gain: f32) {
        self.push_in(kind, gain, 0.0);
    }

    pub fn push_in(&mut self, kind: Sfx, gain: f32, delay: f32) {
        if gain > 0.01 {
            self.sounds.push(Sound { kind, gain: gain.min(1.0), delay });
        }
    }
}

/// How loud a sound `away` meters off is, out to `reach`: full up close, nothing past it.
pub fn earshot(away: f32, reach: f32) -> f32 {
    (1.0 - away / reach).clamp(0.0, 1.0)
}

/// Every melee swing nearby cuts the air (`LastSwing` comes from the server, once per strike).
fn swing_sounds(
    me: Query<&arena_shared::protocol::Pos, With<crate::Me>>,
    swings: Query<&arena_shared::protocol::Pos, (Changed<arena_shared::protocol::LastSwing>, With<arena_shared::protocol::PlayerId>)>,
    mut queue: ResMut<SoundQueue>,
) {
    let Ok(me) = me.single() else { return };
    for pos in &swings {
        queue.push(Sfx::Swing, 0.7 * earshot(pos.0.distance(me.0), 13.0));
    }
}

#[cfg(target_family = "wasm")]
mod web {
    use super::*;

    use web_sys::{AudioBuffer, AudioContext, AudioContextState, AudioNode, BiquadFilterType, DynamicsCompressorNode, GainNode, OscillatorType};

    /// Everything sounds through this: each sound's own nodes feed the compressor (so a burst of
    /// kills squeezes instead of clipping), then the master gain, then out.
    pub(super) struct AudioOut {
        ctx: AudioContext,
        master: GainNode,
        squeeze: DynamicsCompressorNode,
        noise: AudioBuffer,
        seed: u32,
        /// The wind is started once, the first time sound gets through.
        ambience_on: bool,
    }

    /// Built the first time a sound is asked for.
    #[derive(Default)]
    pub(super) struct AudioLazy(Option<AudioOut>);

    const MASTER: f32 = 0.5;
    /// The most voices one kind of sound gets in a frame (the rest fold into their gain), and
    /// how far apart they start, so a cleared swarm crackles instead of phasing into one blast.
    const VOICES: usize = 3;
    const STAGGER: f64 = 0.014;

    pub(super) fn play_sounds(mut queue: ResMut<SoundQueue>, mut audio: NonSendMut<AudioLazy>) {
        if queue.sounds.is_empty() {
            return;
        }
        let mut sounds = std::mem::take(&mut queue.sounds);
        if audio.0.is_none() {
            audio.0 = AudioOut::new();
        }
        let Some(out) = audio.0.as_mut() else { return };
        if out.ctx.state() != AudioContextState::Running {
            // Asked for on every sound until the browser lets audio through (after the page's
            // first interaction); what queued meanwhile is dropped, not saved up.
            let _ = out.ctx.resume();
            return;
        }
        out.ambience();
        // The loudest few of each kind sound, briefly staggered; the rest only thicken them.
        sounds.sort_by(|a, b| a.kind.cmp(&b.kind).then(b.gain.total_cmp(&a.gain)));
        let mut k = 0;
        let mut kind = None;
        for sound in &sounds {
            k = if Some(sound.kind) == kind { k + 1 } else { 0 };
            kind = Some(sound.kind);
            if k >= VOICES {
                continue;
            }
            let crowd = sounds.iter().filter(|s| s.kind == sound.kind).count();
            let gain = sound.gain * (1.0 + (crowd.saturating_sub(VOICES) as f32) * 0.1).min(1.6);
            out.play(sound.kind, gain.min(1.0), sound.delay as f64 + k as f64 * STAGGER);
        }
    }

    impl AudioOut {
        fn new() -> Option<AudioOut> {
            let ctx = AudioContext::new().ok()?;
            let squeeze = ctx.create_dynamics_compressor().ok()?;
            let master = ctx.create_gain().ok()?;
            master.gain().set_value(MASTER);
            squeeze.connect_with_audio_node(&master).ok()?;
            master.connect_with_audio_node(&ctx.destination()).ok()?;
            // A second of white noise every noisy sound reads from, each from its own spot.
            let rate = ctx.sample_rate();
            let noise = ctx.create_buffer(1, rate as u32, rate).ok()?;
            let mut seed: u32 = 0x9E37_79B9;
            let mut samples: Vec<f32> = Vec::with_capacity(rate as usize);
            for _ in 0..rate as usize {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                samples.push((seed >> 8) as f32 / (1u32 << 23) as f32 - 1.0);
            }
            noise.copy_to_channel(&samples, 0).ok()?;
            Some(AudioOut { ctx, master, squeeze, noise, seed: 1, ambience_on: false })
        }

        /// A number in 0..1, never the same twice.
        fn r(&mut self) -> f32 {
            self.seed = self.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (self.seed >> 8) as f32 / (1u32 << 24) as f32
        }

        /// A pitch multiplier within `spread` of 1, so no two plays are the twin of each other.
        fn vary(&mut self, spread: f32) -> f32 {
            1.0 + (self.r() * 2.0 - 1.0) * spread
        }

        /// An envelope into the mix: silent at `when`, up to `peak` over `attack`, dying away to
        /// nothing at `when + dur`.
        fn env(&self, when: f64, attack: f64, peak: f32, dur: f64) -> Option<GainNode> {
            let g = self.ctx.create_gain().ok()?;
            let p = g.gain();
            let _ = p.set_value_at_time(0.0001, when);
            let _ = p.linear_ramp_to_value_at_time(peak.max(0.0002), when + attack);
            let _ = p.exponential_ramp_to_value_at_time(0.0001, when + dur);
            g.connect_with_audio_node(&self.squeeze).ok()?;
            Some(g)
        }

        /// An oscillator gliding `from` to `to` Hz through `into`, from `when` for `dur`.
        fn osc(&self, kind: OscillatorType, from: f32, to: f32, when: f64, dur: f64, into: &AudioNode) {
            let Ok(o) = self.ctx.create_oscillator() else { return };
            o.set_type(kind);
            let f = o.frequency();
            let _ = f.set_value_at_time(from.max(1.0), when);
            let _ = f.exponential_ramp_to_value_at_time(to.max(1.0), when + dur);
            if o.connect_with_audio_node(into).is_ok() {
                let _ = o.start_with_when(when);
                let _ = o.stop_with_when(when + dur + 0.05);
            }
        }

        /// Noise through a filter gliding `from` to `to` Hz into `into`, from `when` for `dur`.
        fn noise(&mut self, kind: BiquadFilterType, from: f32, to: f32, q: f32, when: f64, dur: f64, into: &AudioNode) {
            let Ok(source) = self.ctx.create_buffer_source() else { return };
            source.set_buffer(Some(&self.noise));
            source.set_loop(true);
            let _ = source.set_loop_start(0.0);
            let offset = (self.r() * 0.8) as f64;
            let Ok(filter) = self.ctx.create_biquad_filter() else { return };
            filter.set_type(kind);
            let f = filter.frequency();
            let _ = f.set_value_at_time(from.max(10.0), when);
            let _ = f.exponential_ramp_to_value_at_time(to.max(10.0), when + dur);
            filter.q().set_value(q);
            if source.connect_with_audio_node(&filter).is_ok() && filter.connect_with_audio_node(into).is_ok() {
                let _ = source.start_with_when_and_grain_offset(when, offset);
                // web-sys deprecated this spelling, but it's the one way to schedule the stop.
                #[allow(deprecated)]
                let _ = source.stop_with_when(when + dur + 0.05);
            }
        }

        /// A quiet bed of wind under everything: looped noise through a low filter, swelling and
        /// dying on a slow breath, so the world is never dead silent. Skips the compressor: the
        /// fights shouldn't pump the wind.
        fn ambience(&mut self) {
            if std::mem::replace(&mut self.ambience_on, true) {
                return;
            }
            let now = self.ctx.current_time();
            let (Ok(source), Ok(filter), Ok(gain), Ok(lfo), Ok(depth)) =
                (self.ctx.create_buffer_source(), self.ctx.create_biquad_filter(), self.ctx.create_gain(), self.ctx.create_oscillator(), self.ctx.create_gain())
            else {
                return;
            };
            source.set_buffer(Some(&self.noise));
            source.set_loop(true);
            filter.set_type(BiquadFilterType::Lowpass);
            filter.frequency().set_value(240.0);
            gain.gain().set_value(0.022);
            lfo.frequency().set_value(0.08);
            depth.gain().set_value(0.013);
            let connected = source.connect_with_audio_node(&filter).is_ok()
                && filter.connect_with_audio_node(&gain).is_ok()
                && gain.connect_with_audio_node(&self.master).is_ok()
                && lfo.connect_with_audio_node(&depth).is_ok()
                && depth.connect_with_audio_param(&gain.gain()).is_ok();
            if connected {
                let _ = source.start_with_when(now);
                let _ = lfo.start_with_when(now);
            }
        }

        fn play(&mut self, kind: Sfx, gain: f32, delay: f64) {
            use BiquadFilterType::{Bandpass, Highpass, Lowpass};
            use OscillatorType::{Sawtooth, Sine, Triangle};
            let now = self.ctx.current_time() + 0.01 + delay;
            let g = gain;
            let p = self.vary(0.12);
            match kind {
                Sfx::Swing => {
                    if let Some(e) = self.env(now, 0.05, 0.14 * g, 0.16) {
                        self.noise(Bandpass, 500.0 * p, 1500.0 * p, 1.4, now, 0.16, &e);
                    }
                }
                Sfx::Hit => {
                    if let Some(e) = self.env(now, 0.004, 0.4 * g, 0.06) {
                        self.osc(Sine, 260.0 * p, 130.0, now, 0.06, &e);
                    }
                    if let Some(e) = self.env(now, 0.002, 0.22 * g, 0.045) {
                        self.noise(Bandpass, 1800.0 * p, 1200.0, 1.0, now, 0.045, &e);
                    }
                }
                Sfx::Crit => {
                    if let Some(e) = self.env(now, 0.003, 0.35 * g, 0.11) {
                        self.osc(Sawtooth, 900.0 * p, 200.0, now, 0.11, &e);
                    }
                    if let Some(e) = self.env(now, 0.002, 0.4 * g, 0.09) {
                        self.osc(Sine, 200.0 * p, 80.0, now, 0.09, &e);
                    }
                    if let Some(e) = self.env(now, 0.002, 0.2 * g, 0.05) {
                        self.noise(Highpass, 2500.0, 2000.0, 0.8, now, 0.05, &e);
                    }
                }
                Sfx::Hurt => {
                    if let Some(e) = self.env(now, 0.005, 0.5 * g, 0.13) {
                        self.osc(Sine, 170.0 * p, 55.0, now, 0.13, &e);
                    }
                    if let Some(e) = self.env(now, 0.003, 0.3 * g, 0.09) {
                        self.noise(Lowpass, 600.0, 250.0, 0.7, now, 0.09, &e);
                    }
                }
                Sfx::Kill => {
                    if let Some(e) = self.env(now, 0.004, 0.6 * g, 0.13) {
                        self.osc(Sine, 210.0 * p, 55.0, now, 0.13, &e);
                    }
                    if let Some(e) = self.env(now, 0.003, 0.45 * g, 0.14) {
                        self.noise(Lowpass, 900.0 * p, 220.0, 0.9, now, 0.14, &e);
                    }
                    if let Some(e) = self.env(now, 0.001, 0.12 * g, 0.035) {
                        self.noise(Bandpass, 3200.0 * p, 2800.0, 2.0, now, 0.035, &e);
                    }
                }
                Sfx::BossKill => {
                    if let Some(e) = self.env(now, 0.01, 0.85 * g, 0.7) {
                        self.osc(Sine, 120.0, 34.0, now, 0.7, &e);
                    }
                    if let Some(e) = self.env(now, 0.005, 0.6 * g, 0.9) {
                        self.noise(Lowpass, 500.0, 90.0, 0.8, now, 0.9, &e);
                    }
                    if let Some(e) = self.env(now, 0.02, 0.25 * g, 0.6) {
                        self.osc(Sawtooth, 70.0, 28.0, now, 0.6, &e);
                    }
                }
                Sfx::Heal => {
                    if let Some(e) = self.env(now, 0.02, 0.16 * g, 0.2) {
                        self.osc(Sine, 660.0 * p, 680.0 * p, now, 0.2, &e);
                    }
                    if let Some(e) = self.env(now + 0.06, 0.02, 0.12 * g, 0.24) {
                        self.osc(Sine, 990.0 * p, 1010.0 * p, now + 0.06, 0.24, &e);
                    }
                }
                Sfx::Coin => {
                    if let Some(e) = self.env(now, 0.002, 0.18 * g, 0.07) {
                        self.osc(Triangle, 1320.0 * p, 1320.0 * p, now, 0.07, &e);
                    }
                    if let Some(e) = self.env(now + 0.05, 0.002, 0.15 * g, 0.1) {
                        self.osc(Triangle, 1760.0 * p, 1760.0 * p, now + 0.05, 0.1, &e);
                    }
                }
                Sfx::Thunder => {
                    if let Some(e) = self.env(now, 0.002, 0.35 * g, 0.06) {
                        self.noise(Bandpass, 1500.0, 900.0, 1.2, now, 0.06, &e);
                    }
                    if let Some(e) = self.env(now + 0.02, 0.01, 0.7 * g, 1.1) {
                        self.noise(Lowpass, 300.0, 70.0, 0.6, now + 0.02, 1.1, &e);
                    }
                }
                Sfx::Explosion => {
                    if let Some(e) = self.env(now, 0.005, 0.75 * g, 0.65) {
                        self.noise(Lowpass, 800.0, 90.0, 0.7, now, 0.65, &e);
                    }
                    if let Some(e) = self.env(now, 0.004, 0.5 * g, 0.4) {
                        self.osc(Sine, 100.0 * p, 32.0, now, 0.4, &e);
                    }
                }
                Sfx::Horn => {
                    for (detune, peak) in [(1.0, 0.2), (1.006, 0.2), (1.5, 0.08)] {
                        if let Some(e) = self.env(now, 0.22, peak * g, 1.0) {
                            let f = 98.0 * detune;
                            self.osc(Sawtooth, f, f * 1.02, now, 1.0, &e);
                        }
                    }
                }
                Sfx::BossHorn => {
                    for (detune, peak) in [(1.0, 0.26), (1.05, 0.2), (1.01, 0.22)] {
                        if let Some(e) = self.env(now, 0.4, peak * g, 1.8) {
                            let f = 55.0 * detune;
                            self.osc(Sawtooth, f, f * 0.97, now, 1.8, &e);
                        }
                    }
                }
                Sfx::Dirge => {
                    for (i, f) in [220.0, 174.6, 146.8].iter().enumerate() {
                        let at = now + i as f64 * 0.45;
                        if let Some(e) = self.env(at, 0.05, 0.16 * g, 0.9) {
                            self.osc(Triangle, *f, *f, at, 0.9, &e);
                        }
                    }
                    if let Some(e) = self.env(now, 0.3, 0.2 * g, 2.2) {
                        self.osc(Sine, 73.4, 72.0, now, 2.2, &e);
                    }
                }
            }
        }
    }
}
