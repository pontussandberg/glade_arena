//! Combat feedback: a health bar over every fighter with a cast bar under it while it winds up
//! an attack, the health lost lingering on the bar a moment before it drains (gold, longer, and
//! with the bar popping on a crit), a damage number when one takes damage (as big as the blow,
//! thrown up and falling; a crit's bigger and golden), a green number when one is healed, and
//! dead fighters disappearing until they respawn.

use arena_shared::config::TICK_HZ;
use arena_shared::protocol::*;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::render::sync_world::RenderEntity;
use bevy::ui::ComputedStackIndex;
use lightyear::core::time::TickInstant;
use lightyear::interpolation::timeline::InterpolationTimeline;
use lightyear::prelude::client::RemoteTimeline;
use lightyear::prelude::*;

use crate::Me;

use crate::camera::CameraPlaced;
use crate::arena::{palette, to_gameplay};
use crate::render::{Relation, shown, ui_text};
use crate::rects::Rects;
use crate::render::GameUi;
use crate::rooms::Screen;

pub struct FeedbackPlugin;

impl Plugin for FeedbackPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(crate::rects::RectsPlugin);
        app.add_systems(Startup, spawn_world_ui);
        app.add_systems(
            Update,
            (
                add_bars,
                draw_bars.in_set(CameraPlaced).run_if(in_state(Screen::InGame)),
                react_to_hits,
                hide_the_dead,
                spawn_damage_numbers,
                float_damage_numbers.in_set(CameraPlaced).run_if(in_state(Screen::InGame)),
            ),
        );
    }
}

/// Bar size in pixels, and how high above a fighter's feet it floats.
const BAR_SIZE: Vec2 = Vec2::new(76.0, 10.0);
/// A monster's (a zombie's): shorter and thinner, so it never reads as a player's.
const MONSTER_BAR_SIZE: Vec2 = Vec2::new(38.0, 5.0);

/// The size of `class`'s health bar: a player's, or a monster's (a class no one picks).
fn bar_size(class: ClassId, elite: bool) -> Vec2 {
    let def = class.def();
    match () {
        _ if def.playable => BAR_SIZE,
        _ if def.is_boss() => MONSTER_BAR_SIZE * Vec2::new(2.2, 1.4),
        _ if elite => MONSTER_BAR_SIZE * Vec2::new(1.4, 1.2),
        _ => MONSTER_BAR_SIZE,
    }
}
const BAR_HEIGHT: f32 = 2.3;
/// The cast bar hangs right under the health bar, as wide and a bit thinner.
const CAST_BAR_HEIGHT: f32 = 6.0;
/// A mark across the health bar every this much health, for scale.
const HP_PER_TICK: i32 = 25;
/// About the most marks a bar has, and how close together (pixels) they may be.
const MOST_TICKS: i32 = 12;
const TICK_GAP: f32 = 5.0;
/// Health just lost stays on the bar as a pale chunk: white at the hit, paling to `SUN` over
/// `TRAIL_HOLD` seconds (each new hit starts it over, so a combo adds up), then draining to the
/// health left, closing `TRAIL_DRAIN` of the gap a second (eased: fast, then settling).
const TRAIL_HOLD: f32 = 0.225;
const TRAIL_DRAIN: f32 = 10.0;
/// A crit's trail starts flame-gold (like its number) and holds `CRIT_TRAIL_HOLD` instead, and
/// the bar pops `CRIT_BAR_POP` bigger, settling over `CRIT_BAR_POP_SECONDS`.
const CRIT_TRAIL_HOLD: f32 = 0.6;
const CRIT_BAR_POP: f32 = 0.15;
const CRIT_BAR_POP_SECONDS: f32 = 0.15;
/// Damage under this doesn't flinch the body (`rig::HitSquash`) or spark: a damage-over-time
/// pulse counts, but only a real blow rocks the figure.
const SQUASH_DAMAGE: i32 = 4;

/// Damage numbers: text size in pixels, from the smallest blow's up to the size of one of
/// `NUMBER_FULL_DAMAGE` or more (`number_scale`; a crit's bigger again by `CRIT_NUMBER_GROWTH`, and popping in
/// bigger still by `CRIT_POP`, settling over the first `CRIT_POP_PART` of its life), how long
/// one shows (fading out over the last `NUMBER_FADE_PART`), and how far apart numbers landing
/// together spread sideways. Each is thrown up off the health bar (`NUMBER_THROW`, pixels a
/// second) a little to one side of straight up (up to `NUMBER_TILT` radians), slows, and falls
/// back under `NUMBER_GRAVITY` (pixels a second, a second).
const NUMBER_SIZE: (f32, f32) = (15.0, 40.0);
const NUMBER_FULL_DAMAGE: f32 = 80.0;
const CRIT_NUMBER_GROWTH: f32 = 1.3;
const CRIT_POP: f32 = 0.6;
const CRIT_POP_PART: f32 = 0.15;
const NUMBER_SECONDS: f32 = 0.9;
/// Plain damage to one fighter landing this soon (seconds) after its last number adds to that
/// number instead of floating up a new one, so a flurry (arcs, bursts, a barrage) reads as one.
const NUMBER_MERGE_SECONDS: f32 = 0.25;
const CRIT_NUMBER_SECONDS: f32 = 1.2;
const NUMBER_FADE_PART: f32 = 0.4;
const NUMBER_SPREAD: f32 = 14.0;
const NUMBER_THROW: f32 = 290.0;
const NUMBER_TILT: (f32, f32) = (0.05, 0.1);
const NUMBER_GRAVITY: f32 = 800.0;

/// How big a number for `amount` is, as a share of the biggest (`NUMBER_SIZE`): its text is laid
/// out at that size and scaled down, so there are only ever a couple of font sizes (each one
/// costs a font atlas of its own).
fn number_scale(amount: i32) -> f32 {
    let heft = (amount as f32 / NUMBER_FULL_DAMAGE).clamp(0.0, 1.0).powf(0.7);
    (NUMBER_SIZE.0 + (NUMBER_SIZE.1 - NUMBER_SIZE.0) * heft) / NUMBER_SIZE.1
}

/// How the number for the hit `seq` is thrown (pixels a second, right and up): a little to one
/// side of straight up (`NUMBER_TILT`), which and how far picked from the hit.
fn number_throw(seq: u32) -> Vec2 {
    let r = |k: u32| crate::swish::noise(seq, k);
    let tilt = (NUMBER_TILT.0 + (NUMBER_TILT.1 - NUMBER_TILT.0) * r(1)) * if r(2) < 0.5 { -1.0 } else { 1.0 };
    Vec2::new(tilt.sin(), tilt.cos()) * NUMBER_THROW
}

/// How far (pixels, right and up) a number thrown at `throw` has flown `age` seconds on: up and
/// a little aside, slowing, then falling (`NUMBER_GRAVITY`).
fn number_flown(throw: Vec2, age: f32) -> Vec2 {
    throw * age - Vec2::Y * NUMBER_GRAVITY * age * age / 2.0
}

/// On a fighter with bars (`draw_bars`): the health its health bar's trail still shows, as a
/// fraction of the most; the health it saw last frame, to tell a new hit; when the last hit
/// landed; and when the last crit we saw (`CritAt`) landed, and whether the trail still shows
/// one (until it has drained).
#[derive(Component)]
struct HealthTrail {
    fraction: f32,
    last: f32,
    hit_at: f32,
    crit_at: f32,
    crit: bool,
}

/// On a fighter with bars: whether a wall, a roof or a hill hides them from the camera
/// (`Map::sight_meets`), as last looked (`None` before the first look). Looked again only every
/// `SIGHT_FRAMES` frames, each fighter on frames of its own, as a crowd's every bar is looked at.
#[derive(Component, Default)]
struct BarSight(Option<bool>);
const SIGHT_FRAMES: u32 = 4;

/// When this fighter last took a crit we showed a number for.
#[derive(Component)]
struct CritAt(f32);

/// The last health we showed for this fighter, to tell a hit from a heal or a respawn.
#[derive(Component)]
struct ShownHealth(i32);

/// The running number (`RecentHits::seq`) of the last hit on this fighter we showed a number for.
#[derive(Component)]
struct ShownHits(u32);

/// A damage number thrown up from over `player`'s health bar, from `born` for `lasts` seconds.
#[derive(Component)]
struct DamageNumber {
    player: Entity,
    born: f32,
    lasts: f32,
    crit: bool,
    /// Sideways from the bar's center, in pixels.
    dx: f32,
    /// How fast it's thrown (pixels a second, right and up), a little off straight up.
    throw: Vec2,
    /// How big it's drawn, as a share of its text's size (`number_scale`).
    scale: f32,
    /// For plain damage, how much it shows, for more landing on it (`NUMBER_MERGE_SECONDS`).
    merges: Option<i32>,
}

/// What floats over the fighters: their damage numbers under one screen-filling `GameUi` root,
/// and their bars drawn just over it (`draw_bars`), so it's all hidden outside the arena, where
/// others can still be fighting.
#[derive(Component)]
struct WorldUi;

fn spawn_world_ui(mut commands: Commands) {
    commands.spawn((
        WorldUi,
        GameUi,
        Node { position_type: PositionType::Absolute, width: percent(100.0), height: percent(100.0), ..default() },
    ));
}

/// Each fighter gets its bars once it has a body. The shopkeeper has none: no one can hurt them.
fn add_bars(mut commands: Commands, new: Query<Entity, (With<PlayerId>, Added<Mesh3d>, Without<Shopkeeper>)>) {
    for fighter in &new {
        commands.entity(fighter).insert((HealthTrail { fraction: 1.0, last: 1.0, hit_at: 0.0, crit_at: 0.0, crit: false }, BarSight::default()));
    }
}

/// How much health a bar's marks are apart: every `HP_PER_TICK`, or for a tough one (or on a
/// small bar) every so many of them, so the marks don't run together: about `MOST_TICKS` of
/// them at most, and none closer than `TICK_GAP` pixels on a bar `width` wide.
fn tick_every(max_hp: i32, width: f32) -> i32 {
    let tough = max_hp / (HP_PER_TICK * MOST_TICKS);
    let close = (TICK_GAP * max_hp as f32 / (width * HP_PER_TICK as f32)).ceil() as i32;
    HP_PER_TICK * tough.max(close).max(1)
}

/// Where a fighter's health bar is, over its head, in the world.
fn bar_point(body: &Transform, size: f32) -> Vec3 {
    body.translation + Vec3::Y * BAR_HEIGHT * size.max(0.8)
}

/// Where a fighter's health bar is centered on screen (`None` off camera).
fn over_bar(camera: &Camera, camera_transform: &GlobalTransform, body: &Transform, size: f32) -> Option<Vec2> {
    camera.world_to_viewport(camera_transform, bar_point(body, size)).ok()
}

/// How far behind the latest state from the server our own fighter is drawn, in ticks: enough
/// that the next update is (almost) always in by the time it's needed, so it moves smoothly
/// between updates however unevenly they arrive (`render::glide_me`).
pub(crate) const ME_BEHIND_TICKS: f32 = 1.0;

/// The server tick our latest news from it is from, advancing smoothly with real time between
/// packets (`advance_latest_clock`): a packet from tick T arrives about when this reads T.
#[derive(Resource, Default)]
pub(crate) struct LatestClock {
    tick: Option<f32>,
    /// The tick of the latest packet from the server.
    pub(crate) latest: Option<u32>,
}

/// How hard each new packet pulls the clock toward its tick (a little: arrivals are uneven), and
/// how far off it may be before it's set straight there instead (after a stall, at the start).
const CLOCK_PULL: f32 = 0.1;
const CLOCK_SNAP_TICKS: f32 = 4.0;

pub(crate) fn advance_latest_clock(time: Res<Time<Real>>, remote: Query<&RemoteTimeline, With<Client>>, mut clock: ResMut<LatestClock>) {
    let Some(latest) = remote.single().ok().and_then(|r| r.last_received_tick()).map(|t| t.0) else { return };
    let mut tick = clock.tick.map_or(latest as f32, |t| t + time.delta_secs() * TICK_HZ as f32);
    if clock.latest != Some(latest) {
        let off = latest as f32 - tick;
        tick = if off.abs() > CLOCK_SNAP_TICKS { latest as f32 } else { tick + off * CLOCK_PULL };
    }
    (clock.tick, clock.latest) = (Some(tick), Some(latest));
}

/// "Now" in fractional ticks, on the timeline a fighter is shown on, to compare with its
/// `AttackState`. Ours: `LatestClock` less `ME_BEHIND_TICKS`. Others' are interpolated (their
/// windup plays out in step with their delayed position). Projectiles go by ours, so a shot
/// coming at us is drawn where it is relative to us.
#[derive(SystemParam)]
pub(crate) struct AttackClock<'w> {
    latest: Option<Res<'w, LatestClock>>,
    interpolation: Option<Res<'w, InterpolationTimeline>>,
}

impl AttackClock<'_> {
    pub(crate) fn now(&self, me: bool) -> f32 {
        let ticks = |t: TickInstant| t.tick().0 as f32 + t.overstep().to_f32();
        let mine = self.latest.as_ref().and_then(|c| c.tick).map(|t| t - ME_BEHIND_TICKS);
        match (&self.interpolation, mine) {
            (Some(timeline), _) if !me => ticks(timeline.now),
            (_, Some(mine)) => mine,
            (Some(timeline), None) => ticks(timeline.now),
            (None, None) => 0.0,
        }
    }

    /// The tick of the latest packet from the server: what a state that just arrived is from.
    pub(crate) fn latest_tick(&self) -> Option<u32> {
        self.latest.as_ref().and_then(|c| c.latest)
    }
}

/// A fighter's bars as `draw_bars` draws them: where the health bar's middle is on screen and how
/// big it is (pixels), how much bigger it pops for a crit, how full it is and its trail, in what
/// colors, its marks (every `per_tick` of `max_hp`), and how full the cast bar under it is while
/// the attack recovers.
struct Bar {
    middle: Vec2,
    size: Vec2,
    pop: f32,
    health: f32,
    trail: f32,
    fill_color: LinearRgba,
    trail_color: LinearRgba,
    per_tick: i32,
    max_hp: i32,
    cast: Option<f32>,
}

/// Every fighter's bars drawn over it, the nearest over the rest, just over `WorldUi` (so under
/// the damage numbers) as flat rectangles (`Rects`), not UI nodes: a crowd's bars as nodes made
/// Bevy go over every one of them every frame. None while a wall, a roof or a hill is between
/// the camera and them (`Map::sight_meets`; ours always shows), nor while they're dead, nor
/// outside the arena (`WorldUi` hidden). The cast bar goes by `AttackClock`.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn draw_bars(
    time: Res<Time>,
    frame: Res<bevy::diagnostic::FrameCount>,
    clock: AttackClock,
    map: Option<Res<crate::rooms::CurrentMap>>,
    camera: Single<(&Camera, &GlobalTransform, &RenderEntity), With<crate::camera::CameraRig>>,
    world_ui: Single<(&ComputedStackIndex, &InheritedVisibility), With<WorldUi>>,
    mut fighters: Query<(Entity, &Transform, &ClassId, Option<&Health>, Option<&CritAt>, &AttackState, Has<Me>, (Option<&MaxHealth>, Has<Elite>, Option<&Relation>), (&mut HealthTrail, &mut BarSight))>,
    mut rects: ResMut<Rects>,
    mut shown: Local<Vec<(f32, Bar)>>,
) {
    let (camera, camera_transform, render_entity) = *camera;
    let (stack, world_ui_shown) = *world_ui;
    rects.camera = Some(render_entity.id());
    rects.over(stack.0);
    let now = time.elapsed_secs();
    let eye = camera_transform.translation();
    shown.clear();
    for (fighter, body, class, health, crit_at, attack, is_me, (max, elite, relation), (mut trail, mut sight)) in &mut fighters {
        let max_hp = max.map_or(class.def().max_hp, |m| m.0);
        let health_fraction = health.map_or(1.0, |h| h.0 as f32 / max_hp as f32);
        update_trail(&mut trail, health_fraction, crit_at.map(|c| c.0), now, time.delta_secs());
        let alive = health.is_none_or(Health::alive);
        let height = crate::rig::drawn_size(*class, elite);
        let over = bar_point(body, height);
        let hidden = !is_me
            && map.as_ref().is_some_and(|map| {
                if sight.0.is_none() || (frame.0 + fighter.index_u32()) % SIGHT_FRAMES == 0 {
                    sight.0 = Some(map.map().sight_meets((to_gameplay(eye), eye.y), (to_gameplay(over), over.y)));
                }
                sight.0 == Some(true)
            });
        let Some(middle) = over_bar(camera, camera_transform, body, height).filter(|_| alive && !hidden && world_ui_shown.get()) else { continue };
        let size = bar_size(*class, elite);
        let (start, hold) = if trail.crit { (palette::TORCH_FLAME, CRIT_TRAIL_HOLD) } else { (Color::WHITE, TRAIL_HOLD) };
        let pale = ((now - trail.hit_at) / hold).min(1.0);
        // The attack timer, once the attack has gone off (the windup only shows in the fighter,
        // and a cancelled one shows nothing): full as it goes off, draining from the right until
        // the next attack may start.
        let cast = attack.swing.filter(|_| attack.released()).map(|swing| 1.0 - swing.cooldown_progress(clock.now(is_me), &class.def().attack));
        let bar = Bar {
            middle,
            size,
            pop: 1.0 + CRIT_BAR_POP * (1.0 - (now - trail.crit_at) / CRIT_BAR_POP_SECONDS).max(0.0).powi(2),
            health: health_fraction,
            trail: trail.fraction,
            fill_color: relation.copied().unwrap_or(Relation::of(is_me, false)).color().into(),
            trail_color: start.mix(&palette::SUN, pale).into(),
            per_tick: tick_every(max_hp, size.x),
            max_hp,
            cast,
        };
        shown.push((eye.distance(over), bar));
    }
    shown.sort_by(|a, b| b.0.total_cmp(&a.0));
    let scale = camera.target_scaling_factor().unwrap_or(1.0);
    let inks = BarInks::new();
    for (_, bar) in shown.iter() {
        draw_bar(&mut rects, bar, &inks, scale);
    }
}

/// The colors every bar is drawn in, made linear once for all of them.
struct BarInks {
    border: LinearRgba,
    inside: LinearRgba,
    mark: LinearRgba,
    cast: LinearRgba,
}

impl BarInks {
    fn new() -> Self {
        BarInks {
            border: palette::ui::PANEL.into(),
            inside: palette::ui::PANEL.with_alpha(0.8).into(),
            mark: palette::ui::PANEL.with_alpha(0.85).into(),
            cast: palette::SUN.into(),
        }
    }
}

/// `bar`'s rectangles, laid out as the UI would lay out a node of that size with a border, its
/// fills, marks and cast bar in it: from its top left (`(0, 0)`, in pixels before `scale`) on a
/// whole pixel, growing from its middle as it pops.
fn draw_bar(rects: &mut Rects, bar: &Bar, inks: &BarInks, scale: f32) {
    let corner = ((bar.middle - bar.size / 2.0) * scale).round();
    let middle = corner + bar.size / 2.0 * scale;
    let at = |p: Vec2| middle + (p - bar.size / 2.0) * bar.pop * scale;
    let mut rect = |from: Vec2, to: Vec2, color: LinearRgba| {
        let (from, to) = (at(from), at(to));
        rects.push((from.x, from.y), (to.x, to.y), color);
    };
    // A frame: see-through inside, its border solid.
    let frame = |rect: &mut dyn FnMut(Vec2, Vec2, LinearRgba), from: Vec2, to: Vec2| {
        rect(from + 1.0, to - 1.0, inks.inside);
        rect(from, Vec2::new(to.x, from.y + 1.0), inks.border);
        rect(Vec2::new(from.x, to.y - 1.0), to, inks.border);
        rect(Vec2::new(from.x, from.y + 1.0), Vec2::new(from.x + 1.0, to.y - 1.0), inks.border);
        rect(Vec2::new(to.x - 1.0, from.y + 1.0), Vec2::new(to.x, to.y - 1.0), inks.border);
    };
    frame(&mut rect, Vec2::ZERO, bar.size);
    let inside = bar.size - 2.0;
    let filled = |fraction: f32| Vec2::ONE + Vec2::new(inside.x * fraction.clamp(0.0, 1.0), inside.y);
    // The health just lost behind the health, then the marks over it, a pixel wide each.
    rect(Vec2::ONE, filled(bar.trail), bar.trail_color);
    rect(Vec2::ONE, filled(bar.health), bar.fill_color);
    let width = inside.x.round();
    for hp in (1..).map(|n| n * bar.per_tick).take_while(|&hp| hp < bar.max_hp) {
        let x = 1.0 + (width * hp as f32 / bar.max_hp as f32).round();
        rect(Vec2::new(x, 1.0), Vec2::new(x + 1.0, 1.0 + inside.y), inks.mark);
    }
    // Hanging right under it, as wide; pale, so it never reads as a (colored) health bar.
    if let Some(cast) = bar.cast {
        let top = Vec2::new(0.0, bar.size.y);
        frame(&mut rect, top, top + Vec2::new(bar.size.x, CAST_BAR_HEIGHT));
        rect(top + 1.0, top + Vec2::new(1.0 + (bar.size.x - 2.0) * cast.clamp(0.0, 1.0), CAST_BAR_HEIGHT - 1.0), inks.cast);
    }
}

/// Move a bar's trail along: up with the health at once (a heal or respawn), held where it was
/// for `TRAIL_HOLD` after each hit (`CRIT_TRAIL_HOLD` while it shows a crit), then draining
/// down to the health. A crit can be seen a frame apart from its health drop (they're separate
/// components), so it starts the hold over too.
fn update_trail(trail: &mut HealthTrail, health: f32, crit_at: Option<f32>, now: f32, dt: f32) {
    if health < trail.last {
        trail.hit_at = now;
    }
    trail.last = health;
    if let Some(crit_at) = crit_at.filter(|&at| at > trail.crit_at) {
        trail.crit_at = crit_at;
        trail.crit = true;
        trail.hit_at = now;
    }
    let hold = if trail.crit { CRIT_TRAIL_HOLD } else { TRAIL_HOLD };
    if health >= trail.fraction {
        trail.fraction = health;
    } else if now - trail.hit_at >= hold {
        let gap = (trail.fraction - health) * (-TRAIL_DRAIN * dt).exp();
        // Snapped shut once too thin to see, so a still bar stops changing.
        trail.fraction = if gap < 0.002 { health } else { health + gap };
    }
    // Not while the crit is fresh: its health drop may be yet to come.
    if trail.fraction <= health && now - trail.crit_at >= hold {
        trail.crit = false;
    }
}

/// A hit when health drops: the camera stirred and a thud for a blow we take, a slap for one
/// landing near us, the body flinching. `Health` is server-authoritative, so this shows
/// confirmed hits only, about one round trip after the swing or shot. A fighter's health is only
/// noted when it's first seen, so the first hit isn't spent on noting it.
fn react_to_hits(
    mut commands: Commands,
    time: Res<Time>,
    mut shake: ResMut<crate::camera::Shake>,
    mut sounds: ResMut<crate::audio::SoundQueue>,
    me: Query<&Pos, (With<Me>, With<PlayerId>)>,
    mut players: Query<
        (Entity, &Health, &Pos, Option<&mut ShownHealth>, Has<Me>),
        Changed<Health>,
    >,
    // When a Hit and a Hurt last sounded.
    mut heard: Local<(f32, f32)>,
) {
    let now = time.elapsed_secs();
    for (player, health, pos, shown, is_me) in &mut players {
        let Some(mut shown) = shown else {
            commands.entity(player).insert(ShownHealth(health.0));
            continue;
        };
        let lost = shown.0 - health.0;
        let was_hit = lost > 0;
        shown.0 = health.0;
        if !was_hit {
            continue;
        }
        // A blow we take stirs the camera and thuds; one landing near us slaps, fainter with
        // distance and lighter for chip damage. Rate-limited, or burning ground pulsing through
        // a pack would be a wall of slaps.
        if is_me {
            shake.add((lost as f32 / 120.0).clamp(0.08, 0.3));
            if now - heard.1 > 0.15 {
                heard.1 = now;
                sounds.push(crate::audio::Sfx::Hurt, (0.5 + lost as f32 / 60.0).min(1.0));
            }
        } else if now - heard.0 > 0.09
            && let Ok(me) = me.single()
        {
            heard.0 = now;
            let heft = (0.4 + lost as f32 / 50.0).min(1.0);
            sounds.push(crate::audio::Sfx::Hit, crate::audio::earshot(pos.0.distance(me.0), 18.0) * heft);
        }
        // Chip damage (a burn's pulse) doesn't flinch the body or spark: a blow does.
        if lost >= SQUASH_DAMAGE {
            commands.entity(player).insert(crate::rig::HitSquash { at: now });
        }
    }
}

/// Dead fighters (waiting to respawn) aren't drawn. Also checked when a fighter first gets its
/// body, in case we first see someone while they're dead.
fn hide_the_dead(
    mut players: Query<(&Health, &mut Visibility), (With<PlayerId>, Or<(Changed<Health>, Added<Mesh3d>)>)>,
) {
    for (health, mut visibility) in &mut players {
        visibility.set_if_neq(shown(health.alive()));
    }
}

/// A number for each new hit in a fighter's `RecentHits` (server-confirmed). The hits already
/// there when we first see a fighter are only noted. Each is as big as the blow
/// (`number_scale`), thrown off a little to a side picked at random (`number_throw`). Damage to us is in our
/// enemies' red; a crit is bigger, flame-gold, and ends in "!". A heal is a green "+".
fn spawn_damage_numbers(
    mut commands: Commands,
    time: Res<Time>,
    world_ui: Single<Entity, With<WorldUi>>,
    mut sounds: ResMut<crate::audio::SoundQueue>,
    mut shake: ResMut<crate::camera::Shake>,
    me: Query<&Pos, (With<Me>, With<PlayerId>)>,
    mut players: Query<(Entity, &RecentHits, &Pos, Option<&mut ShownHits>, Has<Me>), Changed<RecentHits>>,
    mut numbers: Query<(&mut DamageNumber, &mut Text)>,
    mut chimed: Local<f32>,
) {
    let now = time.elapsed_secs();
    for (player, recent, pos, shown, is_me) in &mut players {
        let Some(mut shown) = shown else {
            commands.entity(player).insert(ShownHits(recent.seq()));
            continue;
        };
        let last = std::mem::replace(&mut shown.0, recent.seq());
        // Plain damage first adds to a number just shown, or comes as one number for all of it.
        let new: Vec<_> = recent.0.iter().filter(|hit| hit.seq > last).copied().collect();
        let plain: i32 = new.iter().filter(|hit| hit.kind == HitKind::Damage).map(|hit| hit.amount).sum();
        let merged = plain > 0
            && numbers.iter_mut().filter(|(n, _)| n.player == player && n.merges.is_some() && now - n.born < NUMBER_MERGE_SECONDS).last().is_some_and(|(mut number, mut text)| {
                let total = number.merges.unwrap_or(0) + plain;
                number.merges = Some(total);
                // Kept up a moment longer, growing with what it adds up to.
                number.lasts += now - number.born;
                text.0 = total.to_string();
                number.scale = number_scale(total);
                true
            });
        let mut plain_shown = merged;
        for (i, hit) in new.iter().enumerate() {
            let mut hit = *hit;
            if hit.kind == HitKind::Damage {
                if std::mem::replace(&mut plain_shown, true) {
                    continue;
                }
                hit.amount = plain;
            }
            let crit = hit.kind == HitKind::Crit;
            if crit {
                commands.entity(player).insert(CritAt(time.elapsed_secs()));
                let near = me.single().map_or(1.0, |me| crate::audio::earshot(pos.0.distance(me.0), 24.0));
                sounds.push(crate::audio::Sfx::Crit, 0.4 + 0.5 * near);
                // A nudge of shake for a crit landing on someone else (ours, nearly always,
                // when it's this close); a crit on us already shakes through react_to_hits.
                if !is_me {
                    shake.add(0.045 + 0.055 * near);
                }
            }
            // One soft chime, however the healing comes in: a regen or a full between-waves
            // restore lands as a pulse of heal hits, and a chime per pulse rang like an alarm.
            if hit.kind == HitKind::Heal && is_me && now - *chimed > 1.2 {
                *chimed = now;
                sounds.push(crate::audio::Sfx::Heal, 0.5);
            }
            let (text, color) = match (hit.kind, is_me) {
                (HitKind::Heal, _) => (format!("+{}", hit.amount), palette::HEAL),
                (HitKind::Crit, _) => (format!("{}!", hit.amount), palette::TORCH_FLAME),
                (HitKind::Damage, true) => (hit.amount.to_string(), palette::ENEMY),
                (HitKind::Damage, false) => (hit.amount.to_string(), palette::SUN),
            };
            let throw = number_throw(hit.seq);
            // Alternate sides from the second on, so numbers landing together don't overlap.
            let side = if i % 2 == 0 { 1.0 } else { -1.0 };
            let lasts = if crit { CRIT_NUMBER_SECONDS } else { NUMBER_SECONDS };
            commands.spawn((
                DamageNumber { player, born: now, lasts, crit, dx: side * NUMBER_SPREAD * i.div_ceil(2) as f32, throw, scale: number_scale(hit.amount), merges: (hit.kind == HitKind::Damage).then_some(hit.amount) },
                ChildOf(*world_ui),
                ui_text(text, NUMBER_SIZE.1 * if crit { CRIT_NUMBER_GROWTH } else { 1.0 }, color),
                TextShadow { offset: Vec2::splat(2.0), color: palette::ui::PANEL },
                Node { position_type: PositionType::Absolute, ..default() },
                GlobalZIndex(5),
                Visibility::Hidden,
            ));
        }
    }
}

/// Damage numbers follow their fighter's health bar, thrown up off it a little to one side,
/// slowing and falling back as they fade out; a crit pops in big and settles. Where it is, the
/// throw, the size and the pop are its `UiTransform`, which doesn't make Bevy lay out the UI
/// again, and like `place_bars` only what changed is written.
fn float_damage_numbers(
    mut commands: Commands,
    time: Res<Time>,
    camera: Single<(&Camera, &GlobalTransform), With<crate::camera::CameraRig>>,
    players: Query<&Transform, With<PlayerId>>,
    mut numbers: Query<(Entity, &DamageNumber, &ComputedNode, &mut UiTransform, &mut TextColor, &mut Visibility)>,
) {
    let (camera, camera_transform) = *camera;
    for (number, info, computed, mut transform, mut color, mut visibility) in &mut numbers {
        let age = time.elapsed_secs() - info.born;
        let t = age / info.lasts;
        let (Ok(body), true) = (players.get(info.player), t < 1.0) else {
            commands.entity(number).despawn();
            continue;
        };
        let screen = over_bar(camera, camera_transform, body, 1.0);
        visibility.set_if_neq(shown(screen.is_some()));
        let Some(screen) = screen else { continue };
        let size = computed.size() * computed.inverse_scale_factor();
        // Its bottom on the bar, as big as it's drawn (scaled about its middle), and thrown off it.
        let at = Vec2::new(screen.x + info.dx - size.x / 2.0, screen.y - BAR_SIZE.y - size.y * (1.0 + info.scale) / 2.0);
        let flown = number_flown(info.throw, age);
        let pop = if info.crit { 1.0 + CRIT_POP * (1.0 - t / CRIT_POP_PART).max(0.0).powi(2) } else { 1.0 };
        let next = UiTransform { translation: Val2::px(at.x + flown.x, at.y - flown.y), scale: Vec2::splat(info.scale * pop), ..default() };
        transform.set_if_neq(next);
        let alpha = ((1.0 - t) / NUMBER_FADE_PART).min(1.0);
        if color.0.alpha() != alpha {
            color.0.set_alpha(alpha);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_number_grows_with_the_damage_up_to_full_size_at_full_damage() {
        let scales: Vec<f32> = (0..=200).map(number_scale).collect();
        assert!((scales[0] - NUMBER_SIZE.0 / NUMBER_SIZE.1).abs() < 1e-6, "nothing's number isn't the smallest");
        for amount in 1..=NUMBER_FULL_DAMAGE as usize {
            assert!(scales[amount] > scales[amount - 1], "{amount} damage's number is no bigger than {}'s", amount - 1);
        }
        for amount in NUMBER_FULL_DAMAGE as usize..=200 {
            assert_eq!(scales[amount], 1.0, "{amount} damage's number isn't full size");
        }
        // Small blows still read: a tenth of full damage is well over the smallest.
        assert!(scales[8] - scales[0] > (1.0 - scales[0]) * 0.15);
    }

    #[test]
    fn numbers_are_thrown_a_little_off_straight_up_to_either_side() {
        let throws: Vec<Vec2> = (0..2000).map(number_throw).collect();
        for (seq, throw) in throws.iter().enumerate() {
            assert!((throw.length() - NUMBER_THROW).abs() < 1e-3, "hit {seq}: thrown at {} px/s", throw.length());
            let tilt = throw.x.atan2(throw.y).abs();
            assert!(tilt >= NUMBER_TILT.0 - 1e-5 && tilt <= NUMBER_TILT.1 + 1e-5, "hit {seq}: thrown {} degrees off straight up", tilt.to_degrees());
        }
        // Either side about as often, and not the same side hit after hit.
        let right = throws.iter().filter(|throw| throw.x > 0.0).count();
        assert!((800..=1200).contains(&right), "{right} of 2000 thrown right");
        let swaps = throws.windows(2).filter(|w| (w[0].x > 0.0) != (w[1].x > 0.0)).count();
        assert!(swaps > 700, "only {swaps} changes of side in 2000 hits");
        // How far off straight up differs from hit to hit.
        let (least, most) = throws.iter().map(|throw| throw.x.abs()).fold((f32::INFINITY, 0.0f32), |(lo, hi), x| (lo.min(x), hi.max(x)));
        assert!(most - least > (NUMBER_TILT.1 - NUMBER_TILT.0).sin() * NUMBER_THROW * 0.8);
    }

    #[test]
    fn a_number_flies_up_slows_and_falls_back_before_it_is_gone() {
        for seq in 0..50 {
            let throw = number_throw(seq);
            let path: Vec<Vec2> = (0..=90).map(|i| number_flown(throw, NUMBER_SECONDS * i as f32 / 90.0)).collect();
            let top = path.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max);
            // Up well clear of the bar, then back down to it while it still shows.
            assert!(top > NUMBER_SIZE.1, "hit {seq}: thrown only {top} px up");
            let peak = path.iter().position(|p| p.y == top).unwrap();
            assert!(path[..=peak].windows(2).all(|w| w[1].y >= w[0].y) && path[peak..].windows(2).all(|w| w[1].y <= w[0].y), "hit {seq}: doesn't rise and then fall");
            assert!(path.last().unwrap().y < 0.0, "hit {seq}: never falls back past the bar");
            // Drifting to its side all the way, never back across.
            assert!(path[1..].iter().all(|p| p.x.signum() == throw.x.signum()));
        }
    }
}
