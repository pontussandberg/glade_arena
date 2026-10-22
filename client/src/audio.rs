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
        app.add_systems(Update, (swing_sounds, windup_sounds, loose_sounds, dash_sounds, heartbeat));
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
    /// A blade (or claw) cutting the air: the fallback swing for a figure without its own,
    /// pitched by the weapon's weight — and, pitched down, the rush of a dash.
    Swing,
    /// The revenant's glaive: a broad, heavy cut with a hiss off the edge.
    GlaiveSwing,
    /// The warden's thrust: the shield's face driven in first, the sword's steel a beat behind.
    ShieldThrust,
    /// The warden gathering: the shield coming up, the sword drawn back behind its rim.
    ShieldReady,
    /// Someone slammed into a wall.
    Wallop,
    /// The breaker's maul coming round: a long, deep haul of air, then the head landing in the
    /// earth, the haft groaning with it.
    MaulSwing,
    /// The breaker winding up: the haft creaking as the whole body loads behind it.
    MaulReady,
    /// The shade's knives: one snick, then the other hand's a beat behind, with a touch of
    /// steel on the second.
    KnifeSwing,
    /// The shade gathering: next to nothing, a catch of breath and the knives turning in the hands.
    KnifeReady,
    /// A zombie's claws: a ragged, breathy swipe, nothing clean about it.
    ClawSwing,
    /// The faint gather of air before any attack goes off.
    Windup,
    /// A shot loosed, with nothing more telling to say about it.
    Loose,
    /// A spear taking flight: wind whistling over the shaft.
    SpearLoose,
    /// A gout of fire tossed: it catches with a snap, whoomps away, and crackles after.
    FireLoose,
    /// The pyre witch gathering: fire catching in the hand, a fuse of a hiss.
    KindleReady,
    /// A burning fighter going up all at once.
    Ignite,
    /// A frostbolt leaving the hand: a glassy shiver.
    FrostLoose,
    /// A storm bolt let go: one hard crack, a buzz of charge under it, a fizz of static after.
    StormLoose,
    /// The stormweaver gathering: static crackling up the staff, quicker and quicker.
    StormReady,
    /// Lightning arcing from one to the next.
    Arc,
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
    /// A nova going off: a heavy thump into the ground.
    Slam,
    /// A wave starting.
    Horn,
    /// A boss wave starting.
    BossHorn,
    /// The run is over.
    Dirge,
    /// Your own heart, when you're nearly done.
    Heart,
}

/// A sound asked for: played next frame (or `delay` seconds later, for one whose cause is yet
/// to land, like a meteor). Only the web client reads it; native drains the queue unread.
#[cfg_attr(not(target_family = "wasm"), allow(dead_code))]
struct Sound {
    kind: Sfx,
    gain: f32,
    delay: f32,
    /// A pitch multiplier: a kill chain climbs, so a cleared swarm sings up a little scale.
    pitch: f32,
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
        self.push_pitched(kind, gain, delay, 1.0);
    }

    pub fn push_pitched(&mut self, kind: Sfx, gain: f32, delay: f32, pitch: f32) {
        if gain > 0.01 {
            self.sounds.push(Sound { kind, gain: gain.min(1.0), delay, pitch });
        }
    }
}

/// How loud a sound `away` meters off is, out to `reach`: full up close, nothing past it.
pub fn earshot(away: f32, reach: f32) -> f32 {
    (1.0 - away / reach).clamp(0.0, 1.0)
}

/// How heavy a class's weapon sounds, from how slowly it attacks: a knife rides high and quick,
/// a maul low and long. A pitch multiplier around 1.
fn weapon_pitch(class: arena_shared::protocol::ClassId) -> f32 {
    (40.0 / class.def().attack.attack_ticks as f32).clamp(0.65, 1.35)
}

/// What a class's auto-attack sounds like: as it gathers (the windup) and as it goes off, and
/// the pitch both play at. Each hero's is engineered for its weapon and the fantasy behind it; a
/// figure without its own gets the plain gather and swish, pitched by its weapon's weight.
fn attack_voice(class: arena_shared::protocol::ClassId) -> (Sfx, Sfx, f32) {
    match class.def().figure() {
        "revenant" => (Sfx::Windup, Sfx::GlaiveSwing, 1.0),
        "warden" => (Sfx::ShieldReady, Sfx::ShieldThrust, 1.0),
        "breaker" => (Sfx::MaulReady, Sfx::MaulSwing, 1.0),
        // Casters' shots sound as they leave the hand (`loose_sounds`): only the gather is theirs here.
        "pyre_witch" => (Sfx::KindleReady, Sfx::Swing, 1.0),
        "stormweaver" => (Sfx::StormReady, Sfx::Swing, 1.0),
        "shade" => (Sfx::KnifeReady, Sfx::KnifeSwing, 1.0),
        "zombie" => (Sfx::Windup, Sfx::ClawSwing, 1.0),
        _ => (Sfx::Windup, Sfx::Swing, weapon_pitch(class)),
    }
}

/// Every shot loosed nearby sounds as it leaves the hand, as what it is: wind over a spear, a
/// whoof of fire, a shiver of frost, a crackle of storm.
fn loose_sounds(
    me: Query<&arena_shared::protocol::Pos, With<crate::Me>>,
    shots: Query<(&arena_shared::protocol::Pos, &arena_shared::protocol::Projectile), Added<arena_shared::protocol::Projectile>>,
    mut queue: ResMut<SoundQueue>,
) {
    use arena_shared::protocol::FxColor;
    let Ok(me) = me.single() else { return };
    for (pos, projectile) in &shots {
        let def = projectile.class.def();
        let sfx = match def.look.as_ref().and_then(|look| look.bolts) {
            Some(FxColor::Fire) => Sfx::FireLoose,
            Some(FxColor::Frost | FxColor::Spirit) => Sfx::FrostLoose,
            Some(FxColor::Storm) => Sfx::StormLoose,
            _ if def.figure() == "javelinist" => Sfx::SpearLoose,
            _ => Sfx::Loose,
        };
        queue.push(sfx, 0.6 * earshot(pos.0.distance(me.0), 14.0));
    }
}

/// The gather before any attack: as a swing starts winding up, a faint breath of air, pitched
/// by the weapon's weight like the swing itself.
fn windup_sounds(
    me: Query<&arena_shared::protocol::Pos, With<crate::Me>>,
    attackers: Query<
        (Entity, &arena_shared::protocol::Pos, &arena_shared::protocol::ClassId, &arena_shared::protocol::AttackState),
        (Changed<arena_shared::protocol::AttackState>, With<arena_shared::protocol::PlayerId>),
    >,
    mut queue: ResMut<SoundQueue>,
    mut heard: Local<bevy::platform::collections::HashMap<Entity, u32>>,
) {
    let Ok(me) = me.single() else { return };
    for (attacker, pos, class, attack) in &attackers {
        let Some(swing) = attack.swing else { continue };
        // The first sighting only notes the swing: it may be long past.
        match heard.insert(attacker, swing.started_at) {
            Some(seen) if seen != swing.started_at => {
                let (gather, _, pitch) = attack_voice(*class);
                queue.push_pitched(gather, 0.4 * earshot(pos.0.distance(me.0), 10.0), 0.0, pitch);
            }
            _ => {}
        }
    }
}

/// Every dash nearby is a deeper, longer rush of air than a swing: the same swish, pitched down.
fn dash_sounds(
    me: Query<&arena_shared::protocol::Pos, With<crate::Me>>,
    dashes: Query<(Entity, &arena_shared::protocol::Pos, &arena_shared::protocol::Dashing), Changed<arena_shared::protocol::Dashing>>,
    mut queue: ResMut<SoundQueue>,
    mut heard: Local<bevy::platform::collections::HashMap<Entity, u32>>,
) {
    let Ok(me) = me.single() else { return };
    for (dasher, pos, dashing) in &dashes {
        let Some(dash) = dashing.0 else { continue };
        // Changed fires every tick of the dash: only its first sounds.
        if heard.insert(dasher, dash.started_at) == Some(dash.started_at) {
            continue;
        }
        queue.push_pitched(Sfx::Swing, 0.55 * earshot(pos.0.distance(me.0), 14.0), 0.0, 0.7);
    }
}

/// Below a quarter of your health, your own heart thuds under the mix, quicker the lower you
/// get: danger you can hear without looking at the bar.
fn heartbeat(
    time: Res<Time>,
    me: Query<(&arena_shared::protocol::Health, Option<&arena_shared::protocol::MaxHealth>, &arena_shared::protocol::ClassId), With<crate::Me>>,
    mut queue: ResMut<SoundQueue>,
    mut next: Local<f32>,
) {
    let Ok((health, max, class)) = me.single() else { return };
    let max = max.map_or(class.def().max_hp, |m| m.0);
    let fraction = health.0 as f32 / max.max(1) as f32;
    if !health.alive() || fraction >= 0.25 {
        return;
    }
    let now = time.elapsed_secs();
    if now < *next {
        return;
    }
    *next = now + 0.5 + 1.6 * fraction;
    queue.push(Sfx::Heart, 0.45 + (0.25 - fraction));
}

/// Every melee swing nearby cuts the air, once per strike: by the swing's own tick, never by
/// the component's change flag — replication marks that changed every update, which played a
/// roar of swishes from anyone standing near (the merchant at every camp).
fn swing_sounds(
    me: Query<&arena_shared::protocol::Pos, With<crate::Me>>,
    swings: Query<
        (Entity, &arena_shared::protocol::Pos, &arena_shared::protocol::ClassId, &arena_shared::protocol::LastSwing),
        (Changed<arena_shared::protocol::LastSwing>, With<arena_shared::protocol::PlayerId>),
    >,
    mut queue: ResMut<SoundQueue>,
    mut heard: Local<bevy::platform::collections::HashMap<Entity, u32>>,
) {
    let Ok(me) = me.single() else { return };
    for (swinger, pos, class, swing) in &swings {
        // The first sighting only notes the swing: it may be long past.
        match heard.insert(swinger, swing.tick) {
            Some(seen) if seen != swing.tick => {
                let (_, strike, pitch) = attack_voice(*class);
                queue.push_pitched(strike, 0.7 * earshot(pos.0.distance(me.0), 13.0), 0.0, pitch);
            }
            _ => {}
        }
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
            out.play(sound.kind, gain.min(1.0), sound.delay as f64 + k as f64 * STAGGER, sound.pitch);
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
            filter.frequency().set_value(180.0);
            // Barely there: a floor under the mix, never surf. It read as a loud waterfall at
            // 0.022 with a 0.013 swell.
            gain.gain().set_value(0.007);
            lfo.frequency().set_value(0.06);
            depth.gain().set_value(0.003);
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

        fn play(&mut self, kind: Sfx, gain: f32, delay: f64, pitch: f32) {
            use BiquadFilterType::{Bandpass, Highpass, Lowpass};
            use OscillatorType::{Sawtooth, Sine, Triangle};
            let now = self.ctx.current_time() + 0.01 + delay;
            let g = gain;
            let p = self.vary(0.12) * pitch;
            match kind {
                Sfx::Swing => {
                    // A swish falls in pitch as the weapon comes through; a heavy one (low
                    // pitch) takes proportionally longer to pass.
                    let dur = (0.14 / p as f64).min(0.24);
                    if let Some(e) = self.env(now, dur * 0.3, 0.13 * g, dur) {
                        self.noise(Bandpass, 1400.0 * p, 550.0 * p, 1.1, now, dur, &e);
                    }
                }
                Sfx::GlaiveSwing => {
                    // A broad, heavy cut: the body of the swish low and wide, a hiss off the
                    // long edge.
                    if let Some(e) = self.env(now, 0.045, 0.14 * g, 0.17) {
                        self.noise(Bandpass, 1150.0 * p, 460.0, 1.0, now, 0.17, &e);
                    }
                    if let Some(e) = self.env(now + 0.02, 0.01, 0.05 * g, 0.06) {
                        self.noise(Highpass, 3500.0, 2800.0, 0.8, now + 0.02, 0.06, &e);
                    }
                }
                Sfx::ShieldReady => {
                    // The shield coming up and the sword cocked back behind its rim: a creak of
                    // strap and leather, then a short rasp of steel clearing the rim. A gather
                    // with no air in it: a thrust is coiled, not swung.
                    if let Some(e) = self.env(now, 0.01, 0.05 * g, 0.07) {
                        self.noise(Lowpass, 420.0 * p, 300.0, 0.7, now, 0.07, &e);
                    }
                    if let Some(e) = self.env(now + 0.03, 0.005, 0.035 * g, 0.06) {
                        self.noise(Highpass, 5200.0, 5200.0, 1.0, now + 0.03, 0.06, &e);
                    }
                }
                Sfx::ShieldThrust => {
                    // In the order the body does it: the shield's face driven in first, a flat,
                    // broad thud (a low tone with a slap of air on it), then the sword a beat
                    // behind, a short narrow whisk of steel with the edge's ring held under it.
                    if let Some(e) = self.env(now, 0.004, 0.3 * g, 0.09) {
                        self.osc(Sine, 190.0 * p, 115.0, now, 0.09, &e);
                    }
                    if let Some(e) = self.env(now, 0.003, 0.14 * g, 0.045) {
                        self.noise(Lowpass, 900.0 * p, 350.0, 0.8, now, 0.045, &e);
                    }
                    if let Some(e) = self.env(now + 0.035, 0.012, 0.11 * g, 0.09) {
                        self.noise(Bandpass, 1900.0 * p, 800.0, 1.3, now + 0.035, 0.09, &e);
                    }
                    if let Some(e) = self.env(now + 0.045, 0.004, 0.03 * g, 0.08) {
                        self.osc(Triangle, 2050.0 * p, 2050.0 * p, now + 0.045, 0.08, &e);
                    }
                }
                Sfx::Wallop => {
                    // A body driven into stone: a deep dull thud, the crack of the hit off the
                    // wall on top of it, and grit sifting down after.
                    if let Some(e) = self.env(now, 0.005, 0.55 * g, 0.16) {
                        self.osc(Sine, 130.0 * p, 58.0, now, 0.16, &e);
                    }
                    if let Some(e) = self.env(now, 0.002, 0.3 * g, 0.05) {
                        self.noise(Bandpass, 1100.0 * p, 1100.0 * p, 0.9, now, 0.05, &e);
                    }
                    if let Some(e) = self.env(now + 0.04, 0.02, 0.08 * g, 0.18) {
                        self.noise(Highpass, 3800.0, 3800.0, 0.7, now + 0.04, 0.18, &e);
                    }
                }
                Sfx::MaulReady => {
                    // The whole body loading behind the haft: a long, low creak of wood under
                    // strain (a low saw, barely there), and the air beginning to move.
                    if let Some(e) = self.env(now, 0.12, 0.028 * g, 0.3) {
                        self.osc(Sawtooth, 96.0 * p, 104.0 * p, now, 0.3, &e);
                    }
                    if let Some(e) = self.env(now + 0.05, 0.2, 0.05 * g, 0.3) {
                        self.noise(Lowpass, 220.0 * p, 420.0 * p, 0.7, now + 0.05, 0.3, &e);
                    }
                }
                Sfx::MaulSwing => {
                    // The maul comes round: a long, low haul of air gathering to the blow; then
                    // the head lands, a deep thud in the earth with a crack of stone off it, the
                    // haft groaning, and grit rattling down after. Haul, land, settle: the mass
                    // is in the length of it.
                    if let Some(e) = self.env(now, 0.07, 0.14 * g, 0.2) {
                        self.noise(Lowpass, 560.0 * p, 180.0, 0.8, now, 0.2, &e);
                    }
                    if let Some(e) = self.env(now + 0.04, 0.004, 0.5 * g, 0.24) {
                        self.osc(Sine, 78.0 * p, 36.0, now + 0.04, 0.24, &e);
                    }
                    if let Some(e) = self.env(now + 0.04, 0.002, 0.22 * g, 0.05) {
                        self.noise(Bandpass, 640.0 * p, 640.0 * p, 1.1, now + 0.04, 0.05, &e);
                    }
                    if let Some(e) = self.env(now + 0.05, 0.01, 0.03 * g, 0.12) {
                        self.osc(Sawtooth, 88.0 * p, 80.0 * p, now + 0.05, 0.12, &e);
                    }
                    if let Some(e) = self.env(now + 0.09, 0.03, 0.06 * g, 0.22) {
                        self.noise(Highpass, 3200.0, 3200.0, 0.7, now + 0.09, 0.22, &e);
                    }
                }
                Sfx::KnifeReady => {
                    // Next to nothing: a catch of breath, and the knives turning in the hands,
                    // a whisper of steel on leather.
                    if let Some(e) = self.env(now, 0.02, 0.025 * g, 0.06) {
                        self.noise(Bandpass, 900.0 * p, 900.0 * p, 0.6, now, 0.06, &e);
                    }
                    if let Some(e) = self.env(now + 0.02, 0.004, 0.02 * g, 0.04) {
                        self.noise(Highpass, 6500.0, 6500.0, 0.8, now + 0.02, 0.04, &e);
                    }
                }
                Sfx::KnifeSwing => {
                    // Two cuts as they really come: one snick, then the other hand's a beat
                    // behind (the second cut's own moment), that one with a touch of steel
                    // ringing off it, so the pair reads as two blades and not an echo.
                    let beat = 5.0 / arena_shared::config::TICK_HZ as f64;
                    for (at, peak) in [(now, 0.1), (now + beat, 0.085)] {
                        if let Some(e) = self.env(at, 0.004, peak * g, 0.055) {
                            self.noise(Bandpass, 2400.0 * p, 1500.0, 1.5, at, 0.055, &e);
                        }
                    }
                    if let Some(e) = self.env(now + beat + 0.01, 0.002, 0.025 * g, 0.07) {
                        self.osc(Triangle, 3100.0 * p, 3100.0 * p, now + beat + 0.01, 0.07, &e);
                    }
                }
                Sfx::ClawSwing => {
                    // A ragged swipe: two rough bands of breath, overlapping and unclean.
                    if let Some(e) = self.env(now, 0.03, 0.12 * g, 0.15) {
                        self.noise(Bandpass, 950.0 * p, 420.0, 0.6, now, 0.15, &e);
                    }
                    if let Some(e) = self.env(now + 0.025, 0.02, 0.07 * g, 0.1) {
                        self.noise(Bandpass, 620.0 * p, 300.0, 0.5, now + 0.025, 0.1, &e);
                    }
                }
                Sfx::Windup => {
                    // A faint gather of air, rising as the weapon is raised.
                    let dur = (0.13 / p as f64).min(0.2);
                    if let Some(e) = self.env(now, dur * 0.7, 0.05 * g, dur) {
                        self.noise(Bandpass, 400.0 * p, 900.0 * p, 1.0, now, dur, &e);
                    }
                }
                Sfx::SpearLoose => {
                    // Wind whistling over the shaft as it takes flight: a narrow band of air
                    // easing down, longer than any other loose.
                    if let Some(e) = self.env(now, 0.015, 0.1 * g, 0.22) {
                        self.noise(Bandpass, 1900.0 * p, 1250.0, 2.6, now, 0.22, &e);
                    }
                    if let Some(e) = self.env(now, 0.008, 0.07 * g, 0.08) {
                        self.noise(Bandpass, 1100.0 * p, 1100.0 * p, 0.7, now, 0.08, &e);
                    }
                }
                Sfx::KindleReady => {
                    // Fire catching in the hand: a thin hiss swelling like a fuse, and the first
                    // small crackles as it takes.
                    if let Some(e) = self.env(now, 0.18, 0.04 * g, 0.24) {
                        self.noise(Bandpass, 2600.0 * p, 2600.0 * p, 1.6, now, 0.24, &e);
                    }
                    for (at, peak) in [(now + 0.1, 0.03), (now + 0.17, 0.025)] {
                        if let Some(e) = self.env(at, 0.001, peak * g, 0.02) {
                            self.noise(Highpass, 4200.0, 4200.0, 0.8, at, 0.02, &e);
                        }
                    }
                }
                Sfx::FireLoose => {
                    // A gout of fire tossed: it catches with a snap, then the whoomp of it
                    // leaving (low air with a soft thump under it), and crackles trailing off
                    // behind it. Snap, whoomp, crackle: how fire actually goes.
                    if let Some(e) = self.env(now, 0.001, 0.09 * g, 0.03) {
                        self.noise(Highpass, 3800.0, 3800.0, 0.8, now, 0.03, &e);
                    }
                    if let Some(e) = self.env(now + 0.01, 0.02, 0.17 * g, 0.2) {
                        self.noise(Lowpass, 720.0 * p, 260.0, 0.8, now + 0.01, 0.2, &e);
                    }
                    if let Some(e) = self.env(now + 0.01, 0.01, 0.1 * g, 0.12) {
                        self.osc(Sine, 120.0 * p, 62.0, now + 0.01, 0.12, &e);
                    }
                    for (at, peak) in [(now + 0.08, 0.035), (now + 0.13, 0.03), (now + 0.19, 0.022)] {
                        if let Some(e) = self.env(at, 0.001, peak * g, 0.018) {
                            self.noise(Bandpass, 3000.0 * p, 3000.0 * p, 1.4, at, 0.018, &e);
                        }
                    }
                }
                Sfx::Ignite => {
                    // A burning body going up all at once: a hard whoomp (a gulp of low air with
                    // a thump in it), a flare of hiss over the top, and a scatter of crackles
                    // dying away after.
                    if let Some(e) = self.env(now, 0.012, 0.42 * g, 0.3) {
                        self.noise(Lowpass, 900.0 * p, 200.0, 0.8, now, 0.3, &e);
                    }
                    if let Some(e) = self.env(now, 0.004, 0.3 * g, 0.16) {
                        self.osc(Sine, 105.0 * p, 48.0, now, 0.16, &e);
                    }
                    if let Some(e) = self.env(now + 0.01, 0.03, 0.09 * g, 0.22) {
                        self.noise(Highpass, 3000.0, 3000.0, 0.7, now + 0.01, 0.22, &e);
                    }
                    for (at, peak) in [(now + 0.12, 0.05), (now + 0.19, 0.04), (now + 0.27, 0.035), (now + 0.36, 0.025)] {
                        if let Some(e) = self.env(at, 0.001, peak * g, 0.02) {
                            self.noise(Bandpass, 2800.0 * p, 2800.0 * p, 1.4, at, 0.02, &e);
                        }
                    }
                }
                Sfx::FrostLoose => {
                    // A glassy shiver off the top, over a breath of air.
                    if let Some(e) = self.env(now, 0.004, 0.08 * g, 0.07) {
                        self.noise(Highpass, 4500.0, 3800.0, 1.2, now, 0.07, &e);
                    }
                    if let Some(e) = self.env(now, 0.006, 0.07 * g, 0.07) {
                        self.noise(Bandpass, 2600.0 * p, 2600.0 * p, 1.8, now, 0.07, &e);
                    }
                }
                Sfx::StormReady => {
                    // Static gathering up the staff: ticks of crackle coming quicker and
                    // louder, over a thin hum held under them.
                    for (i, at) in [0.0, 0.08, 0.14, 0.19, 0.23].into_iter().enumerate() {
                        if let Some(e) = self.env(now + at, 0.001, (0.02 + 0.006 * i as f32) * g, 0.015) {
                            self.noise(Highpass, 5000.0, 5000.0, 0.8, now + at, 0.015, &e);
                        }
                    }
                    if let Some(e) = self.env(now, 0.2, 0.025 * g, 0.28) {
                        self.osc(Sine, 160.0 * p, 160.0 * p, now, 0.28, &e);
                    }
                }
                Sfx::StormLoose => {
                    // A bolt let go: one hard crack (a click with a dry snap on it), a short
                    // buzz of charge under it, and a fizz of static after. No sweep anywhere:
                    // lightning is a crack, not a pew.
                    if let Some(e) = self.env(now, 0.001, 0.14 * g, 0.012) {
                        self.noise(Highpass, 6000.0, 6000.0, 0.8, now, 0.012, &e);
                    }
                    if let Some(e) = self.env(now + 0.004, 0.001, 0.16 * g, 0.035) {
                        self.noise(Bandpass, 2900.0 * p, 2900.0 * p, 1.3, now + 0.004, 0.035, &e);
                    }
                    if let Some(e) = self.env(now + 0.004, 0.003, 0.05 * g, 0.07) {
                        self.osc(Sawtooth, 92.0 * p, 92.0 * p, now + 0.004, 0.07, &e);
                    }
                    if let Some(e) = self.env(now + 0.03, 0.01, 0.04 * g, 0.1) {
                        self.noise(Highpass, 4200.0, 4200.0, 0.7, now + 0.03, 0.1, &e);
                    }
                }
                Sfx::Arc => {
                    // Lightning leaping to the next: a snap, and a short buzz of it finding its
                    // way, with a fizz off the end.
                    if let Some(e) = self.env(now, 0.001, 0.12 * g, 0.025) {
                        self.noise(Bandpass, 3200.0 * p, 3200.0 * p, 1.4, now, 0.025, &e);
                    }
                    if let Some(e) = self.env(now, 0.004, 0.045 * g, 0.06) {
                        self.osc(Sawtooth, 110.0 * p, 110.0 * p, now, 0.06, &e);
                    }
                    if let Some(e) = self.env(now + 0.02, 0.005, 0.03 * g, 0.05) {
                        self.noise(Highpass, 5000.0, 5000.0, 0.8, now + 0.02, 0.05, &e);
                    }
                }
                Sfx::Loose => {
                    // A puff of air leaving the hand: a still filter, so it never whistles into
                    // a pew.
                    if let Some(e) = self.env(now, 0.006, 0.11 * g, 0.08) {
                        self.noise(Bandpass, 1250.0 * p, 1250.0 * p, 0.7, now, 0.08, &e);
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
                    // A bright bell strike over a deep thump: steady pitches, nothing swept —
                    // a sweep reads as a laser, a struck pair of tones reads as metal.
                    for (f, peak, dur) in [(1180.0, 0.2, 0.09), (1770.0, 0.1, 0.06)] {
                        if let Some(e) = self.env(now, 0.001, peak * g, dur) {
                            self.osc(Triangle, f * p, f * p, now, dur, &e);
                        }
                    }
                    if let Some(e) = self.env(now, 0.003, 0.42 * g, 0.09) {
                        self.osc(Sine, 230.0 * p, 95.0, now, 0.09, &e);
                    }
                    if let Some(e) = self.env(now, 0.001, 0.14 * g, 0.035) {
                        self.noise(Highpass, 3400.0, 3000.0, 0.8, now, 0.035, &e);
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
                    // Quiet enough that a shower of gold charms instead of ringing.
                    if let Some(e) = self.env(now, 0.002, 0.13 * g, 0.07) {
                        self.osc(Triangle, 1320.0 * p, 1320.0 * p, now, 0.07, &e);
                    }
                    if let Some(e) = self.env(now + 0.05, 0.002, 0.11 * g, 0.1) {
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
                    // The crack off the top, so a blast snaps before it rumbles.
                    if let Some(e) = self.env(now, 0.001, 0.22 * g, 0.07) {
                        self.noise(Bandpass, 2200.0 * p, 900.0, 1.0, now, 0.07, &e);
                    }
                }
                Sfx::Slam => {
                    if let Some(e) = self.env(now, 0.004, 0.5 * g, 0.22) {
                        self.osc(Sine, 95.0 * p, 38.0, now, 0.22, &e);
                    }
                    if let Some(e) = self.env(now, 0.003, 0.3 * g, 0.18) {
                        self.noise(Lowpass, 450.0 * p, 120.0, 0.8, now, 0.18, &e);
                    }
                }
                Sfx::Horn => {
                    for (detune, peak) in [(1.0, 0.16), (1.006, 0.16), (1.5, 0.06)] {
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
                Sfx::Heart => {
                    for (at, peak) in [(now, 0.5), (now + 0.16, 0.3)] {
                        if let Some(e) = self.env(at, 0.01, peak * g, 0.12) {
                            self.osc(Sine, 64.0, 44.0, at, 0.12, &e);
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
