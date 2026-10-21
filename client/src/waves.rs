//! The waves mode, on screen: the run's banner at the top (the wave, the shop's countdown and the
//! way to say we're ready, the monsters left, who's down), a boss's health under it, the
//! end of a lost run across the middle; our gold and items over the action bar; the merchant's
//! shop, open while we stand by them; and what items and monsters do (`Fx`), and wisps circling
//! whoever carries a Wisp Lantern.

use arena_shared::items::{self, Item, MAX_LEVEL, SLOTS, Satchel};
use arena_shared::protocol::*;
use arena_shared::rooms::Phase;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use lightyear::prelude::client::*;
use lightyear::prelude::*;

use crate::Me;
use crate::arena::palette::ui::ACCENT;
use crate::arena::{self, palette, to_world};
use crate::feedback::AttackClock;
use crate::item_icons::{ICON_PIXELS, ItemIcons, load_item_icons};
use crate::render::{GameUi, HudButton, button, clicked, set_fill, shown, ui_text};
use crate::rooms::{CurrentRoom, request};
use crate::tooltip::{Side, hover_shows, tip, tip_note, tip_text, tip_title};

pub struct WavesPlugin;

impl Plugin for WavesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (spawn_hud, load_fx_looks, load_item_icons));
        app.add_systems(
            Update,
            (
                (show_banner, show_boss, show_over, ready_up, leave_when_over).chain(),
                (show_satchel, open_shop, show_shop, trade).chain(),
                circle_wisps,
                (name_in_world, place_names).chain(),
            ),
        );
    }
}

/// Spacing (pixels).
const GAP: f32 = 8.0;
/// An item's tile in our satchel, and in the shop.
const SLOT: f32 = 44.0;
/// The shop's width.
const SHOP_WIDTH: f32 = 470.0;
const TIP_WIDTH: f32 = 300.0;

/// The banner at the top: its lines (rebuilt as the run changes).
#[derive(Component)]
struct Banner;

/// The boss's bar under the banner, its name and fill.
#[derive(Component)]
struct BossBar {
    name: Entity,
    fill: Entity,
}

/// A lost run's end, across the middle.
#[derive(Component)]
struct OverBanner;

/// The "Ready" button on the banner.
#[derive(Component)]
struct ReadyButton;

/// Our gold and items, over the action bar.
#[derive(Component)]
struct SatchelBar;

/// The merchant's shop, at the right.
#[derive(Component)]
struct ShopPanel;

/// A button in the shop: buy (or raise) an item; or, on our item's tile while the shop's open,
/// sell what's in that slot.
#[derive(Component)]
struct BuyButton(Item);

#[derive(Component)]
struct SellButton(u8);

/// Whether the shop is open: we're by the merchant in a run.
#[derive(Resource, Default, PartialEq)]
struct ShopOpen(bool);

fn spawn_hud(mut commands: Commands) {
    commands.init_resource::<ShopOpen>();
    let top_center = |top: Val| Node { position_type: PositionType::Absolute, width: percent(100.0), top, justify_content: JustifyContent::Center, ..default() };
    commands.spawn((GameUi, top_center(px(8.0)), Pickable::IGNORE)).with_child((
        Banner,
        HudButton,
        Node {
            display: Display::None,
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(3.0),
            padding: UiRect::axes(px(18.0), px(7.0)),
            border: UiRect::bottom(px(2.0)),
            ..default()
        },
        BackgroundColor(palette::ui::PANEL.with_alpha(0.88)),
        BorderColor::all(ACCENT),
    ));
    let mut name = None;
    let mut fill = None;
    let boss = commands
        .spawn((
            Node { display: Display::None, flex_direction: FlexDirection::Column, align_items: AlignItems::Center, row_gap: px(3.0), ..default() },
            Pickable::IGNORE,
        ))
        .with_children(|boss| {
            name = Some(boss.spawn(ui_text("", 15.0, palette::TORCH_FLAME)).id());
            boss.spawn((
                Node { width: px(420.0), height: px(12.0), border: UiRect::all(px(1.0)), ..default() },
                BackgroundColor(palette::ui::BACKDROP.with_alpha(0.9)),
                BorderColor::all(palette::ui::MUTED),
            ))
            .with_children(|bar| {
                fill = Some(bar.spawn((Node { width: percent(100.0), height: percent(100.0), ..default() }, BackgroundColor(palette::MONSTER))).id());
            });
        })
        .id();
    commands.entity(boss).insert(BossBar { name: name.expect("spawned"), fill: fill.expect("spawned") });
    commands.spawn((GameUi, top_center(px(92.0)), Pickable::IGNORE)).add_child(boss);
    commands.spawn((GameUi, top_center(percent(26.0)), Pickable::IGNORE)).with_child((
        OverBanner,
        Node {
            display: Display::None,
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(6.0),
            padding: UiRect::axes(px(44.0), px(18.0)),
            ..default()
        },
        BackgroundColor(palette::ui::BACKDROP.with_alpha(0.88)),
    ));
    commands.spawn((
        GameUi,
        Node { position_type: PositionType::Absolute, width: percent(100.0), bottom: px(100.0), justify_content: JustifyContent::Center, ..default() },
        Pickable::IGNORE,
    ))
    .with_child((SatchelBar, HudButton, Node { display: Display::None, column_gap: px(6.0), align_items: AlignItems::Center, padding: UiRect::all(px(5.0)), ..default() }, BackgroundColor(palette::ui::PANEL.with_alpha(0.8))));
    commands.spawn((
        GameUi,
        ShopPanel,
        HudButton,
        Node {
            display: Display::None,
            position_type: PositionType::Absolute,
            right: px(12.0),
            top: px(64.0),
            width: px(SHOP_WIDTH),
            flex_direction: FlexDirection::Column,
            row_gap: px(4.0),
            padding: UiRect::all(px(12.0)),
            border: UiRect::all(px(1.0)),
            ..default()
        },
        BackgroundColor(palette::ui::PANEL.with_alpha(0.95)),
        BorderColor::all(ACCENT.with_alpha(0.6)),
        Interaction::default(),
    ));
}

/// The banner: rebuilt whenever the room changes.
fn show_banner(
    mut commands: Commands,
    room: Res<CurrentRoom>,
    me: Option<Res<crate::rooms::Me>>,
    mut sounds: ResMut<crate::audio::SoundQueue>,
    mut heard: Local<Option<(u32, u8)>>,
    banner: Single<(Entity, &mut Node), With<Banner>>,
) {
    if !room.is_changed() {
        return;
    }
    let (banner, mut node) = banner.into_inner();
    let run = room.run();
    // A horn as a wave breaks (deeper for a boss's), a dirge as the run ends. Only on a change
    // heard in play: joining mid-run says nothing.
    let key = run.map(|run| (run.wave, match run.phase { Phase::Shop { .. } => 0, Phase::Fight => 1, Phase::Over { .. } => 2 }));
    let last = std::mem::replace(&mut *heard, key);
    if let (Some(run), Some((_, 1)), Some(last)) = (run, key, last)
        && key != Some(last)
    {
        match run.boss.is_some() {
            true => sounds.push(crate::audio::Sfx::BossHorn, 1.0),
            false => sounds.push(crate::audio::Sfx::Horn, 0.9),
        }
    }
    if let (Some((_, 2)), Some((_, phase))) = (key, last)
        && phase != 2
    {
        sounds.push(crate::audio::Sfx::Dirge, 0.9);
    }
    node.display = if run.is_some() { Display::Flex } else { Display::None };
    commands.entity(banner).despawn_children();
    let Some(run) = run else { return };
    let my_id = me.map_or(0, |me| me.guest_id);
    let party = room.0.as_ref().map_or(1, |view| view.members.len());
    commands.entity(banner).with_children(|banner| {
        match run.phase {
            Phase::Shop { secs_left } => {
                let next = if run.boss.is_some() { format!("WAVE {} - BOSS", run.wave) } else { format!("WAVE {}", run.wave) };
                banner.spawn(ui_text(format!("{next} IN {secs_left}s"), 20.0, palette::ui::TEXT));
                if let Some(omen) = run.omen {
                    banner.spawn(ui_text(format!("OMEN: {}  -  {}", omen.label().to_uppercase(), omen.blurb()), 12.0, palette::BLOOD));
                }
                let ready = run.ready.contains(&my_id);
                let label = if ready { format!("READY  {}/{}", run.ready.len(), party) } else { "READY (F)".to_string() };
                let fill = if ready { palette::ui::SELECTED } else { palette::ui::BACKDROP };
                banner.spawn((ReadyButton, HudButton, button(label, 12.0, fill, palette::ui::TEXT, ACCENT)));
            }
            Phase::Fight => {
                banner.spawn(ui_text(format!("WAVE {}", run.wave), 22.0, palette::ui::TEXT));
                if let Some(omen) = run.omen {
                    banner.spawn(ui_text(omen.label().to_uppercase(), 11.0, palette::BLOOD));
                }
                banner.spawn(Node { column_gap: px(GAP * 2.0), ..default() }).with_children(|row| {
                    let left = if run.enemies_left == 1 { "1 monster left".to_string() } else { format!("{} monsters left", run.enemies_left) };
                    row.spawn(ui_text(left, 12.0, palette::MONSTER));
                    if !run.waiting.is_empty() {
                        row.spawn(ui_text(format!("{} down", run.waiting.len()), 12.0, palette::ENEMY));
                    }
                });
            }
            Phase::Over { reached } => {
                banner.spawn(ui_text(format!("FELL ON WAVE {reached}"), 20.0, palette::ENEMY));
            }
        }
        if run.best > 0 {
            banner.spawn(ui_text(format!("Best: wave {}", run.best), 10.0, palette::ui::MUTED));
        }
    });
}

/// The boss's health, while one is about.
fn show_boss(
    bar: Single<(&BossBar, &mut Node)>,
    bosses: Query<(&ClassId, &Health, &MaxHealth)>,
    mut nodes: Query<&mut Node, Without<BossBar>>,
    mut texts: Query<&mut Text>,
) {
    let (bar, mut node) = bar.into_inner();
    let boss = bosses.iter().find(|(class, health, _)| health.alive() && class.def().is_boss());
    let display = if boss.is_some() { Display::Flex } else { Display::None };
    if node.display != display {
        node.display = display;
    }
    let Some((class, health, max)) = boss else { return };
    let name = class.def().name.to_uppercase();
    if let Ok(mut text) = texts.get_mut(bar.name)
        && text.0 != name
    {
        text.0 = name;
    }
    if let Ok(mut fill) = nodes.get_mut(bar.fill) {
        set_fill(&mut fill, health.0 as f32 / max.0.max(1) as f32);
    }
}

/// The way out of a room whose run (or match) is over, on its end's banner: out of the room, home.
#[derive(Component)]
pub(crate) struct LeaveOverButton;

/// The button out of a room whose run or match is over (`scoreboard.rs` puts one on a won
/// match's banner too).
pub(crate) fn leave_over_button() -> impl Bundle {
    let leave = (LeaveOverButton, HudButton, button("LEAVE", 14.0, ACCENT, palette::ui::PANEL, ACCENT));
    (Node { margin: UiRect::top(px(GAP * 1.5)), ..default() }, children![leave])
}

/// A click on `leave_over_button` leaves the room (home, where a score that made a leaderboard
/// is named).
fn leave_when_over(
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    buttons: Query<Ref<Interaction>, With<LeaveOverButton>>,
    mut leaving: ResMut<crate::rooms::Leaving>,
    mut sender: Single<&mut MessageSender<RoomRequest>, With<Client>>,
) {
    if buttons.iter().any(clicked) {
        mouse.reset(MouseButton::Left);
        crate::rooms::leave_room(&mut sender, &mut leaving);
    }
}

/// A lost run's end, across the middle, until each leaves the room.
fn show_over(mut commands: Commands, room: Res<CurrentRoom>, banner: Single<(Entity, &mut Node), With<OverBanner>>) {
    if !room.is_changed() {
        return;
    }
    let (banner, mut node) = banner.into_inner();
    let reached = room.run().and_then(|run| match run.phase {
        Phase::Over { reached } => Some(reached),
        _ => None,
    });
    node.display = if reached.is_some() { Display::Flex } else { Display::None };
    commands.entity(banner).despawn_children();
    if let Some(reached) = reached {
        commands.entity(banner).with_children(|banner| {
            banner.spawn(ui_text("THE PARTY HAS FALLEN", 34.0, palette::ENEMY));
            banner.spawn(ui_text(format!("You reached wave {reached}"), 16.0, palette::ui::TEXT));
            banner.spawn(leave_over_button());
        });
    }
}

/// F, or the banner's button, says we're ready for the next wave (or not, again).
fn ready_up(
    keys: Res<ButtonInput<KeyCode>>,
    room: Res<CurrentRoom>,
    me: Option<Res<crate::rooms::Me>>,
    typing: Option<Res<crate::browser::Typing>>,
    buttons: Query<Ref<Interaction>, With<ReadyButton>>,
    mut sender: Single<&mut MessageSender<RoomRequest>, With<Client>>,
) {
    let Some(run) = room.run().filter(|run| matches!(run.phase, Phase::Shop { .. })) else { return };
    let pressed = keys.just_pressed(KeyCode::KeyF) && !typing.is_some_and(|t| t.0);
    if pressed || buttons.iter().any(clicked) {
        let ready = me.is_some_and(|me| run.ready.contains(&me.guest_id));
        request(&mut sender, RoomRequest::Ready(!ready));
    }
}

/// An item's tile: its icon on its trigger's color, its level in pips over its corner, `size`
/// square.
fn item_tile(parent: &mut ChildSpawnerCommands, icons: &ItemIcons, item: Item, level: u8, size: f32) -> Entity {
    // The icon's pixels drawn whole: as big a multiple of them as fits.
    let pixel = ((size - 4.0) / ICON_PIXELS as f32).floor().max(1.0);
    parent
        .spawn((
            Node {
                width: px(size),
                height: px(size),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(px(if item.class().is_some() { 2.0 } else { 1.0 })),
                ..default()
            },
            BackgroundColor(trigger_color(item).with_alpha(0.3)),
            // A class's own item is rimmed in gold.
            BorderColor::all(if item.class().is_some() { palette::GOLD } else { trigger_color(item) }),
        ))
        .with_children(|tile| {
            let side = px(pixel * ICON_PIXELS as f32);
            tile.spawn((Node { width: side, height: side, ..default() }, ImageNode::new(icons.of(item)), Pickable::IGNORE));
            if level > 0 {
                tile.spawn((
                    Node { position_type: PositionType::Absolute, right: px(1.0), bottom: px(-2.0), ..default() },
                    ui_text("*".repeat(level as usize), (size * 0.26).max(8.0), palette::GOLD),
                    TextShadow { offset: Vec2::splat(1.0), color: palette::ui::PANEL },
                    Pickable::IGNORE,
                ));
            }
        })
        .id()
}

/// What sets an item off, as a color.
fn trigger_color(item: Item) -> Color {
    match item.trigger() {
        items::Trigger::OnHit => palette::ENEMY,
        items::Trigger::OnKill => palette::TORCH_FLAME,
        items::Trigger::OnCast => palette::SPIRIT,
        items::Trigger::WhenHit => palette::HEAL,
        items::Trigger::Always => Color::srgb_u8(0xB0, 0x80, 0xF0),
    }
}

/// A tooltip on what an item does, at `level` (and what it'd do raised, if `next`).
fn item_tip(parent: &mut EntityCommands, item: Item, level: u8, next: bool) {
    let mut tip_id = None;
    parent.with_children(|tile| {
        // In the shop (up at the right) it opens below, leftward; on our satchel, above.
        let place = if next { tip(Side::Below, -TIP_WIDTH + 30.0, TIP_WIDTH) } else { tip(Side::Above, -TIP_WIDTH / 2.0, TIP_WIDTH) };
        tip_id = Some(
            tile.spawn(place)
                .with_children(|tip| {
                    let shown_level = if next { (level + 1).min(MAX_LEVEL) } else { level.max(1) };
                    tip.spawn(tip_title(format!("{}  (level {shown_level})", item.name())));
                    let class = item.class().and_then(arena_shared::classes::ClassId::by_key).map(|c| c.def().name.clone());
                    let tag = match class {
                        Some(class) => format!("{}  -  {class} only", item.trigger().label()),
                        None => item.trigger().label().to_string(),
                    };
                    tip.spawn(tip_note(tag));
                    tip.spawn(tip_text(item.describe(shown_level)));
                    if next && level > 0 && level < MAX_LEVEL {
                        tip.spawn(tip_note(format!("Now: {}", item.describe(level))));
                    }
                    tip.spawn(tip_note(item.flavor()));
                })
                .id(),
        );
    });
    parent.insert(hover_shows(tip_id.expect("spawned")));
}

/// Our gold and items: rebuilt when they change, or the shop opens or shuts (selling shows then).
fn show_satchel(
    mut commands: Commands,
    room: Res<CurrentRoom>,
    open: Res<ShopOpen>,
    icons: Res<ItemIcons>,
    me: Query<Ref<Satchel>, With<Me>>,
    bar: Single<(Entity, &mut Node), With<SatchelBar>>,
) {
    let (bar, mut node) = bar.into_inner();
    let satchel = me.single().ok().filter(|_| room.run().is_some() || room.shopping());
    let display = if satchel.is_some() { Display::Flex } else { Display::None };
    if node.display != display {
        node.display = display;
    }
    let Some(satchel) = satchel else { return };
    if !satchel.is_changed() && !open.is_changed() {
        return;
    }
    commands.entity(bar).despawn_children();
    commands.entity(bar).with_children(|bar| {
        bar.spawn(Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center, margin: UiRect::right(px(6.0)), ..default() }).with_children(|gold| {
            gold.spawn(ui_text(satchel.gold.to_string(), 18.0, palette::GOLD));
            gold.spawn(ui_text("GOLD", 9.0, palette::ui::MUTED));
        });
        for slot in 0..SLOTS {
            match satchel.slots[slot] {
                Some(owned) => {
                    let tile = item_tile(bar, &icons, owned.item, owned.level, SLOT);
                    let mut tile_commands = bar.commands();
                    let mut tile_entity = tile_commands.entity(tile);
                    item_tip(&mut tile_entity, owned.item, owned.level, false);
                    if open.0 {
                        let back = owned.sell_value();
                        tile_entity.with_children(|tile| {
                            // Over the tile (under it is the action bar).
                            tile.spawn((SellButton(slot as u8), HudButton, button(format!("+{back}"), 9.0, palette::ui::BACKDROP, palette::GOLD, palette::GOLD.with_alpha(0.6))))
                                .entry::<Node>()
                                .and_modify(|mut node| {
                                    node.position_type = PositionType::Absolute;
                                    node.top = px(-22.0);
                                });
                        });
                    }
                }
                None => {
                    bar.spawn((
                        Node { width: px(SLOT), height: px(SLOT), border: UiRect::all(px(1.0)), ..default() },
                        BorderColor::all(palette::ui::MUTED.with_alpha(0.3)),
                        BackgroundColor(palette::ui::BACKDROP.with_alpha(0.5)),
                    ));
                }
            }
        }
    });
}

/// The shop opens as we come up to the merchant, in a run, and shuts as we walk off.
fn open_shop(
    room: Res<CurrentRoom>,
    mut open: ResMut<ShopOpen>,
    me: Query<&Pos, (With<Me>, With<PlayerId>)>,
    merchant: Query<&Pos, With<Shopkeeper>>,
) {
    let near = room.shopping()
        && me.single().ok().zip(merchant.iter().next()).is_some_and(|(me, merchant)| me.distance(merchant.0) <= SHOP_RADIUS);
    open.set_if_neq(ShopOpen(near));
}

/// The shop: every item our fighter can carry, what it costs us now and a way to buy it; rebuilt
/// when it opens, or our gold or items change.
fn show_shop(
    mut commands: Commands,
    room: Res<CurrentRoom>,
    open: Res<ShopOpen>,
    icons: Res<ItemIcons>,
    me: Query<(Ref<Satchel>, &ClassId), With<Me>>,
    panel: Single<(Entity, &mut Node), With<ShopPanel>>,
) {
    let (panel, mut node) = panel.into_inner();
    let display = if open.0 { Display::Flex } else { Display::None };
    if node.display != display {
        node.display = display;
    }
    let Ok((satchel, class)) = me.single() else { return };
    if !open.0 || !(open.is_changed() || satchel.is_changed()) {
        return;
    }
    commands.entity(panel).despawn_children();
    commands.entity(panel).with_children(|panel| {
        panel.spawn(Node { justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, ..default() }).with_children(|head| {
            head.spawn(ui_text("THE MERCHANT", 18.0, palette::ui::TEXT));
            head.spawn(ui_text(format!("{} gold", satchel.gold), 14.0, palette::GOLD));
        });
        let free = room.practice();
        let note = if free { "Practice: everything's free. Items do things, never stats." } else { "Items do things, never stats. Buy one again to raise it." };
        panel.spawn(ui_text(note, 10.0, palette::ui::MUTED));
        // Our class's own items first, set apart, then those for every class; each in two columns,
        // so the whole stock fits on a short screen.
        let (own, shared): (Vec<Item>, Vec<Item>) = Item::ALL.into_iter().filter(|item| item.fits(*class)).partition(|item| item.class().is_some());
        let sections = [(format!("{} ONLY", class.def().name.to_uppercase()), palette::GOLD, own), ("FOR EVERY CLASS".to_string(), palette::ui::MUTED, shared)];
        for (title, color, stock) in sections {
            if stock.is_empty() {
                continue;
            }
            panel.spawn((Node { margin: UiRect::top(px(4.0)), ..default() }, ui_text(title, 10.0, color)));
            let grid = Node { display: Display::Grid, grid_template_columns: RepeatedGridTrack::flex(2, 1.0), column_gap: px(GAP), row_gap: px(3.0), ..default() };
            panel.spawn(grid).with_children(|grid| {
                for item in stock {
                    let level = satchel.level(item);
                    let price = satchel.price_of(item, *class, free);
                    let own = item.class().is_some();
                    grid.spawn((
                        Node { column_gap: px(6.0), align_items: AlignItems::Center, padding: UiRect::all(px(3.0)), border: UiRect::left(px(if own { 2.0 } else { 0.0 })), ..default() },
                        BackgroundColor(if own { palette::GOLD.with_alpha(0.12) } else { palette::ui::BACKDROP.with_alpha(0.5) }),
                        BorderColor::all(palette::GOLD),
                    ))
                    .with_children(|row| {
                        let tile = item_tile(row, &icons, item, level, 32.0);
                        let mut commands = row.commands();
                        item_tip(&mut commands.entity(tile), item, level, true);
                        row.spawn(Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, flex_basis: px(0.0), ..default() }).with_children(|words| {
                            let name = if level > 0 { format!("{} {level}/{MAX_LEVEL}", item.name()) } else { item.name().to_string() };
                            words.spawn(ui_text(name, 11.0, palette::ui::TEXT));
                            words.spawn(ui_text(item.trigger().label(), 8.0, trigger_color(item)));
                        });
                        match price {
                            Ok(cost) => {
                                let affordable = satchel.gold >= cost;
                                let (fill, text) = if affordable { (palette::ui::SELECTED, palette::GOLD) } else { (palette::ui::BACKDROP, palette::ui::MUTED) };
                                let label = if free { "FREE".to_string() } else { cost.to_string() };
                                row.spawn((BuyButton(item), HudButton, button(label, 10.0, fill, text, palette::GOLD.with_alpha(0.5))));
                            }
                            Err(why) => {
                                let why = if level >= MAX_LEVEL { "MAX" } else if why.starts_with("No room") { "FULL" } else { "-" };
                                row.spawn(ui_text(why, 10.0, palette::ui::MUTED));
                            }
                        }
                    });
                }
            });
        }
        panel.spawn(ui_text("Hover an item to see what it does. Walk away to close.", 9.0, palette::ui::MUTED));
    });
}

/// The shop's buttons and our items' sell buttons ask the server.
fn trade(
    buys: Query<(Ref<Interaction>, &BuyButton)>,
    sells: Query<(Ref<Interaction>, &SellButton)>,
    mut sender: Single<&mut MessageSender<RoomRequest>, With<Client>>,
) {
    for (interaction, buy) in &buys {
        if clicked(interaction) {
            request(&mut sender, RoomRequest::Buy(buy.0));
        }
    }
    for (interaction, sell) in &sells {
        if clicked(interaction) {
            request(&mut sender, RoomRequest::Sell(sell.0));
        }
    }
}

// Names in the world.

/// A name floating over a fighter (`of`), this many meters up: the merchant's, an elite's twist.
#[derive(Component)]
struct NameOver {
    of: Entity,
    height: f32,
}

/// The merchant gets their name over their head, an elite its twist, in its color.
fn name_in_world(
    mut commands: Commands,
    new: Query<(Entity, &ClassId, Option<&Elite>, Has<Shopkeeper>), Or<(Added<Elite>, Added<Shopkeeper>)>>,
) {
    for (entity, class, elite, shopkeeper) in &new {
        let size = crate::rig::drawn_size(*class, elite.is_some());
        let (text, color, height) = match elite {
            _ if shopkeeper => ("MERCHANT".to_string(), palette::GOLD, 2.4 * size),
            Some(elite) => (elite.0.label().to_uppercase(), crate::render::affix_color(elite.0), 2.9 * size),
            None => continue,
        };
        commands.spawn((
            GameUi,
            NameOver { of: entity, height },
            Node { position_type: PositionType::Absolute, ..default() },
            ui_text(text, 11.0, color),
            Pickable::IGNORE,
            Visibility::Hidden,
        ));
    }
}

/// Names follow their fighters on screen, hidden while they're dead; gone with them.
fn place_names(
    mut commands: Commands,
    camera: Single<(&Camera, &GlobalTransform)>,
    fighters: Query<(&Pos, Option<&Health>)>,
    mut names: Query<(Entity, &NameOver, &mut Node, &mut Visibility, &ComputedNode)>,
) {
    let (camera, transform) = *camera;
    for (name, over, mut node, mut visibility, computed) in &mut names {
        let Ok((pos, health)) = fighters.get(over.of) else {
            commands.entity(name).despawn();
            continue;
        };
        let screen = camera.world_to_viewport(transform, to_world(pos.0, over.height)).ok().filter(|_| health.is_none_or(Health::alive));
        visibility.set_if_neq(shown(screen.is_some()));
        let Some(screen) = screen else { continue };
        let width = computed.size().x * computed.inverse_scale_factor();
        let (left, top) = (px(screen.x - width / 2.0), px(screen.y));
        if node.left != left || node.top != top {
            node.left = left;
            node.top = top;
        }
    }
}

// Effects.

/// A wisp's body (the effects themselves are `fx`'s).
#[derive(Resource)]
struct FxLooks {
    ball: Handle<Mesh>,
}

fn load_fx_looks(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(FxLooks { ball: meshes.add(Sphere::new(1.0).mesh().ico(1).expect("a small ico sphere")) });
}

/// How often a wisp leaves a mote of its light behind (seconds).
const WISP_WAKE_EVERY: f32 = 0.04;

/// Wisps circling a fighter that carries a Wisp Lantern, as many as its level gives, each
/// leaving a wake of light.
#[derive(Component)]
struct Wisps(Vec<Entity>);

/// A wisp, and whose: it goes when they do.
#[derive(Component)]
struct WispOf(Entity);

#[allow(clippy::too_many_arguments)]
fn circle_wisps(
    mut commands: Commands,
    clock: AttackClock,
    time: Res<Time>,
    art: Res<crate::fx::FxArt>,
    mut wake_at: Local<f32>,
    looks: Res<FxLooks>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    fighters: Query<(Entity, &Pos, &Satchel, Option<&Health>, Option<&Wisps>, Has<Me>)>,
    mut transforms: Query<&mut Transform, Without<Satchel>>,
    strays: Query<(Entity, &WispOf)>,
    mut material: Local<Option<Handle<StandardMaterial>>>,
) {
    for (wisp, of) in &strays {
        if !fighters.contains(of.0) {
            commands.entity(wisp).despawn();
        }
    }
    let glow = material.get_or_insert_with(|| materials.add(arena::glow(palette::WISP, 6.0))).clone();
    let wake = time.elapsed_secs() >= *wake_at;
    if wake {
        *wake_at = time.elapsed_secs() + WISP_WAKE_EVERY;
    }
    for (entity, pos, satchel, health, wisps, is_me) in &fighters {
        let level = satchel.level(Item::WispLantern);
        let (count, _, orbit, _) = items::wisp_lantern(level.max(1));
        let wanted = if level > 0 && health.is_none_or(Health::alive) { count as usize } else { 0 };
        let have = wisps.map_or(0, |w| w.0.len());
        if have != wanted {
            if let Some(wisps) = wisps {
                for wisp in &wisps.0 {
                    commands.entity(*wisp).despawn();
                }
            }
            let new: Vec<Entity> = (0..wanted)
                .map(|_| commands.spawn((WispOf(entity), Mesh3d(looks.ball.clone()), MeshMaterial3d(glow.clone()), Transform::from_scale(Vec3::splat(0.16)), NotShadowCaster)).id())
                .collect();
            commands.entity(entity).insert(Wisps(new));
            continue;
        }
        let Some(wisps) = wisps else { continue };
        let now = clock.now(is_me);
        for (n, wisp) in wisps.0.iter().enumerate() {
            if let Ok(mut transform) = transforms.get_mut(*wisp) {
                let at = pos.0 + items::wisp_offset(now, n as u32, count, orbit);
                let bob = 0.15 * (now * 0.3 + n as f32).sin();
                transform.translation = to_world(at, 1.1 + bob);
                if wake {
                    crate::fx::wisp_wake(&mut commands, &art, time.elapsed_secs(), transform.translation);
                }
            }
        }
    }
}
