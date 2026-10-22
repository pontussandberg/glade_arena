//! Combat feedback: a health bar over every fighter with a cast bar under it while it winds up
//! an attack, the health lost lingering on the bar a moment before it drains (gold, longer, and
//! with the bar popping on a crit), a white flash and a damage number when one takes damage (a crit's bigger and
//! golden), a green number when one is healed, and dead fighters disappearing until they
//! respawn.

use arena_shared::config::TICK_HZ;
use arena_shared::protocol::*;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use lightyear::core::time::TickInstant;
use lightyear::interpolation::timeline::InterpolationTimeline;
use lightyear::prelude::client::RemoteTimeline;
use lightyear::prelude::*;

use crate::Me;

use crate::camera::CameraPlaced;
use crate::arena::{palette, to_world};
use crate::render::{Relation, set_fill, shown, ui_text};
use crate::render::GameUi;
use crate::rooms::Screen;

pub struct FeedbackPlugin;

impl Plugin for FeedbackPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_world_ui);
        app.add_systems(
            Update,
            (
                (spawn_bars, color_bars).chain(),
                place_bars.in_set(CameraPlaced).run_if(in_state(Screen::InGame)),
                flash_on_hit,
                end_flashes,
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
/// The most marks a bar has.
const MOST_TICKS: i32 = 12;
const FLASH_SECONDS: f32 = 0.12;
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
/// How bright the flash is (emissive white on the fighter's own material).
const FLASH_GLOW: f32 = 3.0;
/// Damage under this doesn't flinch the body (`rig::HitSquash`) or spark: a damage-over-time
/// pulse flashes and counts, but only a real blow rocks the figure.
const SQUASH_DAMAGE: i32 = 4;

/// Damage numbers: text size in pixels (a crit's, and how much bigger it pops in at, settling
/// over the first `CRIT_POP_PART` of its life), how long one shows (fading out over the last
/// `NUMBER_FADE_PART`), how far it rises above the health bar meanwhile, and how far apart
/// numbers landing together spread sideways.
const NUMBER_SIZE: f32 = 16.0;
const CRIT_NUMBER_SIZE: f32 = 26.0;
const CRIT_POP: f32 = 0.6;
const CRIT_POP_PART: f32 = 0.15;
const NUMBER_SECONDS: f32 = 0.9;
/// Plain damage to one fighter landing this soon (seconds) after its last number adds to that
/// number instead of floating up a new one, so a flurry (arcs, bursts, a barrage) reads as one.
const NUMBER_MERGE_SECONDS: f32 = 0.25;
const CRIT_NUMBER_SECONDS: f32 = 1.2;
const NUMBER_FADE_PART: f32 = 0.4;
const NUMBER_RISE: f32 = 36.0;
const NUMBER_SPREAD: f32 = 14.0;

/// A fighter's bars: the health bar (this entity) and its parts.
#[derive(Component)]
struct Bars {
    player: Entity,
    health_fill: Entity,
    trail: Entity,
    cast: Entity,
    cast_fill: Entity,
}

/// The health a bar's trail (`Bars::trail`) still shows, as a fraction of the most; the health
/// it saw last frame, to tell a new hit; when the last hit landed; and when the last crit we
/// saw (`CritAt`) landed, and whether the trail still shows one (until it has drained).
#[derive(Component)]
struct HealthTrail {
    fraction: f32,
    last: f32,
    hit_at: f32,
    crit_at: f32,
    crit: bool,
}

/// When this fighter last took a crit we showed a number for.
#[derive(Component)]
struct CritAt(f32);

/// The last health we showed for this fighter, to tell a hit from a heal or a respawn.
#[derive(Component)]
struct ShownHealth(i32);

/// A fighter flashing white after a hit, until `until`.
#[derive(Component)]
struct HitFlash {
    until: f32,
}

/// The running number (`RecentHits::seq`) of the last hit on this fighter we showed a number for.
#[derive(Component)]
struct ShownHits(u32);

/// A damage number floating up from over `player`'s health bar, from `born` for `lasts` seconds.
#[derive(Component)]
struct DamageNumber {
    player: Entity,
    born: f32,
    lasts: f32,
    crit: bool,
    /// Sideways from the bar's center, in pixels.
    dx: f32,
    /// For plain damage, how much it shows, for more landing on it (`NUMBER_MERGE_SECONDS`).
    merges: Option<i32>,
}

/// What floats over the fighters (bars, damage numbers): under one screen-filling `GameUi` root,
/// so it's all hidden outside the arena, where others can still be fighting.
#[derive(Component)]
struct WorldUi;

fn spawn_world_ui(mut commands: Commands) {
    commands.spawn((
        WorldUi,
        GameUi,
        Node { position_type: PositionType::Absolute, width: percent(100.0), height: percent(100.0), ..default() },
        Pickable::IGNORE,
    ));
}

/// Each fighter gets its bars once it has a body.
#[allow(clippy::type_complexity)]
fn spawn_bars(
    mut commands: Commands,
    world_ui: Single<Entity, With<WorldUi>>,
    // The shopkeeper has none: no one can hurt them.
    new: Query<(Entity, &ClassId, Has<Me>, Option<&MaxHealth>, Has<Elite>), (With<PlayerId>, Added<Mesh3d>, Without<Shopkeeper>)>,
) {
    let fill = |color: Color| (Node { width: percent(100.0), height: percent(100.0), ..default() }, BackgroundColor(color));
    let frame = |node: Node| {
        (
            Node { border: UiRect::all(px(1.0)), position_type: PositionType::Absolute, ..node },
            BackgroundColor(palette::ui::PANEL.with_alpha(0.8)),
            BorderColor::all(palette::ui::PANEL),
        )
    };
    for (player, class, is_me, max, elite) in &new {
        let size = bar_size(*class, elite);
        // Behind the fill (spawned before it), reaching past it to show health just lost.
        let trail = commands
            .spawn((Node { position_type: PositionType::Absolute, width: percent(100.0), height: percent(100.0), ..default() }, BackgroundColor(Color::WHITE)))
            .id();
        let health_fill = commands.spawn(fill(Relation::of(is_me, false).color())).id();
        // Over the fill (spawned after it), at every `HP_PER_TICK` of the fighter's health.
        let max_hp = max.map_or(class.def().max_hp, |m| m.0);
        // A tough one's marks further apart, so they don't run together.
        let per_tick = HP_PER_TICK * (max_hp / (HP_PER_TICK * MOST_TICKS)).max(1);
        let ticks: Vec<Entity> = (1..)
            .map(|n| n * per_tick)
            .take_while(|&hp| hp < max_hp)
            .map(|hp| {
                let left = percent(100.0 * hp as f32 / max_hp as f32);
                let node = Node { position_type: PositionType::Absolute, left, width: px(1.0), height: percent(100.0), ..default() };
                commands.spawn((node, BackgroundColor(palette::ui::PANEL.with_alpha(0.85)))).id()
            })
            .collect();
        // Pale, so it never reads as a (colored) health bar.
        let cast_fill = commands.spawn(fill(palette::SUN)).id();
        // Absolute inside the health bar's border, so shift left by it to line the two up.
        let cast = commands
            .spawn((
                frame(Node { left: px(-1.0), top: px(size.y - 1.0), width: px(size.x), height: px(CAST_BAR_HEIGHT), ..default() }),
                Visibility::Hidden,
            ))
            .add_child(cast_fill)
            .id();
        commands
            .spawn((
                Bars { player, health_fill, trail, cast, cast_fill },
                HealthTrail { fraction: 1.0, last: 1.0, hit_at: 0.0, crit_at: 0.0, crit: false },
                ChildOf(*world_ui),
                frame(Node { width: px(size.x), height: px(size.y), ..default() }),
                Visibility::Hidden,
            ))
            .add_child(trail)
            .add_child(health_fill)
            .add_children(&ticks)
            .add_child(cast);
    }
}

/// A health bar is in its fighter's `Relation` color: ours, an ally's, an enemy's or a
/// monster's.
fn color_bars(
    players: Query<&Relation>,
    changed: Query<(), Changed<Relation>>,
    new: Query<(), Added<Bars>>,
    bars: Query<(Entity, &Bars)>,
    mut fills: Query<&mut BackgroundColor>,
) {
    for (bar, ids) in &bars {
        if !(changed.contains(ids.player) || new.contains(bar)) {
            continue;
        }
        if let (Ok(relation), Ok(mut fill)) = (players.get(ids.player), fills.get_mut(ids.health_fill)) {
            fill.set_if_neq(BackgroundColor(relation.color()));
        }
    }
}

/// Where a fighter's health bar is centered on screen (`None` off camera).
fn over_bar(camera: &Camera, camera_transform: &GlobalTransform, pos: &Pos, size: f32) -> Option<Vec2> {
    camera.world_to_viewport(camera_transform, to_world(pos.0, BAR_HEIGHT * size.max(0.8))).ok()
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

/// Keep each fighter's bars over it and filled (the cast bar by `AttackClock`). Only writes what
/// changed, so a still scene doesn't make Bevy lay out the UI again every frame.
fn place_bars(
    mut commands: Commands,
    time: Res<Time>,
    clock: AttackClock,
    camera: Single<(&Camera, &GlobalTransform)>,
    players: Query<(&Pos, &ClassId, Option<&Health>, Option<&CritAt>, &AttackState, Has<Me>, (Option<&MaxHealth>, Has<Elite>))>,
    mut bars: Query<(Entity, &Bars, &mut HealthTrail, &mut Node, &mut UiTransform, &mut Visibility)>,
    mut parts: Query<(&mut Node, &mut Visibility), Without<Bars>>,
    mut colors: Query<&mut BackgroundColor>,
) {
    let (camera, camera_transform) = *camera;
    let now = time.elapsed_secs();
    for (bar, ids, mut trail, mut node, mut transform, mut visibility) in &mut bars {
        let Ok((pos, class, health, crit_at, attack, is_me, (max, elite))) = players.get(ids.player) else {
            commands.entity(bar).despawn();
            continue;
        };
        let max_hp = max.map_or(class.def().max_hp, |m| m.0);
        let health_fraction = health.map_or(1.0, |h| h.0 as f32 / max_hp as f32);
        update_trail(&mut trail, health_fraction, crit_at.map(|c| c.0), now, time.delta_secs());
        let alive = health.is_none_or(Health::alive);
        let height = crate::rig::drawn_size(*class, elite);
        let Some(screen) = over_bar(camera, camera_transform, pos, height).filter(|_| alive) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        visibility.set_if_neq(Visibility::Inherited);
        let size = bar_size(*class, elite);
        let (left, top) = (px(screen.x - size.x / 2.0), px(screen.y - size.y / 2.0));
        if node.left != left || node.top != top {
            node.left = left;
            node.top = top;
        }

        let mut fill_to = |part: Entity, fraction: f32| {
            if let Ok((mut fill, _)) = parts.get_mut(part) {
                set_fill(&mut fill, fraction);
            }
        };
        fill_to(ids.health_fill, health_fraction);
        fill_to(ids.trail, trail.fraction);
        if let Ok(mut color) = colors.get_mut(ids.trail) {
            let (start, hold) = if trail.crit { (palette::TORCH_FLAME, CRIT_TRAIL_HOLD) } else { (Color::WHITE, TRAIL_HOLD) };
            let pale = ((now - trail.hit_at) / hold).min(1.0);
            color.set_if_neq(BackgroundColor(start.mix(&palette::SUN, pale)));
        }
        let pop = 1.0 + CRIT_BAR_POP * (1.0 - (now - trail.crit_at) / CRIT_BAR_POP_SECONDS).max(0.0).powi(2);
        transform.set_if_neq(UiTransform { scale: Vec2::splat(pop), ..default() });
        // The attack timer, once the attack has gone off (the windup only shows in the fighter,
        // and a cancelled one shows nothing): full as it goes off, draining from the right until
        // the next attack may start.
        let cooling = attack.swing.filter(|_| attack.released());
        if let Some(swing) = cooling {
            fill_to(ids.cast_fill, 1.0 - swing.cooldown_progress(clock.now(is_me), &class.def().attack));
        }
        if let Ok((_, mut cast_visibility)) = parts.get_mut(ids.cast) {
            cast_visibility.set_if_neq(shown(cooling.is_some()));
        }
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

/// Flash white when health drops. `Health` is server-authoritative, so this shows confirmed
/// hits only, about one round trip after the swing or shot. The fighter's material is its own
/// (one per player), so the flash brightens it in place. Also run when a fighter first gets its
/// body, to note its health then: it arrives before the body does, so waiting for the next
/// change would spend the first hit on noting it, without a flash.
fn flash_on_hit(
    mut commands: Commands,
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut shake: ResMut<crate::camera::Shake>,
    mut sounds: ResMut<crate::audio::SoundQueue>,
    me: Query<&Pos, (With<Me>, With<PlayerId>)>,
    mut players: Query<
        (Entity, &Health, &Pos, &MeshMaterial3d<StandardMaterial>, Option<&mut ShownHealth>, Option<&mut HitFlash>, Has<Me>),
        Or<(Changed<Health>, Added<MeshMaterial3d<StandardMaterial>>)>,
    >,
    // When a Hit and a Hurt last sounded.
    mut heard: Local<(f32, f32)>,
) {
    let now = time.elapsed_secs();
    let until = now + FLASH_SECONDS;
    for (player, health, pos, material, shown, flashing, is_me) in &mut players {
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
        match flashing {
            Some(mut flash) => flash.until = until,
            None => {
                if let Some(mut m) = materials.get_mut(&material.0) {
                    m.emissive = LinearRgba::WHITE * FLASH_GLOW;
                }
                commands.entity(player).insert(HitFlash { until });
            }
        }
    }
}

fn end_flashes(
    mut commands: Commands,
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    flashing: Query<(Entity, &HitFlash, &MeshMaterial3d<StandardMaterial>)>,
) {
    for (player, flash, material) in &flashing {
        if time.elapsed_secs() >= flash.until {
            if let Some(mut m) = materials.get_mut(&material.0) {
                m.emissive = LinearRgba::BLACK;
            }
            commands.entity(player).remove::<HitFlash>();
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

/// A number for each new hit in a fighter's `RecentHits` (server-confirmed, like the flash). The
/// hits already there when we first see a fighter are only noted. Damage to us is in our
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
                // Kept up a moment longer, without dropping back to the bar.
                number.born = number.born.max(now - 0.2);
                text.0 = total.to_string();
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
                // when it's this close); a crit on us already shakes through flash_on_hit.
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
            let (text, size, color) = match (hit.kind, is_me) {
                (HitKind::Heal, _) => (format!("+{}", hit.amount), NUMBER_SIZE, palette::HEAL),
                (HitKind::Crit, _) => (format!("{}!", hit.amount), CRIT_NUMBER_SIZE, palette::TORCH_FLAME),
                (HitKind::Damage, true) => (hit.amount.to_string(), NUMBER_SIZE, palette::ENEMY),
                (HitKind::Damage, false) => (hit.amount.to_string(), NUMBER_SIZE, palette::SUN),
            };
            // Alternate sides from the second on, so numbers landing together don't overlap.
            let side = if i % 2 == 0 { 1.0 } else { -1.0 };
            let lasts = if crit { CRIT_NUMBER_SECONDS } else { NUMBER_SECONDS };
            commands.spawn((
                DamageNumber { player, born: now, lasts, crit, dx: side * NUMBER_SPREAD * i.div_ceil(2) as f32, merges: (hit.kind == HitKind::Damage).then_some(hit.amount) },
                ChildOf(*world_ui),
                ui_text(text, size, color),
                TextShadow { offset: Vec2::splat(2.0), color: palette::ui::PANEL },
                Node { position_type: PositionType::Absolute, ..default() },
                GlobalZIndex(5),
                Visibility::Hidden,
            ));
        }
    }
}

/// Damage numbers follow their fighter's health bar, rising from just over it and fading out; a
/// crit pops in big and settles. The rise and pop are `UiTransform`s, which don't make Bevy lay
/// out the UI again, and like `place_bars` only what changed is written.
fn float_damage_numbers(
    mut commands: Commands,
    time: Res<Time>,
    camera: Single<(&Camera, &GlobalTransform)>,
    players: Query<&Pos>,
    mut numbers: Query<(Entity, &DamageNumber, &ComputedNode, &mut Node, &mut UiTransform, &mut TextColor, &mut Visibility)>,
) {
    let (camera, camera_transform) = *camera;
    for (number, info, computed, mut node, mut transform, mut color, mut visibility) in &mut numbers {
        let t = (time.elapsed_secs() - info.born) / info.lasts;
        let (Ok(pos), true) = (players.get(info.player), t < 1.0) else {
            commands.entity(number).despawn();
            continue;
        };
        let screen = over_bar(camera, camera_transform, pos, 1.0);
        visibility.set_if_neq(shown(screen.is_some()));
        let Some(screen) = screen else { continue };
        let size = computed.size() * computed.inverse_scale_factor();
        let (left, top) = (px(screen.x + info.dx - size.x / 2.0), px(screen.y - BAR_SIZE.y - size.y));
        if node.left != left || node.top != top {
            node.left = left;
            node.top = top;
        }
        // Eased out: quick off the bar, slowing as it fades.
        let rise = NUMBER_RISE * (1.0 - (1.0 - t).powi(2));
        let pop = if info.crit { 1.0 + CRIT_POP * (1.0 - t / CRIT_POP_PART).max(0.0).powi(2) } else { 1.0 };
        let next = UiTransform { translation: Val2::px(0.0, -rise), scale: Vec2::splat(pop), ..default() };
        transform.set_if_neq(next);
        let alpha = ((1.0 - t) / NUMBER_FADE_PART).min(1.0);
        if color.0.alpha() != alpha {
            color.0.set_alpha(alpha);
        }
    }
}
