//! The waves mode, on screen: the run's banner at the top (the wave, the shop's countdown and the
//! way to say we're ready, the monsters left, the party's lives), a boss's health under it, the
//! end of a lost run across the middle; our gold and items over the action bar; the merchant's
//! shop, open while we stand by them; and what items and monsters do (`Fx`), and wisps circling
//! whoever carries a Wisp Lantern.

use arena_shared::items::{self, Item, MAX_LEVEL, SLOTS, Satchel};
use arena_shared::protocol::*;
use arena_shared::rooms::{Phase, RunView};
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use lightyear::prelude::client::*;
use lightyear::prelude::*;

use crate::Me;
use crate::arena::palette::ui::ACCENT;
use crate::arena::{self, palette, to_world};
use crate::feedback::AttackClock;
use crate::render::{GameUi, HudButton, button, clicked, set_fill, shown, ui_text};
use crate::rooms::{CurrentRoom, request};
use crate::tooltip::{Side, hover_shows, tip, tip_note, tip_text, tip_title};

pub struct WavesPlugin;

impl Plugin for WavesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (spawn_hud, load_fx_looks));
        app.add_systems(
            Update,
            (
                (show_banner, show_boss, show_over, ready_up).chain(),
                (show_satchel, open_shop, show_shop, trade).chain(),
                (add_fx, play_fx, circle_wisps),
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

/// The shop's button for a life for the party.
#[derive(Component)]
struct LifeButton;

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

/// The run we're in, while it's on (or just lost).
fn run_of(room: &CurrentRoom) -> Option<&RunView> {
    room.0.as_ref().filter(|view| view.started).and_then(|view| view.run.as_ref())
}

/// The banner: rebuilt whenever the room changes.
fn show_banner(mut commands: Commands, room: Res<CurrentRoom>, me: Option<Res<crate::rooms::Me>>, banner: Single<(Entity, &mut Node), With<Banner>>) {
    if !room.is_changed() {
        return;
    }
    let (banner, mut node) = banner.into_inner();
    let run = run_of(&room);
    node.display = if run.is_some() { Display::Flex } else { Display::None };
    commands.entity(banner).despawn_children();
    let Some(run) = run else { return };
    let my_id = me.map_or(0, |me| me.guest_id);
    let party = room.0.as_ref().map_or(1, |view| view.members.len());
    commands.entity(banner).with_children(|banner| {
        let lives = format!("Lives {}", run.lives);
        match run.phase {
            Phase::Shop { secs_left } => {
                let next = if run.boss.is_some() { format!("WAVE {} - BOSS", run.wave) } else { format!("WAVE {}", run.wave) };
                banner.spawn(ui_text(format!("{next} IN {secs_left}s"), 20.0, palette::ui::TEXT));
                if let Some(omen) = run.omen {
                    banner.spawn(ui_text(format!("OMEN: {}  -  {}", omen.label().to_uppercase(), omen.blurb()), 12.0, palette::BLOOD));
                }
                banner.spawn(Node { column_gap: px(GAP * 2.0), align_items: AlignItems::Center, ..default() }).with_children(|row| {
                    row.spawn(ui_text("Shop at the merchant in the camp; its fire mends you", 12.0, palette::ui::MUTED));
                    row.spawn(ui_text(lives, 12.0, palette::HEAL));
                });
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
                    row.spawn(ui_text(lives, 12.0, palette::HEAL));
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
    let boss = bosses.iter().find(|(class, health, _)| health.alive() && class.def().mob.as_ref().is_some_and(|m| m.boss));
    let display = if boss.is_some() { Display::Flex } else { Display::None };
    if node.display != display {
        node.display = display;
    }
    let Some((class, health, max)) = boss else { return };
    if let Ok(mut text) = texts.get_mut(bar.name)
        && text.0 != class.def().name
    {
        text.0 = class.def().name.to_uppercase();
    }
    if let Ok(mut fill) = nodes.get_mut(bar.fill) {
        set_fill(&mut fill, health.0 as f32 / max.0.max(1) as f32);
    }
}

/// A lost run's end, across the middle, until everyone's back in the lobby.
fn show_over(mut commands: Commands, room: Res<CurrentRoom>, banner: Single<(Entity, &mut Node), With<OverBanner>>) {
    if !room.is_changed() {
        return;
    }
    let (banner, mut node) = banner.into_inner();
    let reached = run_of(&room).and_then(|run| match run.phase {
        Phase::Over { reached } => Some(reached),
        _ => None,
    });
    node.display = if reached.is_some() { Display::Flex } else { Display::None };
    commands.entity(banner).despawn_children();
    if let Some(reached) = reached {
        commands.entity(banner).with_children(|banner| {
            banner.spawn(ui_text("THE PARTY HAS FALLEN", 34.0, palette::ENEMY));
            banner.spawn(ui_text(format!("You reached wave {reached}"), 16.0, palette::ui::TEXT));
            banner.spawn(ui_text("Back to the lobby in a moment", 12.0, palette::ui::MUTED));
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
    let Some(run) = run_of(&room).filter(|run| matches!(run.phase, Phase::Shop { .. })) else { return };
    let pressed = keys.just_pressed(KeyCode::KeyF) && !typing.is_some_and(|t| t.0);
    if pressed || buttons.iter().any(clicked) {
        let ready = me.is_some_and(|me| run.ready.contains(&me.guest_id));
        request(&mut sender, RoomRequest::Ready(!ready));
    }
}

/// An item's tile: its initials on its trigger's color, its level in pips, `size` square.
fn item_tile(parent: &mut ChildSpawnerCommands, item: Item, level: u8, size: f32) -> Entity {
    let initials: String = item.name().split(' ').filter_map(|w| w.chars().next()).take(2).collect();
    parent
        .spawn((
            Node {
                width: px(size),
                height: px(size),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(px(1.0)),
                ..default()
            },
            BackgroundColor(trigger_color(item).with_alpha(0.35)),
            BorderColor::all(trigger_color(item)),
        ))
        .with_children(|tile| {
            tile.spawn(ui_text(initials, size * 0.36, palette::ui::TEXT));
            if level > 0 {
                tile.spawn(ui_text("*".repeat(level as usize), size * 0.26, palette::GOLD));
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
fn item_tip(parent: &mut EntityCommands, item: Item, level: u8, next: bool) -> Entity {
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
    let tip_id = tip_id.expect("spawned");
    parent.insert(hover_shows(tip_id));
    tip_id
}

/// Our gold and items: rebuilt when they change, or the shop opens or shuts (selling shows then).
fn show_satchel(
    mut commands: Commands,
    room: Res<CurrentRoom>,
    open: Res<ShopOpen>,
    me: Query<Ref<Satchel>, With<Me>>,
    bar: Single<(Entity, &mut Node), With<SatchelBar>>,
) {
    let (bar, mut node) = bar.into_inner();
    let satchel = me.single().ok().filter(|_| run_of(&room).is_some());
    let display = if satchel.is_some() { Display::Flex } else { Display::None };
    if node.display != display {
        node.display = display;
    }
    let Some(satchel) = satchel else { return };
    if !satchel.is_changed() && !open.is_changed() && !room.is_changed() {
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
                    let tile = item_tile(bar, owned.item, owned.level, SLOT);
                    let mut tile_commands = bar.commands();
                    let mut tile_entity = tile_commands.entity(tile);
                    item_tip(&mut tile_entity, owned.item, owned.level, false);
                    if open.0 {
                        let back = Satchel::sell_value(&owned);
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
    let near = run_of(&room).is_some_and(|run| !matches!(run.phase, Phase::Over { .. }))
        && me.single().ok().zip(merchant.iter().next()).is_some_and(|(me, merchant)| me.distance(merchant.0) <= SHOP_RADIUS);
    open.set_if_neq(ShopOpen(near));
}

/// The shop: every item our fighter can carry, what it costs us now and a way to buy it; rebuilt
/// when it opens, or our gold or items change.
fn show_shop(
    mut commands: Commands,
    room: Res<CurrentRoom>,
    open: Res<ShopOpen>,
    me: Query<(Ref<Satchel>, &ClassId), With<Me>>,
    panel: Single<(Entity, &mut Node), With<ShopPanel>>,
) {
    let (panel, mut node) = panel.into_inner();
    let display = if open.0 { Display::Flex } else { Display::None };
    if node.display != display {
        node.display = display;
    }
    let Ok((satchel, class)) = me.single() else { return };
    if !open.0 || !(open.is_changed() || satchel.is_changed() || room.is_changed()) {
        return;
    }
    let life_price = run_of(&room).map(|run| run.life_price);
    commands.entity(panel).despawn_children();
    commands.entity(panel).with_children(|panel| {
        panel.spawn(Node { justify_content: JustifyContent::SpaceBetween, align_items: AlignItems::Center, ..default() }).with_children(|head| {
            head.spawn(ui_text("THE MERCHANT", 18.0, palette::ui::TEXT));
            head.spawn(ui_text(format!("{} gold", satchel.gold), 14.0, palette::GOLD));
        });
        panel.spawn(ui_text("Items do things, never stats. Buy one again to raise it.", 10.0, palette::ui::MUTED));
        // Two columns of items, so the whole stock fits on a short screen.
        let grid = Node { display: Display::Grid, grid_template_columns: RepeatedGridTrack::flex(2, 1.0), column_gap: px(GAP), row_gap: px(3.0), ..default() };
        panel.spawn(grid).with_children(|grid| {
            for item in Item::ALL.into_iter().filter(|item| item.fits(*class)) {
                let level = satchel.level(item);
                let price = satchel.price_of(item, *class);
                grid.spawn((
                    Node { column_gap: px(6.0), align_items: AlignItems::Center, padding: UiRect::all(px(3.0)), ..default() },
                    BackgroundColor(palette::ui::BACKDROP.with_alpha(0.5)),
                ))
                .with_children(|row| {
                    let tile = item_tile(row, item, level, 28.0);
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
                            row.spawn((BuyButton(item), HudButton, button(cost.to_string(), 10.0, fill, text, palette::GOLD.with_alpha(0.5))));
                        }
                        Err(why) => {
                            let why = if level >= MAX_LEVEL { "MAX" } else if why.starts_with("No room") { "FULL" } else { "-" };
                            row.spawn(ui_text(why, 10.0, palette::ui::MUTED));
                        }
                    }
                });
            }
        });
        // A life for the party, for whoever has gold to spare.
        if let Some(price) = life_price {
            panel.spawn((Node { column_gap: px(GAP), align_items: AlignItems::Center, padding: UiRect::all(px(3.0)), ..default() }, BackgroundColor(palette::ui::BACKDROP.with_alpha(0.5)))).with_children(|row| {
                row.spawn(Node { flex_direction: FlexDirection::Column, flex_grow: 1.0, ..default() }).with_children(|words| {
                    words.spawn(ui_text("Spirit Candle", 12.0, palette::HEAL));
                    words.spawn(ui_text("A life more for the whole party. Dearer each time.", 9.0, palette::ui::MUTED));
                });
                let (fill, text) = if satchel.gold >= price { (palette::ui::SELECTED, palette::GOLD) } else { (palette::ui::BACKDROP, palette::ui::MUTED) };
                row.spawn((LifeButton, HudButton, button(price.to_string(), 10.0, fill, text, palette::GOLD.with_alpha(0.5))));
            });
        }
        panel.spawn(ui_text("Hover an item to see what it does. Walk away to close.", 9.0, palette::ui::MUTED));
    });
}

/// The shop's buttons and our items' sell buttons ask the server.
fn trade(
    life: Query<Ref<Interaction>, With<LifeButton>>,
    buys: Query<(Ref<Interaction>, &BuyButton)>,
    sells: Query<(Ref<Interaction>, &SellButton)>,
    mut sender: Single<&mut MessageSender<RoomRequest>, With<Client>>,
) {
    if life.iter().any(clicked) {
        request(&mut sender, RoomRequest::BuyLife);
    }
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
        let size = class.def().size() * if elite.is_some() { crate::rig::ELITE_SIZE } else { 1.0 };
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

/// The meshes effects are made of.
#[derive(Resource)]
struct FxLooks {
    disc: Handle<Mesh>,
    ring: Handle<Mesh>,
    bolt: Handle<Mesh>,
    ball: Handle<Mesh>,
    bit: Handle<Mesh>,
}

fn load_fx_looks(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(FxLooks {
        disc: meshes.add(Circle::new(1.0).mesh().resolution(32).build()),
        ring: meshes.add(Annulus::new(0.88, 1.0).mesh().resolution(40).build()),
        bolt: meshes.add(Cuboid::new(1.0, 0.07, 0.07)),
        ball: meshes.add(Sphere::new(1.0).mesh().ico(1).expect("a small ico sphere")),
        bit: meshes.add(Cuboid::new(0.14, 0.14, 0.14)),
    });
}

pub(crate) fn hue_color(hue: FxColor) -> Color {
    match hue {
        FxColor::Fire => palette::TORCH_FLAME,
        FxColor::Spirit => palette::SPIRIT,
        FxColor::Frost => palette::ICE,
        FxColor::Gold => palette::GOLD,
        FxColor::Blood => palette::BLOOD,
        FxColor::Poison => Color::srgb_u8(0x9C, 0xE0, 0x50),
        FxColor::Void => Color::srgb_u8(0xA0, 0x60, 0xFF),
        FxColor::Storm => Color::srgb_u8(0xB4, 0xDC, 0xFF),
        FxColor::Thorn => Color::srgb_u8(0x8C, 0xC0, 0x5A),
    }
}

/// An effect's parts (its children), each with its own material, to fade.
#[derive(Component)]
struct FxParts(Vec<(Entity, Handle<StandardMaterial>, f32)>);

/// Gives each effect its look as it arrives.
fn add_fx(mut commands: Commands, looks: Res<FxLooks>, mut materials: ResMut<Assets<StandardMaterial>>, new: Query<(Entity, &Fx), Added<Fx>>) {
    let flat = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    for (entity, fx) in &new {
        let mut parts = Vec::new();
        let mut part = |commands: &mut Commands, mesh: &Handle<Mesh>, color: Color, alpha: f32, glow: f32, transform: Transform| {
            let material = materials.add(arena::translucent(color, alpha, glow));
            let id = commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), transform, NotShadowCaster)).id();
            parts.push((id, material, alpha));
            id
        };
        let ground = |height: f32, radius: f32| Transform::from_xyz(0.0, height, 0.0).with_rotation(flat).with_scale(Vec3::new(radius, radius, 1.0));
        let children: Vec<Entity> = match fx.kind {
            FxKind::Blast(hue) | FxKind::Zone(hue) | FxKind::Warn(hue) => {
                let c = hue_color(hue);
                vec![part(&mut commands, &looks.disc, c, 0.28, 1.5, ground(0.06, fx.radius)), part(&mut commands, &looks.ring, c, 0.8, 3.0, ground(0.07, fx.radius))]
            }
            FxKind::Arc => {
                // Three jagged pieces from one end to the other, high over the ground.
                let c = hue_color(FxColor::Storm);
                let (a, b) = (to_world(fx.at, 1.0), to_world(fx.to, 1.0));
                let side = (b - a).cross(Vec3::Y).normalize_or_zero() * 0.3;
                let points = [a, a.lerp(b, 0.33) + side, a.lerp(b, 0.66) - side, b];
                points
                    .windows(2)
                    .map(|w| {
                        let mid = (w[0] + w[1]) / 2.0;
                        let along = w[1] - w[0];
                        let transform = Transform::from_translation(mid).looking_to(along.normalize_or(Vec3::X), Vec3::Y) * Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)).with_scale(Vec3::new(along.length(), 1.0, 1.0));
                        let id = part(&mut commands, &looks.bolt, c, 0.95, 8.0, transform);
                        id
                    })
                    .collect()
            }
            FxKind::Lob => vec![part(&mut commands, &looks.ball, palette::BARK.darker(0.1), 1.0, 0.2, Transform::from_scale(Vec3::splat(0.22)))],
            FxKind::Bolt => {
                let c = hue_color(FxColor::Storm);
                vec![
                    part(&mut commands, &looks.ring, c, 0.8, 3.0, ground(0.07, fx.radius)),
                    part(&mut commands, &looks.bolt, c, 0.95, 9.0, Transform::from_xyz(0.0, 4.0, 0.0).with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)).with_scale(Vec3::new(8.0, 3.0, 3.0))),
                ]
            }
            FxKind::Coins => (0..8).map(|_| part(&mut commands, &looks.bit, palette::GOLD, 1.0, 5.0, Transform::default())).collect(),
            FxKind::Rise => {
                let c = palette::ROT_DARK;
                vec![part(&mut commands, &looks.disc, c, 0.6, 0.2, ground(0.05, fx.radius)), part(&mut commands, &looks.ring, palette::MONSTER, 0.6, 2.0, ground(0.06, fx.radius))]
            }
        };
        let root = if fx.kind == FxKind::Arc || fx.kind == FxKind::Lob { Vec3::ZERO } else { to_world(fx.at, 0.0) };
        commands.entity(entity).insert((Transform::from_translation(root), Visibility::Inherited, FxParts(parts))).add_children(&children);
    }
}

/// Effects play out on the timeline others' fighters are drawn on: blasts spread and fade, zones
/// breathe and fade at their end, warnings fill until they land, kegs fly, bolts flash, coins
/// fly up.
fn play_fx(clock: AttackClock, fx: Query<(&Fx, &FxParts)>, mut transforms: Query<&mut Transform>, mut materials: ResMut<Assets<StandardMaterial>>, mut visibility: Query<&mut Visibility>) {
    let now = clock.now(false);
    for (fx, parts) in &fx {
        let span = (fx.until as f32 - fx.from as f32).max(1.0);
        let t = ((now - fx.from as f32) / span).clamp(0.0, 1.0);
        let over = now > fx.until as f32;
        let mut fade = |i: usize, strength: f32| {
            if let Some((_, material, alpha)) = parts.0.get(i)
                && let Some(mut m) = materials.get_mut(material)
            {
                m.base_color.set_alpha(alpha * strength.clamp(0.0, 1.0));
            }
        };
        match fx.kind {
            FxKind::Blast(_) => {
                let out = 1.0 - (1.0 - t) * (1.0 - t);
                for (i, (part, ..)) in parts.0.iter().enumerate() {
                    if let Ok(mut transform) = transforms.get_mut(*part) {
                        let r = fx.radius * (0.3 + 0.7 * out);
                        transform.scale = Vec3::new(r, r, 1.0);
                    }
                    fade(i, 1.0 - t);
                }
            }
            FxKind::Zone(_) => {
                let end = ((fx.until as f32 - now) / 8.0).clamp(0.0, 1.0);
                let breath = 0.75 + 0.25 * (now * 0.4).sin();
                fade(0, breath * end);
                fade(1, end);
            }
            FxKind::Warn(_) => {
                if let Some((part, ..)) = parts.0.first()
                    && let Ok(mut transform) = transforms.get_mut(*part)
                {
                    let r = fx.radius * t;
                    transform.scale = Vec3::new(r, r, 1.0);
                }
                fade(0, if over { 0.0 } else { 1.0 });
                fade(1, if over { 0.0 } else { 0.5 + 0.5 * t });
            }
            FxKind::Arc => {
                for i in 0..parts.0.len() {
                    fade(i, 1.0 - t);
                }
            }
            FxKind::Lob => {
                if let Some((part, ..)) = parts.0.first()
                    && let Ok(mut transform) = transforms.get_mut(*part)
                {
                    let ground = fx.at.lerp(fx.to, t);
                    transform.translation = to_world(ground, 0.4 + 3.0 * t * (1.0 - t) * 4.0 * 0.5);
                }
                fade(0, if over { 0.0 } else { 1.0 });
            }
            FxKind::Bolt => {
                // The ring warns; the bolt strikes at the end, then fades.
                fade(0, if over { 0.0 } else { 0.4 + 0.6 * t });
                let struck = ((now - fx.until as f32) / 6.0).clamp(0.0, 1.0);
                let lit = now >= fx.until as f32;
                if let Some((bolt, ..)) = parts.0.get(1)
                    && let Ok(mut v) = visibility.get_mut(*bolt)
                {
                    v.set_if_neq(shown(lit && struck < 1.0));
                }
                fade(1, 1.0 - struck);
            }
            FxKind::Coins => {
                for (i, (part, ..)) in parts.0.iter().enumerate() {
                    if let Ok(mut transform) = transforms.get_mut(*part) {
                        let angle = i as f32 * std::f32::consts::TAU / parts.0.len() as f32 + t * 2.0;
                        let out = fx.radius * t;
                        transform.translation = Vec3::new(angle.cos() * out, 0.4 + 2.2 * t - 1.6 * t * t, angle.sin() * out);
                        transform.rotation = Quat::from_rotation_y(angle * 3.0);
                    }
                    fade(i, 1.0 - t * t);
                }
            }
            FxKind::Rise => {
                fade(0, 1.0 - t);
                fade(1, 1.0 - t);
            }
        }
    }
}

/// Wisps circling a fighter that carries a Wisp Lantern, as many as its level gives.
#[derive(Component)]
struct Wisps(Vec<Entity>);

/// A wisp, and whose: it goes when they do.
#[derive(Component)]
struct WispOf(Entity);

fn circle_wisps(
    mut commands: Commands,
    clock: AttackClock,
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
            }
        }
    }
}
