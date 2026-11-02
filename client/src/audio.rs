//! Sound, built from recordings: every effect is layered at the moment it plays out of a few
//! short public-domain recordings (`web/sfx/`, fetched and decoded at startup) in the order the
//! event happens — a maul blow is a heavy swish, then wood-and-iron landing, stone breaking,
//! grit rattling down — each pitched a little differently every time, with WebAudio synthesis
//! only for what no recording gives (a spectral hum, a sub thump, a horn). Gameplay systems
//! push `Sfx` onto the `SoundQueue` with a gain, and for one that happens somewhere, where and
//! how far it carries (`Reach`); `play_sounds` drains it each frame, placing each sound round
//! us: fainter and duller the farther off, and off to the side it's on.
//!
//! The browser keeps an `AudioContext` suspended until the page is interacted with, so the
//! queue is dropped (and a resume asked for) until it's running: nothing can sound before the
//! first click anyway. Only the web client sounds; native builds drain the queue silently.

use bevy::prelude::*;

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SoundQueue>();
        app.add_systems(Update, (swing_sounds, windup_sounds, loose_sounds, dash_sounds, footstep_sounds, heartbeat));
        #[cfg(target_family = "wasm")]
        {
            app.init_non_send::<web::AudioLazy>();
            app.add_systems(Startup, web::warm_up);
            app.add_systems(Update, (web::play_sounds, web::apply_volume.run_if(resource_changed::<crate::settings::Settings>)));
        }
        #[cfg(not(target_family = "wasm"))]
        app.add_systems(Update, |mut queue: ResMut<SoundQueue>| {
            queue.sounds.clear();
            queue.hushes.clear();
        });
    }
}

/// What a foot comes down on.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Surface {
    Grass,
    Stone,
    Wood,
}

/// How a figure's dash sounds: what it's wearing and what carries it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum DashVoice {
    /// A plain rush of air.
    Air,
    /// The revenant slipping between worlds: air, and the hum of the other side.
    Spectral,
}

/// The sounds there are.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Sfx {
    /// A blade (or claw) cutting the air: the fallback swing for a figure without its own,
    /// pitched by the weapon's weight.
    Swing,
    /// A foot coming down.
    Step(Surface),
    /// A dash, as the figure dashes.
    Dash(DashVoice),
    /// The revenant's glaive landing as its lead foot stomps down: one short, heavy thunk of
    /// blade and foot into the ground, and on its heels the rift opening where it struck: the
    /// earth groaning open like a heavy gate, briefly.
    GlaiveSwing,
    /// The revenant's glaive coming over and down, just before it lands (`AttackVoice::late`): a
    /// long, heavy haft pushing through the air, the coat flapping with it.
    GlaiveWhoosh,
    /// The javelinist gathering: a soft swish as it goes into stance, just before the throw.
    JavReady,
    /// The javelinist's throw: the arm lashing the spear away, a hard whoosh.
    SpearThrow,
    /// The frost mage gathering: crackling frost channelled up, lingering on as long as the
    /// windup lasts, ending under the cast (`FrostCast`) as the bolt goes.
    FrostReady,
    /// The frost mage's bolt going: a spell's climax and icy tail.
    FrostCast,
    /// A zombie's claws: a ragged, breathy swipe, nothing clean about it.
    ClawSwing,
    /// The faint gather of air before any attack goes off.
    Windup,
    /// A shot loosed, with nothing more telling to say about it.
    Loose,
    /// A thrown spear that missed, striking what isn't a fighter (a wall, a rock, the ground): its
    /// iron tip chipping on grimy stone, high, short and very faint, nothing more.
    SpearTing,
    /// A gout of fire tossed: it catches with a snap, whoomps away, and crackles after.
    FireLoose,
    /// A frostbolt leaving the hand: a hard close snap of ice with a deep crack and a thump of
    /// weight under it, crackling trailing after, the snap coming back off the walls.
    FrostLoose,
    /// A storm bolt let go: one hard crack, a buzz of charge under it, a fizz of static after.
    StormLoose,
    /// Lightning arcing from one to the next.
    Arc,
    /// A blow landing on someone else.
    Hit,
    /// A crit of ours landing.
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
    /// A frost nova: ice shattering out across the ground.
    FrostNova,
    /// A frostbolt shattering where it hit or ended: a punch of ice breaking.
    Shatter,
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
pub struct Sound {
    kind: Sfx,
    /// How loud it is up close, 0 to 1.
    gain: f32,
    delay: f32,
    /// A pitch multiplier: a kill chain climbs, so a cleared swarm sings up a little scale.
    pitch: f32,
    /// How long ago (seconds) it began, for one heard late: it plays from that far in.
    begun: f32,
    /// Whose doing it is, if anyone's: ours carries as far as our own doing does (`Reach::Own`),
    /// and a windup's lingering sound is found by its release (or hush) through it.
    by: Option<lightyear::prelude::PeerId>,
    /// Where it happens (on the gameplay plane) and how far it carries; `None` for one that's
    /// ours alone, heard as it is (a blow we take, our heart).
    at: Option<(Vec2, Reach)>,
}

impl Sound {
    /// Happening at `at`, heard as far as `reach` says.
    pub fn from(&mut self, at: Vec2, reach: Reach) -> &mut Self {
        self.at = Some((at, reach));
        self
    }

    /// Starting `delay` seconds from now.
    pub fn after(&mut self, delay: f32) -> &mut Self {
        self.delay = delay;
        self
    }

    pub fn pitched(&mut self, pitch: f32) -> &mut Self {
        self.pitch = pitch;
        self
    }

    /// `owner`'s doing (`Sound::by`).
    pub fn by(&mut self, owner: impl Into<Option<lightyear::prelude::PeerId>>) -> &mut Self {
        self.by = owner.into();
        self
    }

    /// Due `ago` seconds from now: later if it's positive; if it's negative, begun that long
    /// ago, heard from that far in.
    pub fn due(&mut self, ago: f32) -> &mut Self {
        self.delay = ago.max(0.0);
        self.begun = (-ago).max(0.0);
        self
    }
}

/// What wants to sound this frame, and whose lingering windup sounds to hush (a windup cut
/// off, or one whose release never came).
#[derive(Resource, Default)]
pub struct SoundQueue {
    sounds: Vec<Sound>,
    hushes: Vec<lightyear::prelude::PeerId>,
}

impl SoundQueue {
    /// A sound at `gain` (0 to 1, as loud as it is up close), to be placed and timed with
    /// `Sound`'s methods. One too faint to hear once placed is dropped.
    pub fn add(&mut self, kind: Sfx, gain: f32) -> &mut Sound {
        self.sounds.push(Sound { kind, gain: gain.min(1.0), delay: 0.0, pitch: 1.0, begun: 0.0, by: None, at: None });
        self.sounds.last_mut().expect("just pushed")
    }

    pub fn push(&mut self, kind: Sfx, gain: f32) {
        self.add(kind, gain);
    }

    /// `id`'s lingering windup sound, if one is playing, fades out.
    pub fn hush(&mut self, id: lightyear::prelude::PeerId) {
        self.hushes.push(id);
    }
}

/// How far a sound carries, by what it is.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Reach {
    /// Steps, cloth, a gather: heard close by.
    Small,
    /// Swings, casts, shots, impacts, hits.
    Combat,
    /// Explosions, thunder, novas, a boss.
    Big,
    /// Our own doing (a shot's impact, a spell cast far off): heard much farther, and never
    /// fading out entirely.
    Own,
    /// Heard across the whole map, never fading out: a moment everyone must know of.
    #[allow(dead_code)]
    Global,
}

impl Reach {
    /// How far (meters) it carries before it's gone, and the least it ever fades to.
    fn span(self) -> (f32, f32) {
        match self {
            Reach::Small => (15.0, 0.0),
            Reach::Combat => (35.0, 0.0),
            Reach::Big => (70.0, 0.0),
            Reach::Own => (100.0, 0.015),
            Reach::Global => (f32::INFINITY, 0.25),
        }
    }
}

/// Within this (meters) a sound is as loud as it gets; past it, it halves each time the
/// distance doubles, as sound does in the open.
const NEAR: f32 = 3.0;

/// The share of its reach over which a sound fades out to nothing at the end of it.
const FADE_OUT: f32 = 0.3;

/// How loud a sound `away` meters off is, 0 to 1: full up close, halving with each doubling of
/// the distance, faded smoothly away over the end of its reach to the least it's heard at.
pub fn heard(away: f32, reach: Reach) -> f32 {
    let (far, least) = reach.span();
    let open = NEAR / away.max(NEAR);
    let t = if far.is_finite() { ((far - away) / (FADE_OUT * far)).clamp(0.0, 1.0) } else { 1.0 };
    (open * t * t * (3.0 - 2.0 * t)).max(least)
}

/// How heavy a class's weapon sounds, from how slowly it attacks: a knife rides high and quick,
/// a maul low and long. A pitch multiplier around 1.
fn weapon_pitch(class: arena_shared::protocol::ClassId) -> f32 {
    (40.0 / class.def().attack.attack_ticks as f32).clamp(0.65, 1.35)
}

/// A class's auto-attack as it's heard. Every class's plays the same way, each part by the
/// attack clock so they line up however late the news of the attack came: `gather` as the
/// windup starts (`windup_sounds`), `late` towards its end, and `release` on the tick it goes
/// off (`release_sound`).
struct AttackVoice {
    /// As the windup starts. One that lingers (the frost mage's) lasts until the attack goes
    /// off, however long a shot is held drawn.
    gather: Option<Sfx>,
    /// Late in the windup: what, and how long (seconds) before the attack goes off it starts.
    late: Option<(Sfx, f32)>,
    /// As the attack goes off.
    release: Sfx,
    /// The pitch all of it plays at.
    pitch: f32,
}

/// What a class's auto-attack sounds like. Each hero's is engineered for its weapon and the
/// fantasy behind it; a figure without its own gets the plain gather and swing, pitched by its
/// weapon's weight.
fn attack_voice(class: arena_shared::protocol::ClassId) -> AttackVoice {
    let voice = |gather, late, release| AttackVoice { gather, late, release, pitch: 1.0 };
    match class.def().figure() {
        // Silent until the glaive comes over and down, its whoosh loudest as the blow lands
        // (too late for the air the blade pushes on the way down, the blow's own sound).
        "revenant" => voice(None, Some((Sfx::GlaiveWhoosh, 0.3)), Sfx::GlaiveSwing),
        // Silent until it goes into stance, as far before the throw as `jav_draw` is before
        // `jav_whoosh` in the recording they're cut from.
        "javelinist" => voice(None, Some((Sfx::JavReady, 0.27)), Sfx::SpearThrow),
        // Frost channelled for as long as the windup lasts, then the spell's climax.
        "frost_mage" => voice(Some(Sfx::FrostReady), None, Sfx::FrostCast),
        "zombie" => voice(Some(Sfx::Windup), None, Sfx::ClawSwing),
        _ => AttackVoice { pitch: weapon_pitch(class), ..voice(Some(Sfx::Windup), None, Sfx::Swing) },
    }
}

/// `class`'s attack going off at `tick` (`owner`'s, at `at`): its release, by the attack clock,
/// from partway in if the news of it came late.
fn release_sound(queue: &mut SoundQueue, clock: Option<&crate::feedback::AttackClock>, class: arena_shared::protocol::ClassId, at: Vec2, tick: u32, owner: lightyear::prelude::PeerId, is_me: bool) {
    let voice = attack_voice(class);
    let due = clock.map_or(0.0, |clock| (tick as f32 - clock.now(is_me)) / arena_shared::config::TICK_HZ as f32);
    queue.add(voice.release, 0.42).from(at, Reach::Combat).pitched(voice.pitch).due(due).by(owner);
}

/// Every shot loosed nearby sounds as it leaves the hand: a hero's as its attack's release
/// (`release_sound`; a shot has no swing for `swing_sounds` to hear), a monster's as its bolts
/// look: a whoof of fire, a shiver of frost, a crackle of storm.
fn loose_sounds(
    clock: Option<crate::feedback::AttackClock>,
    me: Query<&arena_shared::protocol::PlayerId, With<crate::Me>>,
    shots: Query<(&arena_shared::protocol::Pos, &arena_shared::protocol::Projectile), Added<arena_shared::protocol::Projectile>>,
    mut queue: ResMut<SoundQueue>,
) {
    use arena_shared::protocol::FxColor;
    let my_id = me.single().ok().map(|id| id.0);
    for (pos, projectile) in &shots {
        let def = projectile.class.def();
        if def.playable {
            release_sound(&mut queue, clock.as_ref(), projectile.class, pos.0, projectile.spawn_tick, projectile.owner, my_id == Some(projectile.owner));
            continue;
        }
        let sfx = match def.look.as_ref().and_then(|look| look.bolts) {
            Some(FxColor::Fire) => Sfx::FireLoose,
            Some(FxColor::Frost | FxColor::Spirit) => Sfx::FrostLoose,
            Some(FxColor::Storm) => Sfx::StormLoose,
            _ => Sfx::Loose,
        };
        queue.add(sfx, 0.36).from(pos.0, Reach::Combat);
    }
}

/// How old (ticks) a swing first seen on a fighter may be and still sound: one that started
/// just now is the fighter's own (its first after spawning, say), not stale news that came
/// with the fighter.
const FRESH_TICKS: f32 = 10.0;

/// Whether a swing seen on `attacker` at `tick` is new: not one sounded already (replication
/// marks the component changed every update), and, the first time the fighter is seen, only
/// if it's fresh.
fn unheard(heard: &mut bevy::platform::collections::HashMap<Entity, u32>, attacker: Entity, tick: u32, now: Option<f32>) -> bool {
    match heard.insert(attacker, tick) {
        Some(seen) => seen != tick,
        None => now.is_some_and(|now| now - tick as f32 <= FRESH_TICKS),
    }
}

/// The gather before any attack: as a swing starts winding up, each figure's own sound of it.
fn windup_sounds(
    clock: Option<crate::feedback::AttackClock>,
    attackers: Query<
        (Entity, &arena_shared::protocol::PlayerId, &arena_shared::protocol::Pos, &arena_shared::protocol::ClassId, &arena_shared::protocol::AttackState, Has<crate::Me>),
        Changed<arena_shared::protocol::AttackState>,
    >,
    mut queue: ResMut<SoundQueue>,
    mut heard: Local<bevy::platform::collections::HashMap<Entity, u32>>,
    time: Res<Time>,
    fighters: Query<(), With<arena_shared::protocol::PlayerId>>,
    // Each fighter's windup under way: its swing's start, the tick it goes off, and whose.
    mut winding: Local<bevy::platform::collections::HashMap<Entity, (u32, u32, lightyear::prelude::PeerId)>>,
    // Windups that went off: hushed a moment later in case their release was never heard
    // (`Time` elapsed seconds), unless the fighter has started another by then.
    mut lapsed: Local<bevy::platform::collections::HashMap<Entity, (f32, lightyear::prelude::PeerId)>>,
) {
    // How long (seconds) a windup that went off waits on its release before it's hushed.
    const RELEASE_GRACE: f32 = 0.5;
    let now = time.elapsed_secs();
    for (attacker, id, pos, class, attack, is_me) in &attackers {
        // A windup lost before it went off (knocked up, or the fighter gone) hushes what still
        // sounds of it; one that went off leaves it to its release, as a lingering sound lasts
        // until then however long the shot is held drawn.
        if let Some(&(started, release, _)) = winding.get(&attacker)
            && attack.swing.is_none_or(|swing| swing.started_at != started)
        {
            winding.remove(&attacker);
            if clock.as_ref().is_some_and(|clock| clock.now(is_me) < release as f32) {
                queue.hush(id.0);
            } else {
                lapsed.insert(attacker, (now + RELEASE_GRACE, id.0));
            }
        }
        let Some(swing) = attack.swing else { continue };
        lapsed.remove(&attacker);
        winding.insert(attacker, (swing.started_at, swing.releases_at(&class.def().attack), id.0));
        if unheard(&mut heard, attacker, swing.started_at, clock.as_ref().map(|clock| clock.now(is_me))) {
            let voice = attack_voice(*class);
            if let Some(gather) = voice.gather {
                queue.add(gather, 0.4).from(pos.0, Reach::Small).pitched(voice.pitch).by(id.0);
            }
            if let Some((late, lead)) = voice.late {
                // By the clock, the tick the attack goes off less its lead, however late the
                // news of it came.
                let windup_ticks = swing.windup_ticks(&class.def().attack);
                let windup = windup_ticks as f32 / arena_shared::config::TICK_HZ as f32;
                let left = clock.as_ref().map_or(windup, |clock| (((swing.started_at + windup_ticks) as f32 - clock.now(is_me)) / arena_shared::config::TICK_HZ as f32).clamp(0.0, windup));
                queue.add(late, 0.42).from(pos.0, Reach::Combat).after((left - lead).max(0.0)).pitched(voice.pitch).by(id.0);
            }
        }
    }
    // A fighter gone mid-windup, or a release never heard: nothing is left sounding.
    winding.retain(|&attacker, &mut (_, _, id)| {
        let here = fighters.contains(attacker);
        if !here {
            queue.hush(id);
        }
        here
    });
    lapsed.retain(|_, &mut (due, id)| {
        if now >= due {
            queue.hush(id);
        }
        now < due
    });
}

/// Every dash nearby sounds as the figure dashing does: plate on the move, cloth, a gale.
fn dash_sounds(
    dashes: Query<(Entity, &arena_shared::protocol::Pos, &arena_shared::protocol::ClassId, &arena_shared::protocol::Dashing), Changed<arena_shared::protocol::Dashing>>,
    mut queue: ResMut<SoundQueue>,
    mut heard: Local<bevy::platform::collections::HashMap<Entity, u32>>,
) {
    for (dasher, pos, class, dashing) in &dashes {
        let Some(dash) = dashing.0 else { continue };
        // Changed fires every tick of the dash: only its first sounds.
        if heard.insert(dasher, dash.started_at) == Some(dash.started_at) {
            continue;
        }
        let voice = match class.def().figure() {
            "revenant" => DashVoice::Spectral,
            _ => DashVoice::Air,
        };
        queue.add(Sfx::Dash(voice), 0.6).from(pos.0, Reach::Combat);
    }
}

/// Every foot that comes down nearby (`rig::Footfalls`, counted as the figures stride) sounds
/// on what it lands on: grass, stone or planks. Our own are quiet and close; a big figure's are
/// lower; a monster's shuffle is softer still.
fn footstep_sounds(
    map: Res<crate::rooms::CurrentMap>,
    steps: Query<(&arena_shared::protocol::Pos, &arena_shared::protocol::ClassId, Has<crate::Me>), Changed<crate::rig::Footfalls>>,
    mut queue: ResMut<SoundQueue>,
) {
    use arena_shared::map::{Map, Tile};
    for (pos, class, is_me) in &steps {
        let surface = match map.map().get(Map::tile_of(pos.0)) {
            Tile::Bridge | Tile::Planks => Surface::Wood,
            Tile::Path | Tile::Wall | Tile::Rock => Surface::Stone,
            _ => Surface::Grass,
        };
        let def = class.def();
        let gain = if is_me { 0.3 } else { 0.26 * if def.mob.is_some() { 0.6 } else { 1.0 } };
        queue.add(Sfx::Step(surface), gain).from(pos.0, Reach::Small).pitched((1.0 / def.size()).clamp(0.75, 1.25));
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

/// Every melee swing nearby strikes (`release_sound`), once per swing: by the swing's own tick, never by the
/// component's change flag — replication marks that changed every update, which played a roar
/// of swishes from anyone standing near (the merchant at every camp).
fn swing_sounds(
    clock: Option<crate::feedback::AttackClock>,
    swings: Query<
        (Entity, &arena_shared::protocol::PlayerId, &arena_shared::protocol::Pos, &arena_shared::protocol::ClassId, &arena_shared::protocol::LastSwing, Has<crate::Me>),
        Changed<arena_shared::protocol::LastSwing>,
    >,
    mut queue: ResMut<SoundQueue>,
    mut heard: Local<bevy::platform::collections::HashMap<Entity, u32>>,
) {
    for (swinger, id, pos, class, swing, is_me) in &swings {
        if unheard(&mut heard, swinger, swing.tick, clock.as_ref().map(|clock| clock.now(is_me))) {
            release_sound(&mut queue, clock.as_ref(), *class, pos.0, swing.tick, id.0, is_me);
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
    use web_sys::{AudioBuffer, AudioBufferSourceNode, AudioContext, AudioContextState, AudioNode, AudioScheduledSourceNode, BiquadFilterType, DynamicsCompressorNode, GainNode, OscillatorType, Response};

    /// The recordings, by file, under `sfx/` next to the page (see `sfx/CREDITS.txt` for where
    /// each is from; CC0 but the author's own and the CC BY ones, which must stay credited
    /// there). A sound names one by its stem.
    const SAMPLES: &[&str] = &[
        // Air: swishes and swings (artisticdude).
        "swish_light_1.wav", "swish_light_2.wav", "swish_light_3.wav", "swish_heavy_1.wav", "swish_heavy_2.wav", "swish_heavy_3.wav",
        "swing_2.wav",
        // Spear swings (Still North Media).
        "spear_swing_1.wav", "spear_swing_2.wav", "spear_swing_3.wav",
        // A javelin drawn back and thrown (saturdaysoundguy, Freesound).
        "jav_draw.wav", "jav_whoosh.wav",
        // Steel: clashes, a ring (artisticdude, StarNinjas).
        "ring.wav", "clash_1.ogg", "clash_2.ogg", "clash_3.ogg",
        // Impacts (Kenney, rubberduck, faxcorp): flesh, metal, wood, glass, stone.
        "impactSoft_heavy_000.ogg", "impactSoft_heavy_001.ogg", "impactSoft_medium_000.ogg", "impactSoft_medium_001.ogg",
        "impactPunch_medium_000.ogg", "impactPunch_medium_001.ogg", "impactPunch_medium_002.ogg", "impactPunch_heavy_000.ogg", "impactPunch_heavy_001.ogg",
        "impactMetal_heavy_000.ogg", "impactMetal_heavy_001.ogg",
        "impactWood_heavy_000.ogg", "impactWood_heavy_001.ogg", "impactGlass_light_000.ogg",
        "impactMining_000.ogg", "impactMining_001.ogg", "impactBell_heavy_000.ogg",
        "stones_01.ogg", "stones_02.ogg", "glass_01.ogg", "groundhit.wav",
        // Cloth and coin (Kenney, artisticdude).
        "cloth1.ogg", "cloth2.ogg", "cloth_heavy.wav", "handleCoins.ogg", "coins.wav",
        // Fire (USC / Sunset Editorial via the Internet Archive).
        "fire_whoosh.wav", "fire_pop.wav", "fire_bed.wav",
        // Storm (faxcorp, rubberduck).
        "zap_hit.wav", "zap_bloom.wav", "crackle.wav", "boom.wav", "thunder_01.ogg",
        // Frost (bart, IgnasD, artisticdude, rubberduck): ice crackling, snapping, cracking and
        // breaking.
        "ice.wav", "magic.wav", "ice_crack_1.wav", "ice_crack_2.wav", "ice_crack_3.wav",
        "ice_deep_1.wav", "ice_deep_2.wav", "ice_deep_3.wav", "ice_thump.wav",
        // Frost channelled, a frost spell's climax and a frost nova (Pontus's, generated); a frostbolt's impact
        // (eminyildirim, Freesound).
        "frost_channel.wav", "frost_cast.wav", "frost_nova.wav", "frost_impact.wav",
        // Footsteps (Kenney).
        "step_grass_0.ogg", "step_grass_1.ogg", "step_grass_2.ogg", "step_grass_3.ogg", "step_grass_4.ogg",
        "step_wood_0.ogg", "step_wood_1.ogg", "step_wood_2.ogg", "step_wood_3.ogg", "step_wood_4.ogg",
        "step_concrete_0.ogg", "step_concrete_1.ogg", "step_concrete_2.ogg", "step_concrete_3.ogg", "step_concrete_4.ogg",
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
        /// What the sound being built feeds: its own place in the mix (`placed`), into `squeeze`.
        bus: AudioNode,
        noise: AudioBuffer,
        samples: Samples,
        seed: u32,
        /// The wind is started once, the first time sound gets through.
        ambience_on: bool,
        /// Windup sounds lingering until their attack goes off, by whose (`Sound::voice`).
        lingering: HashMap<lightyear::prelude::PeerId, Lingering>,
    }

    /// A windup's sound still playing, waiting on its release: its source and its level.
    struct Lingering {
        source: AudioBufferSourceNode,
        level: GainNode,
    }

    /// Built at startup (`warm_up`), so the recordings are in before the first blow.
    #[derive(Default)]
    pub(super) struct AudioLazy(Option<AudioOut>);

    const MASTER: f32 = 0.6;
    /// The bus compressor: a gentle limiter that catches a pile-up of kills, not one that
    /// squashes every blow flat (the browser's default, a 12:1 ratio from -24 dB, did: a hit
    /// came out small and its tail ducked away under it).
    const SQUEEZE: (f32, f32, f32, f32, f32) = (-14.0, 12.0, 4.0, 0.004, 0.18);
    /// The most voices one kind of sound gets in a frame (the rest fold into their gain), and
    /// how far apart they start, so a cleared swarm crackles instead of phasing into one blast.
    const VOICES: usize = 3;
    const STAGGER: f64 = 0.014;
    /// How far to the side a sound off to one side is put (1: all the way): never in one ear
    /// only.
    const PAN_WIDTH: f32 = 0.8;
    /// Within this (meters) a sound is right on us, in the middle; it moves out to its side
    /// over it.
    const PAN_NEAR: f32 = 2.0;
    /// How the air dulls a far sound: the lowpass's cutoff (Hz) up close, how steeply it falls
    /// with distance, and the dullest it gets.
    const DULLING: (f32, f32, f32) = (18000.0, 0.6, 1500.0);
    /// Above this cutoff (Hz) the dulling can't be heard, so none is made.
    const DULL_UNHEARD: f32 = 10000.0;
    /// How loud the frost mage's spell is as its bolt goes.
    const FROST_SPELL: f32 = 0.354;
    /// The frost mage's channelled crackle: how loud, and where (seconds in `frost_channel`) its
    /// rise gives way to the bed it crackles on in (looped to its end).
    const FROST_CHANNEL: f32 = 0.0225;
    const FROST_BED: f64 = 1.0;

    /// Opens the audio (suspended until the page's first click, as browsers want) and starts
    /// fetching the recordings, so they're decoded by the time anything sounds.
    pub(super) fn warm_up(mut audio: NonSendMut<AudioLazy>) {
        if audio.0.is_none() {
            audio.0 = AudioOut::new();
        }
    }

    /// The master gain follows the volume setting.
    pub(super) fn apply_volume(settings: Res<crate::settings::Settings>, audio: NonSendMut<AudioLazy>) {
        if let Some(out) = audio.0.as_ref() {
            out.master.gain().set_value(MASTER * settings.volume);
        }
    }

    /// Where a sound is heard from: how far to the side (-1 left, 1 right) and how dulled by the
    /// air between (a lowpass's cutoff, Hz), if at all.
    #[derive(Clone, Copy)]
    pub(super) struct Place {
        pan: f32,
        dull: Option<f32>,
    }

    impl Place {
        /// Right on us: in the middle, clear.
        const HERE: Place = Place { pan: 0.0, dull: None };

        /// A sound `off` (meters, on the gameplay plane) from us, the camera's right being `right`.
        fn of(off: Vec2, right: Vec2) -> Place {
            let away = off.length();
            let pan = off.normalize_or_zero().dot(right) * PAN_WIDTH * (away / PAN_NEAR).min(1.0);
            let (clear, steep, dullest) = DULLING;
            let dull = Some((clear * (NEAR / away.max(NEAR)).powf(steep)).max(dullest)).filter(|&cutoff| cutoff < DULL_UNHEARD);
            Place { pan, dull }
        }
    }

    pub(super) fn play_sounds(
        mut queue: ResMut<SoundQueue>,
        mut audio: NonSendMut<AudioLazy>,
        me: Query<(&arena_shared::protocol::Pos, &arena_shared::protocol::PlayerId), With<crate::Me>>,
        camera: Res<crate::camera::CameraMode>,
    ) {
        if queue.sounds.is_empty() && queue.hushes.is_empty() {
            return;
        }
        let sounds = std::mem::take(&mut queue.sounds);
        let hushes = std::mem::take(&mut queue.hushes);
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
        for id in hushes {
            out.hush(id);
        }
        // Each placed round us: as loud as it is from here, off to its side. One that happens
        // somewhere is heard only with us somewhere to hear it from.
        let (ear, mine) = me.single().ok().map_or((None, None), |(pos, id)| (Some(pos.0), Some(id.0)));
        // The camera's right along the ground, unshaken.
        let right = camera.ground_axes().1;
        let mut placed: Vec<(&Sound, f32, Place)> = sounds
            .iter()
            .filter_map(|sound| {
                let (gain, place) = match sound.at {
                    None => (sound.gain, Place::HERE),
                    Some((at, reach)) => {
                        let ear = ear?;
                        let reach = if sound.by.is_some() && sound.by == mine { Reach::Own } else { reach };
                        (sound.gain * heard(at.distance(ear), reach), Place::of(at - ear, right))
                    }
                };
                (gain > 0.01).then_some((sound, gain, place))
            })
            .collect();
        // The loudest few of each kind sound, briefly staggered; the rest only thicken them.
        placed.sort_by(|a, b| a.0.kind.cmp(&b.0.kind).then(b.1.total_cmp(&a.1)));
        let mut k = 0;
        let mut kind = None;
        for &(sound, gain, place) in &placed {
            k = if Some(sound.kind) == kind { k + 1 } else { 0 };
            kind = Some(sound.kind);
            if k >= VOICES {
                continue;
            }
            let crowd = placed.iter().filter(|s| s.0.kind == sound.kind).count();
            let gain = gain * (1.0 + (crowd.saturating_sub(VOICES) as f32) * 0.1).min(1.6);
            out.play(sound, gain.min(1.0), sound.delay as f64 + k as f64 * STAGGER, place);
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
        let buffer: AudioBuffer = decoded.dyn_into()?;
        normalize(&buffer);
        Ok(buffer)
    }

    /// What every recording peaks at once it's in.
    const PEAK: f32 = 0.89;

    /// Brings a recording to `PEAK`, whatever level it was published at: a creak of leather
    /// shouldn't come in at a quarter of a sword clash's level just because of who recorded it,
    /// and the mix's gains then mean the same thing for every layer.
    fn normalize(buffer: &AudioBuffer) {
        let mut channels: Vec<Vec<f32>> = (0..buffer.number_of_channels()).filter_map(|c| buffer.get_channel_data(c).ok()).collect();
        let peak = channels.iter().flatten().fold(0.0f32, |m, s| m.max(s.abs()));
        if peak < 0.001 || (peak - PEAK).abs() < 0.02 {
            return;
        }
        let gain = PEAK / peak;
        for (c, samples) in channels.iter_mut().enumerate() {
            for s in samples.iter_mut() {
                *s *= gain;
            }
            let _ = buffer.copy_to_channel(samples, c as i32);
        }
    }

    impl AudioOut {
        fn new() -> Option<AudioOut> {
            let ctx = AudioContext::new().ok()?;
            let squeeze = ctx.create_dynamics_compressor().ok()?;
            let (threshold, knee, ratio, attack, release) = SQUEEZE;
            squeeze.threshold().set_value(threshold);
            squeeze.knee().set_value(knee);
            squeeze.ratio().set_value(ratio);
            squeeze.attack().set_value(attack);
            squeeze.release().set_value(release);
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
            let bus = squeeze.clone().into();
            Some(AudioOut { ctx, master, squeeze, bus, noise, samples: recordings, seed: 1, ambience_on: false, lingering: HashMap::new() })
        }

        /// A sound's own way into the mix, from where it is (`Place`): panned to its side, and
        /// dulled first if it's far. `None` for one right on us, or if the nodes can't be made.
        fn placed(&self, place: Place) -> Option<AudioNode> {
            if place.pan.abs() < 0.01 && place.dull.is_none() {
                return None;
            }
            let panner = self.ctx.create_stereo_panner().ok()?;
            panner.pan().set_value(place.pan);
            panner.connect_with_audio_node(&self.squeeze).ok()?;
            let Some(cutoff) = place.dull else { return Some(panner.into()) };
            let filter = self.ctx.create_biquad_filter().ok()?;
            filter.set_type(BiquadFilterType::Lowpass);
            filter.frequency().set_value(cutoff);
            filter.connect_with_audio_node(&panner).ok()?;
            Some(filter.into())
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
            g.connect_with_audio_node(&self.bus).ok()?;
            Some(g)
        }

        /// An envelope that holds: up to `peak` at once, held `hold` seconds, then dying away over
        /// `release`. For a recording, whose own shape should be heard, not squeezed. The die-away
        /// is exponential only down to a whisper of the peak (an exponential ramp to nothing is
        /// inaudible a third of the way in: the release would be a third as long as it says), and
        /// linear to nothing from there.
        fn held(&self, when: f64, peak: f32, hold: f64, release: f64) -> Option<GainNode> {
            let g = self.ctx.create_gain().ok()?;
            let p = g.gain();
            let peak = peak.max(0.0002);
            let _ = p.set_value_at_time(0.0001, when);
            let _ = p.linear_ramp_to_value_at_time(peak, when + 0.003);
            let _ = p.set_value_at_time(peak, when + 0.003 + hold);
            let _ = p.exponential_ramp_to_value_at_time(peak * 0.06, when + 0.003 + hold + release);
            let _ = p.linear_ramp_to_value_at_time(0.0001, when + 0.003 + hold + release * 1.3);
            g.connect_with_audio_node(&self.bus).ok()?;
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

        /// A sharp sound at `peak` from `when` coming back off the walls round it: twice, each
        /// time quieter and duller. The mix has no reverb; this gives a hit some room.
        fn echo(&mut self, key: &str, when: f64, rate: f32, peak: f32) {
            for (after, share, dull) in [(0.09, 0.14, 2500.0), (0.2, 0.07, 1500.0)] {
                if let Some(e) = self.held(when + after, peak * share, 0.03, 0.12) {
                    self.clip_through(key, when + after, 0.0, None, rate, BiquadFilterType::Lowpass, dull, &e);
                }
            }
        }

        /// A whole recording at `peak`, from `when`, `rate` times its speed, held `hold` seconds
        /// and let go over `release`: the plain way most layers go in.
        fn sample(&mut self, key: &str, when: f64, peak: f32, rate: f32, hold: f64, release: f64) {
            if let Some(e) = self.held(when, peak, hold, release) {
                self.clip(key, when, 0.0, None, rate, &e);
            }
        }

        /// A recording played from its start at `peak` from `when`, looping from `from` (seconds
        /// in) to its end once it gets there, until it's faded (`fade`). `None` until it has
        /// loaded.
        fn linger(&self, key: &str, when: f64, peak: f32, from: f64) -> Option<Lingering> {
            let buffer = self.samples.borrow().get(key)?.clone();
            let source = self.ctx.create_buffer_source().ok()?;
            source.set_buffer(Some(&buffer));
            source.set_loop(true);
            source.set_loop_start(from);
            source.set_loop_end(buffer.duration());
            let level = self.ctx.create_gain().ok()?;
            let p = level.gain();
            let peak = peak.max(0.0002);
            let _ = p.set_value_at_time(0.0001, when);
            let _ = p.linear_ramp_to_value_at_time(peak, when + 0.01);
            source.connect_with_audio_node(&level).ok()?;
            level.connect_with_audio_node(&self.bus).ok()?;
            let scheduled: &AudioScheduledSourceNode = source.as_ref();
            scheduled.start_with_when(when).ok()?;
            Some(Lingering { source, level })
        }

        /// `id`'s lingering windup sound, if one is playing, faded out now.
        fn hush(&mut self, id: lightyear::prelude::PeerId) {
            if let Some(lingering) = self.lingering.remove(&id) {
                self.fade(&lingering, self.ctx.current_time());
            }
        }

        /// A lingering sound faded out quickly from `when`, and stopped.
        fn fade(&self, lingering: &Lingering, when: f64) {
            let p = lingering.level.gain();
            let _ = p.cancel_scheduled_values(when);
            let _ = p.set_target_at_time(0.0, when, 0.006);
            let scheduled: &AudioScheduledSourceNode = lingering.source.as_ref();
            let _ = scheduled.stop_with_when(when + 0.06);
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

        /// A horn's note at `f` Hz: brass, as a synthesizer does it. Two sawtooths a hair apart
        /// (the chorus of a real bore) through a lowpass that opens as the note swells and
        /// closes as it dies (the "wah" of a horn blown harder, then easing), with a vibrato
        /// that only sets in once the note is held. Swelling over `attack` to `peak`, gone at
        /// `when + dur`.
        fn horn(&self, f: f32, when: f64, attack: f64, peak: f32, dur: f64) {
            use BiquadFilterType::Lowpass;
            use OscillatorType::{Sawtooth, Sine};
            let Some(e) = self.env(when, attack, peak, dur) else { return };
            let Ok(filter) = self.ctx.create_biquad_filter() else { return };
            filter.set_type(Lowpass);
            let cut = filter.frequency();
            let _ = cut.set_value_at_time(f * 2.5, when);
            let _ = cut.linear_ramp_to_value_at_time(f * 9.0, when + attack);
            let _ = cut.exponential_ramp_to_value_at_time(f * 3.5, when + dur);
            filter.q().set_value(1.3);
            if filter.connect_with_audio_node(&e).is_err() {
                return;
            }
            for detune in [1.0, 1.004] {
                let Ok(o) = self.ctx.create_oscillator() else { continue };
                o.set_type(Sawtooth);
                o.frequency().set_value(f * detune);
                if let (Ok(lfo), Ok(depth)) = (self.ctx.create_oscillator(), self.ctx.create_gain()) {
                    lfo.set_type(Sine);
                    lfo.frequency().set_value(5.2);
                    let d = depth.gain();
                    let _ = d.set_value_at_time(0.0, when);
                    let _ = d.linear_ramp_to_value_at_time(f * 0.007, when + attack + 0.35);
                    if lfo.connect_with_audio_node(&depth).is_ok() && depth.connect_with_audio_param(&o.frequency()).is_ok() {
                        let _ = lfo.start_with_when(when);
                        let _ = lfo.stop_with_when(when + dur + 0.05);
                    }
                }
                if o.connect_with_audio_node(&filter).is_ok() {
                    let _ = o.start_with_when(when);
                    let _ = o.stop_with_when(when + dur + 0.05);
                }
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
        fn play(&mut self, sound: &Sound, gain: f32, delay: f64, place: Place) {
            let (kind, pitch, begun, voice) = (sound.kind, sound.pitch, sound.begun as f64, sound.by);
            use BiquadFilterType::{Bandpass, Highpass, Lowpass};
            use OscillatorType::{Sine, Triangle};
            self.bus = self.placed(place).unwrap_or_else(|| self.squeeze.clone().into());
            let now = self.ctx.current_time() + 0.01 + delay;
            let g = gain;
            // A small random spread keeps a flurry from repeating. A voice tuned by ear to one
            // speed (the revenant's, the javelin's throw) only drifts off it very slightly, so
            // no two of its attacks are twins either.
            let tuned = matches!(kind, Sfx::GlaiveSwing | Sfx::GlaiveWhoosh | Sfx::SpearThrow | Sfx::FrostCast | Sfx::Shatter);
            let p = self.vary(if tuned { 0.03 } else { 0.08 }) * pitch;
            const SWISH_LIGHT: [&str; 3] = ["swish_light_1", "swish_light_2", "swish_light_3"];
            const SWISH_HEAVY: [&str; 3] = ["swish_heavy_1", "swish_heavy_2", "swish_heavy_3"];
            const SPEAR_SWINGS: [&str; 3] = ["spear_swing_1", "spear_swing_2", "spear_swing_3"];
            const FLESH: [&str; 5] = ["impactPunch_medium_000", "impactPunch_medium_001", "impactPunch_medium_002", "impactSoft_medium_000", "impactSoft_medium_001"];
            const FLESH_HEAVY: [&str; 4] = ["impactPunch_heavy_000", "impactPunch_heavy_001", "impactSoft_heavy_000", "impactSoft_heavy_001"];
            const METAL_HEAVY: [&str; 2] = ["impactMetal_heavy_000", "impactMetal_heavy_001"];
            const WOOD_HEAVY: [&str; 2] = ["impactWood_heavy_000", "impactWood_heavy_001"];
            const STONES: [&str; 2] = ["stones_01", "stones_02"];
            const CLASHES: [&str; 3] = ["clash_1", "clash_2", "clash_3"];
            const ICE_CRACKS: [&str; 3] = ["ice_crack_1", "ice_crack_2", "ice_crack_3"];
            const ICE_DEEP: [&str; 3] = ["ice_deep_1", "ice_deep_2", "ice_deep_3"];
            match kind {
                Sfx::Swing => {
                    // A blade through the air: a real swish, a heavy weapon's lower and longer.
                    let key = self.pick(&SWISH_LIGHT);
                    self.sample(key, now, 0.38 * g, p, 0.1, 0.12);
                }
                Sfx::Step(surface) => {
                    // A foot down on what's there: one of five takes, each a little different.
                    let i = (self.r() * 5.0) as usize % 5;
                    let key = match surface {
                        Surface::Grass => ["step_grass_0", "step_grass_1", "step_grass_2", "step_grass_3", "step_grass_4"][i],
                        Surface::Stone => ["step_concrete_0", "step_concrete_1", "step_concrete_2", "step_concrete_3", "step_concrete_4"][i],
                        Surface::Wood => ["step_wood_0", "step_wood_1", "step_wood_2", "step_wood_3", "step_wood_4"][i],
                    };
                    let rate = p * self.vary(0.06);
                    self.sample(key, now, 0.9 * g, rate, 0.08, 0.15);
                }
                Sfx::Dash(voice) => match voice {
                    DashVoice::Air => {
                        let rush = self.pick(&SWISH_HEAVY);
                        self.sample(rush, now, 0.45 * g, 0.7 * p, 0.15, 0.15);
                        self.sample("cloth_heavy", now + 0.02, 0.3 * g, 0.9 * p, 0.15, 0.1);
                    }
                    DashVoice::Spectral => {
                        // Slipping between worlds: the rush, and the hum of the other side
                        // swelling and gone.
                        let rush = self.pick(&SWISH_HEAVY);
                        self.sample(rush, now, 0.45 * g, 0.6 * p, 0.15, 0.2);
                        self.sample("magic", now, 0.16 * g, 0.8 * p, 0.3, 0.3);
                        for f in [110.0, 113.0] {
                            if let Some(e) = self.env(now, 0.08, 0.03 * g, 0.4) {
                                self.osc(Sine, f * p, f * p, now, 0.4, &e);
                            }
                        }
                    }
                },
                Sfx::Windup => {
                    // Cloth and the first stir of air as the weapon is raised.
                    let cloth = self.pick(&["cloth1", "cloth2"]);
                    self.sample(cloth, now, 0.12 * g, p, 0.15, 0.1);
                }
                Sfx::Loose => {
                    // A shot leaving the hand: a short swing of air.
                    self.sample("swing_2", now, 0.34 * g, p * 1.05, 0.1, 0.1);
                }
                Sfx::GlaiveWhoosh => {
                    // A long haft and a heavy blade shoving through the air (a staff's swing
                    // slowed down, its whistle dulled away), the coat flapping with it.
                    let air = self.pick(&SPEAR_SWINGS);
                    if let Some(e) = self.held(now, 0.4 * g, 0.3, 0.2) {
                        self.clip_through(air, now, 0.0, None, 0.65 * p, Lowpass, 2200.0, &e);
                    }
                    self.sample("cloth_heavy", now + 0.02, 0.12 * g, 0.85 * p, 0.2, 0.1);
                }
                Sfx::GlaiveSwing => {
                    // Recordings only, nothing synthesized, nothing that bounces, no scatter of
                    // debris (it reads as a splash). The thunk, all at once and cut short: the
                    // blade's and the stomping foot's thump, a slowed heavy blow's weight under
                    // it, and a low body to it.
                    if let Some(e) = self.held(now, 0.42 * g, 0.06, 0.1) {
                        self.clip_through("groundhit", now, 0.0, None, 0.7 * p, Lowpass, 1400.0, &e);
                    }
                    let weight = self.pick(&["impactPunch_heavy_000", "impactPunch_heavy_001"]);
                    if let Some(e) = self.held(now, 0.35 * g, 0.05, 0.1) {
                        self.clip_through(weight, now, 0.0, None, 0.55 * p, Lowpass, 600.0, &e);
                    }
                    if let Some(e) = self.held(now, 0.5 * g, 0.05, 0.12) {
                        self.clip_through("boom", now, 0.0, None, 0.6 * p, Lowpass, 180.0, &e);
                    }
                    // Then the rift opening where it struck, right on the thunk's heels: a roll of
                    // thunder slowed right down into the earth groaning open like a heavy gate,
                    // short.
                    let tear = now + 0.03;
                    if let Some(e) = self.held(tear, 0.6 * g, 0.15, 0.25) {
                        self.clip_through("thunder_01", tear, 0.75, Some(0.7), 0.55 * p, Lowpass, 400.0, &e);
                    }
                }
                Sfx::ClawSwing => {
                    // A ragged swipe: a swish with rags dragging through it.
                    let swish = self.pick(&SWISH_LIGHT);
                    self.sample(swish, now, 0.34 * g, 0.75 * p, 0.12, 0.1);
                    self.sample("cloth_heavy", now + 0.02, 0.28 * g, 0.9 * p, 0.15, 0.1);
                }
                Sfx::JavReady => {
                    // Going into stance: a soft swish late in the windup (`AttackVoice::late` times
                    // it). Never slowed. Only a faint confirmation that the attack is coming:
                    // well under the throw, though the recording's own balance already has it
                    // quieter.
                    self.sample("jav_draw", now, 0.0262 * g, 1.0, 0.3, 0.05);
                }
                Sfx::SpearThrow => {
                    // The arm lashing the spear away: a hard whoosh, as recorded. The spear flies
                    // silent after it.
                    if let Some(e) = self.held(now, 0.55 * g, 0.8, 0.05) {
                        self.clip("jav_whoosh", now, begun, None, 1.0, &e);
                    }
                }
                Sfx::FrostReady => {
                    // Frost channelled up: the crackle rising from its start, and if the windup
                    // outlasts it (a shot held drawn), crackling on (a long, even bed of it,
                    // never a pattern) until the bolt goes (`FrostCast` ends it) or the windup is
                    // cut off (`SoundQueue::hush`).
                    let Some(voice) = voice else { return };
                    if let Some(old) = self.lingering.remove(&voice) {
                        self.fade(&old, now);
                    }
                    let Some(lingering) = self.linger("frost_channel", now, FROST_CHANNEL * g, FROST_BED) else { return };
                    self.lingering.insert(voice, lingering);
                }
                Sfx::FrostCast => {
                    // The spell as the bolt goes: its climax and icy tail, as recorded (from
                    // partway in if the news of it came late).
                    if let Some(e) = self.held(now, FROST_SPELL * g, 3.0, 0.2) {
                        self.clip("frost_cast", now, begun, None, 1.0, &e);
                    }
                    // The windup's crackle ends under it: the spell has its own tail.
                    if let Some(lingering) = voice.and_then(|voice| self.lingering.remove(&voice)) {
                        self.fade(&lingering, now);
                    }
                }
                Sfx::FrostLoose => {
                    // A frostbolt let go. The front: a hard close snap of ice, the loudest of
                    // it, a deep crack and a thump of weight under it (the thump's peak is a
                    // little way in, so the snap waits for it). Then crackling trailing off,
                    // and the snap coming back quieter and duller off the walls.
                    let v = 0.92 + 0.16 * self.r();
                    let knock = 0.95 + 0.1 * self.r();
                    self.sample("ice_thump", now, 0.3 * g, knock * p, 0.05, 0.12);
                    let crack = self.pick(&ICE_CRACKS);
                    self.sample(crack, now + 0.018, 0.65 * g, 1.15 * v * p, 0.03, 0.12);
                    let deep = self.pick(&ICE_DEEP);
                    self.sample(deep, now + 0.02, 0.32 * g, v * p, 0.06, 0.15);
                    if let Some(e) = self.held(now + 0.07, 0.09 * g, 0.2, 0.4) {
                        let from = 0.04 + 0.16 * self.r() as f64;
                        self.clip("ice", now + 0.07, from, Some(0.6), 1.1 * v * p, &e);
                    }
                    self.echo(crack, now + 0.018, 1.15 * v * p, 0.65 * g);
                }
                Sfx::FireLoose => {
                    // A gout of fire tossed: it catches with a pop, whooshes away, and
                    // crackles trailing off behind it.
                    self.sample("fire_pop", now, 0.35 * g, 1.05 * p, 0.15, 0.2);
                    self.sample("fire_whoosh", now + 0.01, 0.72 * g, 1.1 * p, 0.5, 0.4);
                    let from = if self.r() < 0.5 { 0.0 } else { 0.5 };
                    if let Some(e) = self.held(now + 0.15, 0.22 * g, 0.3, 0.3) {
                        self.clip("fire_bed", now + 0.15, from, Some(0.6), p, &e);
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
                Sfx::SpearTing => {
                    // A pick's first bite into stone, raised, its rubble after cut off.
                    let chip = self.pick(&["impactMining_000", "impactMining_001"]);
                    self.sample(chip, now, 0.14 * g, 1.7 * p, 0.03, 0.06);
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
                    self.sample(clash, now + 0.01, 0.4 * g, 1.1 * p, 0.2, 0.3);
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
                    if let Some(e) = self.held(now + 0.3, 0.32 * g, 1.0, 1.2) {
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
                    self.sample("thunder_01", now + 0.02, 0.7 * g, p, 1.5, 1.2);
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
                Sfx::FrostNova => {
                    // Ice cracking out across the ground all round: one recording, from its
                    // first crack (the nova is instant), faded out soon after its burst.
                    self.sample("frost_nova", now, 0.48 * g, 1.0, 1.0, 0.2);
                }
                Sfx::Shatter => {
                    // A frostbolt breaking apart: a punch of ice, as the recording has it.
                    self.sample("frost_impact", now, 0.65 * g, p, 0.5, 0.3);
                }
                Sfx::Horn => {
                    // A war horn's call: a short low note, then the long one a fourth up, held
                    // and let die. One horn, blown by one man.
                    self.horn(130.8, now, 0.06, 0.22 * g, 0.36);
                    self.horn(174.6, now + 0.34, 0.14, 0.28 * g, 1.7);
                }
                Sfx::BossHorn => {
                    // The deep horn: lower, slower, the long note held longer, a second horn a
                    // fifth up joining it late, and a bell struck under it all.
                    self.horn(87.3, now, 0.1, 0.3 * g, 0.5);
                    self.horn(116.5, now + 0.48, 0.2, 0.34 * g, 2.6);
                    self.horn(174.6, now + 0.9, 0.35, 0.12 * g, 2.1);
                    self.sample("impactBell_heavy_000", now + 0.5, 0.35 * g, 0.6, 0.8, 1.2);
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
