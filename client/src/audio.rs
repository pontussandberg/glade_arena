//! Sound, built from recordings: every effect is layered at the moment it plays out of a few
//! short public-domain recordings (`web/sfx/`, fetched and decoded at startup) in the order the
//! event happens — a maul blow is a heavy swish, then wood-and-iron landing, stone breaking,
//! grit rattling down — each pitched a little differently every time, with WebAudio synthesis
//! only for what no recording gives (a spectral hum, a sub thump, a horn). Gameplay systems
//! push `Sfx` onto the `SoundQueue` with a gain (loudness, usually by how close it is);
//! `play_sounds` drains it each frame.
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
            app.add_systems(Startup, web::warm_up);
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
    /// The revenant's glaive coming over: a broad, heavy cut with a hiss off the edge, a ghost
    /// of a hum in it, and the blade meeting the ground at the end.
    GlaiveSwing,
    /// The revenant gathering: the point scraping up off the dirt, the long haul of the blade
    /// overhead, and the spectral hum rising with it.
    GlaiveReady,
    /// The javelinist gathering: the shoulder wound back, leather creaking, a breath drawn.
    JavReady,
    /// The frost mage gathering: cold crystallizing round the staff, a glassy shimmer and ticks.
    FrostReady,
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
        "revenant" => (Sfx::GlaiveReady, Sfx::GlaiveSwing, 1.0),
        "javelinist" => (Sfx::JavReady, Sfx::Swing, 1.0),
        "frost_mage" => (Sfx::FrostReady, Sfx::Swing, 1.0),
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
                queue.push_pitched(gather, 0.55 * earshot(pos.0.distance(me.0), 10.0), 0.0, pitch);
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

    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;

    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::{JsFuture, spawn_local};
    use web_sys::{AudioBuffer, AudioContext, AudioContextState, AudioNode, BiquadFilterType, DynamicsCompressorNode, GainNode, OscillatorType, Response};

    /// The recordings, by file, under `sfx/` next to the page (see `sfx/CREDITS.txt` for where
    /// each is from; all CC0). A sound names one by its stem.
    const SAMPLES: &[&str] = &[
        // Air: swishes and swings (artisticdude), a rush of air (rubberduck).
        "swish_light_1.wav", "swish_light_2.wav", "swish_light_3.wav", "swish_heavy_1.wav", "swish_heavy_2.wav", "swish_heavy_3.wav",
        "swing_1.wav", "swing_2.wav", "air_01.ogg",
        // Steel: swords drawn, knives, clashes, rings (artisticdude, StarNinjas, Kenney, rubberduck).
        "unsheathe_1.wav", "unsheathe_2.wav", "ring.wav", "drawKnife1.ogg", "drawKnife2.ogg", "knifeSlice.ogg", "knifeSlice2.ogg",
        "sword_1.ogg", "sword_2.ogg", "sword_3.ogg", "clash_1.ogg", "clash_2.ogg", "clash_3.ogg", "metalClick.ogg", "chop.ogg",
        "metal_hit_01.ogg", "metal_hit_02.ogg",
        // Impacts (Kenney, rubberduck): flesh, plate, metal, wood, glass, stone.
        "impactSoft_heavy_000.ogg", "impactSoft_heavy_001.ogg", "impactSoft_medium_000.ogg", "impactSoft_medium_001.ogg",
        "impactPunch_medium_000.ogg", "impactPunch_medium_001.ogg", "impactPunch_medium_002.ogg", "impactPunch_heavy_000.ogg", "impactPunch_heavy_001.ogg",
        "impactMetal_heavy_000.ogg", "impactMetal_heavy_001.ogg", "impactMetal_medium_000.ogg", "impactMetal_medium_001.ogg",
        "impactPlate_heavy_000.ogg", "impactPlate_heavy_001.ogg", "impactPlate_medium_000.ogg",
        "impactWood_heavy_000.ogg", "impactWood_heavy_001.ogg", "impactGlass_light_000.ogg", "impactGlass_light_001.ogg", "impactGlass_medium_000.ogg",
        "impactMining_000.ogg", "impactMining_001.ogg", "impactGeneric_light_000.ogg", "impactBell_heavy_000.ogg",
        "hit_01.ogg", "hit_02.ogg", "stones_01.ogg", "stones_02.ogg", "glass_01.ogg", "groundhit.wav",
        // Leather, cloth, coin (Kenney, artisticdude).
        "cloth1.ogg", "cloth2.ogg", "clothBelt.ogg", "cloth_heavy.wav", "creak1.ogg", "creak2.ogg", "handleSmallLeather.ogg", "chainmail.wav",
        "handleCoins.ogg", "coins.wav",
        // Fire (USC / Sunset Editorial via the Internet Archive, themightyglider).
        "fire_whoosh.wav", "fire_pop.wav", "fire_bed.wav", "flame.ogg",
        // Storm (faxcorp, rubberduck).
        "zap_hit.wav", "zap_bloom.wav", "crackle.wav", "charge.wav", "boom.wav", "thunder_01.ogg",
        // Frost (bart, IgnasD, artisticdude).
        "ice.wav", "coldsnap.wav", "shatter_1.ogg", "shatter_2.ogg", "magic.wav",
    ];

    /// The decoded recordings, as they arrive (`load_samples`): a sound whose recording isn't
    /// here yet plays without that layer.
    type Samples = Rc<RefCell<HashMap<&'static str, AudioBuffer>>>;

    /// Everything sounds through this: each sound's own nodes feed the compressor (so a burst of
    /// kills squeezes instead of clipping), then the master gain, then out.
    pub(super) struct AudioOut {
        ctx: AudioContext,
        master: GainNode,
        squeeze: DynamicsCompressorNode,
        noise: AudioBuffer,
        samples: Samples,
        seed: u32,
        /// The wind is started once, the first time sound gets through.
        ambience_on: bool,
    }

    /// Built at startup (`warm_up`), so the recordings are in before the first blow.
    #[derive(Default)]
    pub(super) struct AudioLazy(Option<AudioOut>);

    const MASTER: f32 = 0.5;
    /// The most voices one kind of sound gets in a frame (the rest fold into their gain), and
    /// how far apart they start, so a cleared swarm crackles instead of phasing into one blast.
    const VOICES: usize = 3;
    const STAGGER: f64 = 0.014;

    /// Opens the audio (suspended until the page's first click, as browsers want) and starts
    /// fetching the recordings, so they're decoded by the time anything sounds.
    pub(super) fn warm_up(mut audio: NonSendMut<AudioLazy>) {
        if audio.0.is_none() {
            audio.0 = AudioOut::new();
        }
    }

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

    /// Fetches and decodes every recording, each into `samples` as it lands; one that fails is
    /// noted in the console and its layer stays silent.
    fn load_samples(ctx: &AudioContext, samples: &Samples) {
        for file in SAMPLES {
            let (ctx, samples) = (ctx.clone(), samples.clone());
            spawn_local(async move {
                let key = file.rsplit_once('.').map_or(*file, |(stem, _)| stem);
                match fetch_decode(&ctx, file).await {
                    Ok(buffer) => {
                        samples.borrow_mut().insert(key, buffer);
                    }
                    Err(e) => web_sys::console::warn_2(&format!("sfx/{file} didn't load").into(), &e),
                }
            });
        }
    }

    async fn fetch_decode(ctx: &AudioContext, file: &str) -> Result<AudioBuffer, JsValue> {
        let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
        let response: Response = JsFuture::from(window.fetch_with_str(&format!("sfx/{file}"))).await?.dyn_into()?;
        if !response.ok() {
            return Err(JsValue::from_str(&format!("HTTP {}", response.status())));
        }
        let bytes = JsFuture::from(response.array_buffer()?).await?;
        let decoded = JsFuture::from(ctx.decode_audio_data(&bytes.dyn_into()?)?).await?;
        decoded.dyn_into()
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
            let recordings: Samples = Rc::default();
            load_samples(&ctx, &recordings);
            Some(AudioOut { ctx, master, squeeze, noise, samples: recordings, seed: 1, ambience_on: false })
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

        /// One of `keys`, at random: the recordings come in variants so a flurry never repeats.
        fn pick<'a>(&mut self, keys: &[&'a str]) -> &'a str {
            keys[(self.r() * keys.len() as f32) as usize % keys.len()]
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

        /// An envelope that holds: up to `peak` at once, held `hold` seconds, then dying away over
        /// `release`. For a recording, whose own shape should be heard, not squeezed.
        fn held(&self, when: f64, peak: f32, hold: f64, release: f64) -> Option<GainNode> {
            let g = self.ctx.create_gain().ok()?;
            let p = g.gain();
            let _ = p.set_value_at_time(0.0001, when);
            let _ = p.linear_ramp_to_value_at_time(peak.max(0.0002), when + 0.003);
            let _ = p.set_value_at_time(peak.max(0.0002), when + 0.003 + hold);
            let _ = p.exponential_ramp_to_value_at_time(0.0001, when + 0.003 + hold + release);
            g.connect_with_audio_node(&self.squeeze).ok()?;
            Some(g)
        }

        /// A recording played into `into`: from `offset` seconds into it, for `dur` seconds (or
        /// to its end), at `rate` times its speed (its pitch following), from `when`. Nothing
        /// until it has loaded.
        fn clip(&self, key: &str, when: f64, offset: f64, dur: Option<f64>, rate: f32, into: &AudioNode) {
            let samples = self.samples.borrow();
            let Some(buffer) = samples.get(key) else { return };
            let Ok(source) = self.ctx.create_buffer_source() else { return };
            source.set_buffer(Some(buffer));
            source.playback_rate().set_value(rate.max(0.1));
            if source.connect_with_audio_node(into).is_ok() {
                let _ = match dur {
                    Some(dur) => source.start_with_when_and_grain_offset_and_grain_duration(when, offset, dur),
                    None => source.start_with_when_and_grain_offset(when, offset),
                };
            }
        }

        /// `clip`, through a filter at `freq`: a lowpass takes the air off a hit and leaves its
        /// weight; a highpass leaves only its grit.
        #[allow(clippy::too_many_arguments)]
        fn clip_through(&self, key: &str, when: f64, offset: f64, dur: Option<f64>, rate: f32, kind: BiquadFilterType, freq: f32, into: &AudioNode) {
            let Ok(filter) = self.ctx.create_biquad_filter() else { return };
            filter.set_type(kind);
            filter.frequency().set_value(freq);
            if filter.connect_with_audio_node(into).is_ok() {
                self.clip(key, when, offset, dur, rate, &filter);
            }
        }

        /// A whole recording at `peak`, from `when`, `rate` times its speed, held `hold` seconds
        /// and let go over `release`: the plain way most layers go in.
        fn sample(&mut self, key: &str, when: f64, peak: f32, rate: f32, hold: f64, release: f64) {
            if let Some(e) = self.held(when, peak, hold, release) {
                self.clip(key, when, 0.0, None, rate, &e);
            }
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

        /// Every sound is built from recordings layered in the order the event happens, each
        /// pitched a little differently every time, with synthesis only for what no recording
        /// gives (a spectral hum, a sub thump under a blow).
        fn play(&mut self, kind: Sfx, gain: f32, delay: f64, pitch: f32) {
            use BiquadFilterType::{Bandpass, Highpass, Lowpass};
            use OscillatorType::{Sawtooth, Sine, Triangle};
            let now = self.ctx.current_time() + 0.01 + delay;
            let g = gain;
            let p = self.vary(0.08) * pitch;
            const SWISH_LIGHT: [&str; 3] = ["swish_light_1", "swish_light_2", "swish_light_3"];
            const SWISH_HEAVY: [&str; 3] = ["swish_heavy_1", "swish_heavy_2", "swish_heavy_3"];
            const FLESH: [&str; 5] = ["impactPunch_medium_000", "impactPunch_medium_001", "impactPunch_medium_002", "impactSoft_medium_000", "impactSoft_medium_001"];
            const FLESH_HEAVY: [&str; 4] = ["impactPunch_heavy_000", "impactPunch_heavy_001", "impactSoft_heavy_000", "impactSoft_heavy_001"];
            const METAL_HEAVY: [&str; 2] = ["impactMetal_heavy_000", "impactMetal_heavy_001"];
            const WOOD_HEAVY: [&str; 2] = ["impactWood_heavy_000", "impactWood_heavy_001"];
            const PLATE_HEAVY: [&str; 2] = ["impactPlate_heavy_000", "impactPlate_heavy_001"];
            const STONES: [&str; 2] = ["stones_01", "stones_02"];
            const SWORDS: [&str; 3] = ["sword_1", "sword_2", "sword_3"];
            const CLASHES: [&str; 3] = ["clash_1", "clash_2", "clash_3"];
            match kind {
                Sfx::Swing => {
                    // A blade through the air: a real swish, a heavy weapon's lower and longer.
                    let key = self.pick(&SWISH_LIGHT);
                    self.sample(key, now, 0.38 * g, p, 0.1, 0.12);
                }
                Sfx::Windup => {
                    // Cloth and the first stir of air as the weapon is raised.
                    let cloth = self.pick(&["cloth1", "cloth2"]);
                    self.sample(cloth, now, 0.16 * g, p, 0.15, 0.1);
                }
                Sfx::Loose => {
                    // A shot leaving the hand: a short swing of air.
                    self.sample("swing_2", now, 0.34 * g, p * 1.05, 0.1, 0.1);
                }
                Sfx::GlaiveReady => {
                    // The glaive heaved up off the ground: steel scraping out of the dirt (a
                    // sword drawn, slow and low), the haul of air as the heavy blade comes up,
                    // and under it a spectral hum, two low tones a hair apart, swelling.
                    self.sample("unsheathe_1", now, 0.26 * g, 0.72 * p, 0.3, 0.12);
                    let haul = self.pick(&SWISH_HEAVY);
                    if let Some(e) = self.env(now + 0.05, 0.2, 0.16 * g, 0.34) {
                        self.clip(haul, now + 0.05, 0.0, None, 0.45 * p, &e);
                    }
                    for f in [110.0, 113.0] {
                        if let Some(e) = self.env(now + 0.05, 0.25, 0.02 * g, 0.32) {
                            self.osc(Sine, f * p, f * p, now + 0.05, 0.32, &e);
                        }
                    }
                }
                Sfx::GlaiveSwing => {
                    // The glaive coming over: a heavy swish, then the blade meeting the ground
                    // at the end of the arc, a dull body of a thud with grit under it, the ghost
                    // of the hum still in it.
                    let swish = self.pick(&SWISH_HEAVY);
                    self.sample(swish, now, 0.5 * g, 0.8 * p, 0.12, 0.1);
                    let land = self.pick(&["impactSoft_heavy_000", "impactSoft_heavy_001"]);
                    self.sample(land, now + 0.1, 0.5 * g, 0.72 * p, 0.2, 0.3);
                    let grit = self.pick(&STONES);
                    self.sample(grit, now + 0.11, 0.18 * g, 0.9 * p, 0.15, 0.3);
                    for f in [110.0, 113.0] {
                        if let Some(e) = self.env(now, 0.02, 0.016 * g, 0.2) {
                            self.osc(Sine, f * p, f * p, now, 0.2, &e);
                        }
                    }
                }
                Sfx::ShieldReady => {
                    // The shield coming up and the sword cocked back: leather and strap, and a
                    // short rasp of steel clearing the rim.
                    self.sample("handleSmallLeather", now, 0.3 * g, p, 0.15, 0.1);
                    if let Some(e) = self.held(now + 0.04, 0.2 * g, 0.12, 0.08) {
                        self.clip("unsheathe_2", now + 0.04, 0.0, Some(0.2), 1.1 * p, &e);
                    }
                }
                Sfx::ShieldThrust => {
                    // In the order the body does it: the shield's face driven in, a flat clap of
                    // plate, then the sword a beat behind, a short whisk of steel with its ring.
                    self.sample("impactPlate_medium_000", now, 0.55 * g, 0.9 * p, 0.15, 0.25);
                    let thrust = self.pick(&SWORDS);
                    self.sample(thrust, now + 0.035, 0.36 * g, 1.05 * p, 0.15, 0.15);
                    self.sample("ring", now + 0.05, 0.1 * g, 1.2 * p, 0.2, 0.25);
                }
                Sfx::MaulReady => {
                    // The whole body loading behind the haft: wood creaking under the strain,
                    // and the air beginning to move.
                    let creak = self.pick(&["creak1", "creak2"]);
                    self.sample(creak, now, 0.3 * g, 0.8 * p, 0.3, 0.15);
                    let haul = self.pick(&SWISH_HEAVY);
                    if let Some(e) = self.env(now + 0.08, 0.2, 0.14 * g, 0.3) {
                        self.clip(haul, now + 0.08, 0.0, None, 0.5 * p, &e);
                    }
                }
                Sfx::MaulSwing => {
                    // The maul comes round: a long low haul of air, then the head lands, a deep
                    // wooden-iron thud with stone breaking under it and grit rattling down, and
                    // a sub thump that the ear feels more than hears.
                    let haul = self.pick(&SWISH_HEAVY);
                    self.sample(haul, now, 0.45 * g, 0.62 * p, 0.14, 0.12);
                    let land = self.pick(&WOOD_HEAVY);
                    self.sample(land, now + 0.06, 0.65 * g, 0.7 * p, 0.2, 0.35);
                    let rock = self.pick(&["impactMining_000", "impactMining_001"]);
                    self.sample(rock, now + 0.07, 0.3 * g, 0.8 * p, 0.15, 0.3);
                    let grit = self.pick(&STONES);
                    self.sample(grit, now + 0.1, 0.3 * g, 0.85 * p, 0.2, 0.4);
                    if let Some(e) = self.env(now + 0.06, 0.004, 0.4 * g, 0.22) {
                        self.osc(Sine, 78.0 * p, 36.0, now + 0.06, 0.22, &e);
                    }
                }
                Sfx::KnifeReady => {
                    // Next to nothing: cloth, and the knives turning in the hands.
                    self.sample("cloth1", now, 0.16 * g, 1.1 * p, 0.1, 0.08);
                }
                Sfx::KnifeSwing => {
                    // Two cuts as they really come: one knife, then the other hand's a beat
                    // behind (the second cut's own moment).
                    let beat = 5.0 / arena_shared::config::TICK_HZ as f64;
                    self.sample("knifeSlice", now, 0.45 * g, p, 0.12, 0.1);
                    self.sample("knifeSlice2", now + beat, 0.4 * g, 1.04 * p, 0.12, 0.1);
                }
                Sfx::ClawSwing => {
                    // A ragged swipe: a swish with rags dragging through it.
                    let swish = self.pick(&SWISH_LIGHT);
                    self.sample(swish, now, 0.34 * g, 0.75 * p, 0.12, 0.1);
                    self.sample("cloth_heavy", now + 0.02, 0.28 * g, 0.9 * p, 0.15, 0.1);
                }
                Sfx::JavReady => {
                    // The shoulder wound back and the spear drawn: mail and leather shifting
                    // under the strain.
                    self.sample("chainmail", now, 0.28 * g, p, 0.2, 0.1);
                    self.sample("handleSmallLeather", now + 0.05, 0.18 * g, 0.9 * p, 0.15, 0.1);
                }
                Sfx::SpearLoose => {
                    // The throw: the effort of it, a low thump as the body comes through; the
                    // swing of the arm; then wind whistling over the shaft as it takes flight.
                    if let Some(e) = self.env(now, 0.006, 0.12 * g, 0.07) {
                        self.osc(Sine, 160.0 * p, 90.0, now, 0.07, &e);
                    }
                    self.sample("swing_1", now, 0.42 * g, 1.15 * p, 0.15, 0.1);
                    if let Some(e) = self.held(now + 0.05, 0.3 * g, 0.3, 0.25) {
                        self.clip("air_01", now + 0.05, 0.0, Some(0.6), 1.25 * p, &e);
                    }
                }
                Sfx::FrostReady => {
                    // Cold crystallizing round the staff: glass ticking as ice forms, and a
                    // glassy shimmer.
                    if let Some(e) = self.held(now, 0.16 * g, 0.2, 0.15) {
                        self.clip("ice", now, 0.0, Some(0.35), 1.3 * p, &e);
                    }
                    self.sample("glass_01", now + 0.1, 0.1 * g, 1.4 * p, 0.1, 0.15);
                    for f in [3200.0, 4800.0] {
                        if let Some(e) = self.env(now, 0.12, 0.01 * g, 0.24) {
                            self.osc(Triangle, f * p, f * p, now, 0.24, &e);
                        }
                    }
                }
                Sfx::FrostLoose => {
                    // A frostbolt let go: a snap of cold, ice crystallizing out along it, a
                    // chime of glass, and a breath of cold air going out with it.
                    self.sample("coldsnap", now, 0.3 * g, 1.2 * p, 0.1, 0.15);
                    self.sample("ice", now + 0.01, 0.5 * g, 1.1 * p, 0.3, 0.3);
                    let glass = self.pick(&["impactGlass_light_000", "impactGlass_light_001"]);
                    self.sample(glass, now + 0.02, 0.28 * g, 1.1 * p, 0.15, 0.2);
                    if let Some(e) = self.env(now + 0.02, 0.02, 0.05 * g, 0.16) {
                        self.noise(Lowpass, 900.0 * p, 500.0, 0.7, now + 0.02, 0.16, &e);
                    }
                }
                Sfx::KindleReady => {
                    // Fire catching in the hand: a lick of burning, and a pop as it takes.
                    let from = (self.r() * 1.2) as f64;
                    if let Some(e) = self.held(now, 0.26 * g, 0.3, 0.12) {
                        self.clip("fire_bed", now, from, Some(0.45), 1.1 * p, &e);
                    }
                    self.sample("flame", now + 0.05, 0.22 * g, 1.1 * p, 0.2, 0.1);
                }
                Sfx::FireLoose => {
                    // A gout of fire tossed: it catches with a pop, whooshes away, and
                    // crackles trailing off behind it.
                    self.sample("fire_pop", now, 0.35 * g, 1.05 * p, 0.15, 0.2);
                    self.sample("fire_whoosh", now + 0.01, 0.72 * g, 1.1 * p, 0.5, 0.4);
                    let from = (self.r() * 1.2) as f64;
                    if let Some(e) = self.held(now + 0.15, 0.22 * g, 0.3, 0.3) {
                        self.clip("fire_bed", now + 0.15, from, Some(0.6), p, &e);
                    }
                }
                Sfx::Ignite => {
                    // A burning body going up all at once: a hard whoomp of flame, the boom of
                    // it under, and a roar of burning dying back to crackles.
                    self.sample("fire_whoosh", now, 0.9 * g, 0.72 * p, 0.7, 0.4);
                    if let Some(e) = self.held(now, 0.5 * g, 0.3, 0.6) {
                        self.clip_through("boom", now, 0.0, Some(0.9), 1.1 * p, Lowpass, 500.0, &e);
                    }
                    let flesh = self.pick(&FLESH_HEAVY);
                    self.sample(flesh, now + 0.01, 0.3 * g, 0.8 * p, 0.15, 0.3);
                    if let Some(e) = self.held(now + 0.15, 0.4 * g, 0.6, 0.6) {
                        self.clip("fire_bed", now + 0.15, 0.0, Some(1.3), 0.95 * p, &e);
                    }
                }
                Sfx::StormReady => {
                    // Static gathering up the staff: a charge building, crackling.
                    if let Some(e) = self.held(now, 0.3 * g, 0.4, 0.15) {
                        self.clip("charge", now, 0.0, Some(0.55), 1.1 * p, &e);
                    }
                    if let Some(e) = self.held(now + 0.1, 0.18 * g, 0.3, 0.1) {
                        self.clip("crackle", now + 0.1, 0.0, Some(0.4), 1.3 * p, &e);
                    }
                }
                Sfx::StormLoose => {
                    // A bolt let go: one hard electric crack, and static fizzing off it.
                    self.sample("zap_hit", now, 0.62 * g, 1.1 * p, 0.2, 0.3);
                    if let Some(e) = self.held(now + 0.02, 0.3 * g, 0.15, 0.15) {
                        self.clip("crackle", now + 0.02, 0.1, Some(0.3), 1.4 * p, &e);
                    }
                }
                Sfx::Arc => {
                    // Lightning leaping to the next: a bloom of electricity with a snap in it.
                    if let Some(e) = self.held(now, 0.48 * g, 0.3, 0.3) {
                        self.clip("zap_bloom", now, 0.0, Some(0.6), 1.25 * p, &e);
                    }
                    if let Some(e) = self.held(now, 0.25 * g, 0.15, 0.15) {
                        self.clip("crackle", now, 0.2, Some(0.3), 1.5 * p, &e);
                    }
                }
                Sfx::Hit => {
                    // A blow landing on a body.
                    let key = self.pick(&FLESH);
                    self.sample(key, now, 0.5 * g, p, 0.15, 0.25);
                }
                Sfx::Crit => {
                    // A crit: heavy metal on a heavy body, with steel ringing off it.
                    let metal = self.pick(&METAL_HEAVY);
                    self.sample(metal, now, 0.5 * g, 1.05 * p, 0.2, 0.3);
                    let flesh = self.pick(&FLESH_HEAVY);
                    self.sample(flesh, now, 0.5 * g, 0.95 * p, 0.15, 0.3);
                    let clash = self.pick(&CLASHES);
                    self.sample(clash, now + 0.01, 0.22 * g, 1.1 * p, 0.2, 0.3);
                    self.sample("ring", now + 0.02, 0.18 * g, 1.3 * p, 0.3, 0.3);
                }
                Sfx::Hurt => {
                    // A blow we take: heavier and closer than one we see, with a thump in the chest.
                    let flesh = self.pick(&FLESH_HEAVY);
                    self.sample(flesh, now, 0.65 * g, 0.85 * p, 0.2, 0.3);
                    if let Some(e) = self.env(now, 0.005, 0.3 * g, 0.13) {
                        self.osc(Sine, 150.0 * p, 55.0, now, 0.13, &e);
                    }
                }
                Sfx::Kill => {
                    // A monster going down: a heavy soft fall, and the body hitting the ground.
                    let fall = self.pick(&["impactSoft_heavy_000", "impactSoft_heavy_001"]);
                    self.sample(fall, now, 0.6 * g, 0.75 * p, 0.2, 0.35);
                    let flesh = self.pick(&FLESH);
                    self.sample(flesh, now + 0.03, 0.35 * g, 0.8 * p, 0.15, 0.25);
                    self.sample("groundhit", now + 0.08, 0.3 * g, 0.8 * p, 0.15, 0.3);
                }
                Sfx::BossKill => {
                    // A boss going down: a boom, the earth taking the weight, and thunder
                    // rolling away after.
                    self.sample("boom", now, 0.9 * g, 0.85 * p, 0.8, 1.0);
                    let fall = self.pick(&WOOD_HEAVY);
                    self.sample(fall, now + 0.05, 0.6 * g, 0.5 * p, 0.3, 0.5);
                    if let Some(e) = self.held(now + 0.3, 0.4 * g, 1.0, 1.2) {
                        self.clip("thunder_01", now + 0.3, 0.0, Some(2.5), 0.8 * p, &e);
                    }
                }
                Sfx::Heal => {
                    // One soft chime of glass, warm under it.
                    self.sample("impactGlass_light_000", now, 0.22 * g, 1.6 * p, 0.2, 0.3);
                    self.sample("glass_01", now + 0.08, 0.16 * g, 1.9 * p, 0.2, 0.3);
                    if let Some(e) = self.env(now, 0.02, 0.1 * g, 0.3) {
                        self.osc(Sine, 660.0 * p, 680.0 * p, now, 0.3, &e);
                    }
                }
                Sfx::Coin => {
                    // Coins handled: quiet enough that a shower of gold charms instead of ringing.
                    let key = self.pick(&["handleCoins", "coins"]);
                    self.sample(key, now, 0.3 * g, 1.1 * p, 0.3, 0.2);
                }
                Sfx::Thunder => {
                    // Lightning striking: the crack, and the thunder.
                    self.sample("zap_hit", now, 0.5 * g, 0.9 * p, 0.2, 0.3);
                    self.sample("thunder_01", now + 0.02, 0.9 * g, p, 1.5, 1.2);
                }
                Sfx::Explosion => {
                    // A keg, a meteor: the boom, fire going up with it, and stone and earth
                    // coming down after.
                    self.sample("boom", now, 0.95 * g, p, 0.8, 1.0);
                    self.sample("fire_whoosh", now + 0.02, 0.5 * g, 0.9 * p, 0.5, 0.4);
                    let grit = self.pick(&STONES);
                    self.sample(grit, now + 0.15, 0.4 * g, 0.9 * p, 0.3, 0.5);
                    let flesh = self.pick(&FLESH_HEAVY);
                    self.sample(flesh, now, 0.4 * g, 0.7 * p, 0.15, 0.3);
                }
                Sfx::Slam => {
                    // A nova going off: a heavy blow into the ground, stone breaking, a sub thump.
                    let land = self.pick(&WOOD_HEAVY);
                    self.sample(land, now, 0.6 * g, 0.55 * p, 0.2, 0.4);
                    self.sample("groundhit", now, 0.5 * g, 0.7 * p, 0.15, 0.3);
                    let grit = self.pick(&STONES);
                    self.sample(grit, now + 0.05, 0.35 * g, 0.8 * p, 0.3, 0.5);
                    if let Some(e) = self.env(now, 0.004, 0.45 * g, 0.22) {
                        self.osc(Sine, 95.0 * p, 38.0, now, 0.22, &e);
                    }
                }
                Sfx::Wallop => {
                    // A body driven into stone: plate and flesh meeting a wall at once, and grit
                    // knocked off it sifting down.
                    let plate = self.pick(&PLATE_HEAVY);
                    self.sample(plate, now, 0.6 * g, 0.8 * p, 0.2, 0.3);
                    let flesh = self.pick(&FLESH_HEAVY);
                    self.sample(flesh, now, 0.5 * g, 0.85 * p, 0.15, 0.3);
                    let grit = self.pick(&STONES);
                    self.sample(grit, now + 0.08, 0.35 * g, 0.9 * p, 0.3, 0.5);
                }
                Sfx::Horn => {
                    // A horn blown: two reeds a hair apart with a fifth over them, swelling.
                    for (detune, peak) in [(1.0, 0.16), (1.006, 0.16), (1.5, 0.06)] {
                        if let Some(e) = self.env(now, 0.22, peak * g, 1.0) {
                            let f = 98.0 * detune;
                            self.osc(Sawtooth, f, f * 1.02, now, 1.0, &e);
                        }
                    }
                }
                Sfx::BossHorn => {
                    // The deep horn, and a bell struck under it.
                    for (detune, peak) in [(1.0, 0.26), (1.05, 0.2), (1.01, 0.22)] {
                        if let Some(e) = self.env(now, 0.4, peak * g, 1.8) {
                            let f = 55.0 * detune;
                            self.osc(Sawtooth, f, f * 0.97, now, 1.8, &e);
                        }
                    }
                    self.sample("impactBell_heavy_000", now + 0.1, 0.35 * g, 0.6, 0.8, 1.2);
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
            // Keep the unused filter kinds named, for the layers that want them.
            let _ = (Bandpass, Highpass);
        }
    }
}
