//! The map pick, top middle of the character select: a small tile per map, named, the one we're
//! on lit. At home it picks the map practice and new
//! lobbies play on (`CurrentMap`, Sunken Keep to begin with), and the stage's view changes with it. In
//! a room it shows the room's map, as the room column does: its leader may switch it there or
//! here until the match starts; everyone else only sees it.

use arena_shared::map::MapId;
use arena_shared::protocol::RoomRequest;
use bevy::prelude::*;
use lightyear::prelude::client::*;
use lightyear::prelude::*;

use crate::arena::palette::ui::ACCENT;
use crate::arena::palette;
use crate::lobby::{GAP, MARGIN};
use crate::render::ui_text;
use crate::rooms::{CurrentMap, CurrentRoom, Me, Picking, Screen, request};

pub struct MapPickPlugin;

impl Plugin for MapPickPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(Picking), spawn_pick);
        app.add_systems(OnExit(Picking), despawn_pick);
        app.add_systems(Update, pick_map.run_if(in_state(Picking)));
    }
}

const TILE_WIDTH: f32 = 128.0;

/// The whole pick, despawned when the character select closes.
#[derive(Component)]
struct MapPick;

#[derive(Component)]
struct MapTile(MapId);

/// Its name, dimmed when it can't be picked.
#[derive(Component)]
struct TileName(MapId);

/// Under the tiles: who may switch it, when we can't.
#[derive(Component)]
struct PickHint;

fn spawn_pick(mut commands: Commands) {
    commands
        .spawn((
            MapPick,
            Node {
                position_type: PositionType::Absolute,
                top: px(MARGIN),
                left: px(0.0),
                right: px(0.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(GAP * 0.75),
                ..default()
            },
            Pickable::IGNORE,
            GlobalZIndex(10),
        ))
        .with_children(|pick| {
            pick.spawn(Node { column_gap: px(GAP), ..default() }).with_children(|row| {
                for map in MapId::ALL {
                    spawn_tile(row, map);
                }
            });
            pick.spawn((PickHint, ui_text("", 11.0, palette::ui::MUTED)));
        });
}

fn despawn_pick(mut commands: Commands, pick: Query<Entity, With<MapPick>>) {
    for pick in &pick {
        commands.entity(pick).despawn();
    }
}

/// A map's tile: its name.
fn spawn_tile(row: &mut ChildSpawnerCommands, map: MapId) {
    row.spawn((
        MapTile(map),
        Button,
        Node {
            width: px(TILE_WIDTH),
            justify_content: JustifyContent::Center,
            padding: UiRect::axes(px(GAP), px(GAP * 0.75)),
            border: UiRect::all(px(1.0)),
            ..default()
        },
        BorderColor::all(palette::ui::MUTED.with_alpha(0.3)),
        BackgroundColor(palette::ui::PANEL),
    ))
    .with_child((TileName(map), ui_text(map.label(), 12.0, palette::ui::TEXT)));
}

/// A click on a tile picks its map, where we may: at home our own pick; in a room, as its leader
/// before the start, the room's (the server tells us back, and `CurrentMap` follows). The tiles
/// show the map we're on, and whether they can be picked.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn pick_map(
    screen: Res<State<Screen>>,
    room: Res<CurrentRoom>,
    me: Option<Res<Me>>,
    mut current: ResMut<CurrentMap>,
    mut tiles: Query<(Ref<Interaction>, &MapTile, &mut BorderColor, &mut BackgroundColor)>,
    mut names: Query<(&TileName, &mut TextColor)>,
    mut hint: Single<&mut Text, With<PickHint>>,
    mut sender: Single<&mut MessageSender<RoomRequest>, With<Client>>,
) {
    let view = room.0.as_ref().filter(|_| *screen.get() == Screen::Room);
    let my_id = me.as_ref().map_or(0, |me| me.guest_id);
    let locked = view.filter(|view| !view.may_edit(my_id)).map(|view| if view.started { "Match in progress" } else { "Set by the leader" });
    let text = locked.unwrap_or_default().to_string();
    if hint.0 != text {
        hint.0 = text;
    }
    for (interaction, tile, mut border, mut fill) in &mut tiles {
        if locked.is_none() && interaction.is_changed() && *interaction == Interaction::Pressed && tile.0 != current.0 {
            match view {
                Some(_) => request(&mut sender, RoomRequest::SetMap(tile.0)),
                None => current.0 = tile.0,
            }
        }
        let on = tile.0 == current.0;
        let (edge, color) = match (on, locked.is_some(), *interaction) {
            (true, _, _) => (ACCENT, palette::ui::SELECTED),
            (false, true, _) => (palette::ui::MUTED.with_alpha(0.15), palette::ui::PANEL.with_alpha(0.6)),
            (false, false, Interaction::None) => (palette::ui::MUTED.with_alpha(0.3), palette::ui::PANEL),
            (false, false, _) => (palette::ui::MUTED.with_alpha(0.6), palette::ui::PANEL.lighter(0.08)),
        };
        border.set_if_neq(BorderColor::all(edge));
        fill.set_if_neq(BackgroundColor(color));
    }
    for (name, mut color) in &mut names {
        let dim = locked.is_some() && name.0 != current.0;
        color.set_if_neq(TextColor(if dim { palette::ui::MUTED } else { palette::ui::TEXT }));
    }
}
