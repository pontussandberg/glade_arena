//! Combat feedback: a health bar over every fighter with a cast bar under it while it winds up
//! an attack, a white flash and a damage number when one takes damage (a crit's bigger and
//! golden), a green number when one is healed, and dead fighters disappearing until they
//! respawn.

use arena_shared::protocol::*;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use lightyear::core::time::TickInstant;
use lightyear::interpolation::timeline::InterpolationTimeline;
use lightyear::prelude::*;

use crate::camera::CameraPlaced;
use crate::arena::{palette, to_world};
use crate::render::{Relation, set_fill, shown, ui_text};
use crate::render::GameUi;
use crate::rig::SeenThrows;
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
const BAR_SIZE: Vec2 = Vec2::new(56.0, 7.0);
const BAR_HEIGHT: f32 = 2.3;
/// The cast bar hangs right under the health bar, as wide and a bit thinner.
const CAST_BAR_HEIGHT: f32 = 5.0;
const FLASH_SECONDS: f32 = 0.12;
/// How bright the flash is (emissive white on the fighter's own material).
const FLASH_GLOW: f32 = 3.0;

/// Damage numbers: text size in pixels (a crit's, and how much bigger it pops in at, settling
/// over the first `CRIT_POP_PART` of its life), how long one shows (fading out over the last
/// `NUMBER_FADE_PART`), how far it rises above the health bar meanwhile, and how far apart
/// numbers landing together spread sideways.
const NUMBER_SIZE: f32 = 16.0;
const CRIT_NUMBER_SIZE: f32 = 26.0;
const CRIT_POP: f32 = 0.6;
const CRIT_POP_PART: f32 = 0.15;
const NUMBER_SECONDS: f32 = 0.9;
const CRIT_NUMBER_SECONDS: f32 = 1.2;
const NUMBER_FADE_PART: f32 = 0.4;
const NUMBER_RISE: f32 = 36.0;
const NUMBER_SPREAD: f32 = 14.0;

/// A fighter's bars: the health bar (this entity) and its parts.
#[derive(Component)]
struct Bars {
    player: Entity,
    health_fill: Entity,
    cast: Entity,
    cast_fill: Entity,
}

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
fn spawn_bars(mut commands: Commands, world_ui: Single<Entity, With<WorldUi>>, new: Query<(Entity, Has<Predicted>), (With<PlayerId>, Added<Mesh3d>)>) {
    let fill = |color: Color| (Node { width: percent(100.0), height: percent(100.0), ..default() }, BackgroundColor(color));
    let frame = |node: Node| {
        (
            Node { border: UiRect::all(px(1.0)), position_type: PositionType::Absolute, ..node },
            BackgroundColor(palette::ui::PANEL.with_alpha(0.8)),
            BorderColor::all(palette::ui::PANEL),
        )
    };
    for (player, is_me) in &new {
        let health_fill = commands.spawn(fill(Relation::of(is_me, false).color())).id();
        // Pale, so it never reads as a (colored) health bar.
        let cast_fill = commands.spawn(fill(palette::SUN)).id();
        // Absolute inside the health bar's border, so shift left by it to line the two up.
        let cast = commands
            .spawn((
                frame(Node { left: px(-1.0), top: px(BAR_SIZE.y - 1.0), width: px(BAR_SIZE.x), height: px(CAST_BAR_HEIGHT), ..default() }),
                Visibility::Hidden,
            ))
            .add_child(cast_fill)
            .id();
        commands
            .spawn((
                Bars { player, health_fill, cast, cast_fill },
                ChildOf(*world_ui),
                frame(Node { width: px(BAR_SIZE.x), height: px(BAR_SIZE.y), ..default() }),
                Visibility::Hidden,
            ))
            .add_children(&[health_fill, cast]);
    }
}

/// A health bar is in its fighter's `Relation` color: ours, an ally's or an enemy's.
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
fn over_bar(camera: &Camera, camera_transform: &GlobalTransform, pos: &Pos) -> Option<Vec2> {
    camera.world_to_viewport(camera_transform, to_world(pos.0, BAR_HEIGHT)).ok()
}

/// "Now" in fractional ticks, on the timeline a fighter is shown on, to compare with its
/// `AttackState`: ours is predicted (our own windup starts at the click), others' are
/// interpolated (their windup plays out in step with their delayed position).
#[derive(SystemParam)]
pub(crate) struct AttackClock<'w, 's> {
    local: SyncedLocalTimeline<'w, 's>,
    fixed: Res<'w, Time<Fixed>>,
    interpolation: Option<Res<'w, InterpolationTimeline>>,
}

impl AttackClock<'_, '_> {
    pub(crate) fn now(&self, predicted: bool) -> f32 {
        let ticks = |t: TickInstant| t.tick().0 as f32 + t.overstep().to_f32();
        match &self.interpolation {
            Some(timeline) if !predicted => ticks(timeline.now),
            _ => ticks(self.local.instant(&self.fixed)),
        }
    }
}

/// Keep each fighter's bars over it and filled (the cast bar by `AttackClock`). Only writes what
/// changed, so a still scene doesn't make Bevy lay out the UI again every frame.
fn place_bars(
    mut commands: Commands,
    clock: AttackClock,
    camera: Single<(&Camera, &GlobalTransform)>,
    players: Query<(&Pos, &ClassId, Option<&Health>, &AttackState, Option<&SeenThrows>, Has<Predicted>)>,
    mut bars: Query<(Entity, &Bars, &mut Node, &mut Visibility)>,
    mut parts: Query<(&mut Node, &mut Visibility), Without<Bars>>,
) {
    let (camera, camera_transform) = *camera;
    for (bar, ids, mut node, mut visibility) in &mut bars {
        let Ok((pos, class, health, attack, seen, is_me)) = players.get(ids.player) else {
            commands.entity(bar).despawn();
            continue;
        };
        let alive = health.is_none_or(Health::alive);
        let Some(screen) = over_bar(camera, camera_transform, pos).filter(|_| alive) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        visibility.set_if_neq(Visibility::Inherited);
        let (left, top) = (px(screen.x - BAR_SIZE.x / 2.0), px(screen.y - BAR_SIZE.y / 2.0));
        if node.left != left || node.top != top {
            node.left = left;
            node.top = top;
        }

        let mut fill_to = |part: Entity, fraction: f32| {
            if let Ok((mut fill, _)) = parts.get_mut(part) {
                set_fill(&mut fill, fraction);
            }
        };
        fill_to(ids.health_fill, health.map_or(1.0, |h| h.0 as f32 / class.def().max_hp as f32));
        let windup = SeenThrows::windup(seen, attack);
        if let Some(windup) = windup {
            fill_to(ids.cast_fill, windup.progress(clock.now(is_me), *class));
        }
        if let Ok((_, mut cast_visibility)) = parts.get_mut(ids.cast) {
            cast_visibility.set_if_neq(shown(windup.is_some()));
        }
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
    mut players: Query<
        (Entity, &Health, &MeshMaterial3d<StandardMaterial>, Option<&mut ShownHealth>, Option<&mut HitFlash>),
        Or<(Changed<Health>, Added<MeshMaterial3d<StandardMaterial>>)>,
    >,
) {
    let until = time.elapsed_secs() + FLASH_SECONDS;
    for (player, health, material, shown, flashing) in &mut players {
        let Some(mut shown) = shown else {
            commands.entity(player).insert(ShownHealth(health.0));
            continue;
        };
        let was_hit = health.0 < shown.0;
        shown.0 = health.0;
        if !was_hit {
            continue;
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
    mut players: Query<(Entity, &RecentHits, Option<&mut ShownHits>, Has<Predicted>), Changed<RecentHits>>,
) {
    for (player, recent, shown, is_me) in &mut players {
        let Some(mut shown) = shown else {
            commands.entity(player).insert(ShownHits(recent.seq()));
            continue;
        };
        let last = std::mem::replace(&mut shown.0, recent.seq());
        for (i, hit) in recent.0.iter().filter(|hit| hit.seq > last).enumerate() {
            let crit = hit.kind == HitKind::Crit;
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
                DamageNumber { player, born: time.elapsed_secs(), lasts, crit, dx: side * NUMBER_SPREAD * i.div_ceil(2) as f32 },
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
        let screen = over_bar(camera, camera_transform, pos);
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
