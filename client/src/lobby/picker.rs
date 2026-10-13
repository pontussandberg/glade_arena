//! Picking a fighter: the row of tiles (clicked, or by number), and the pane telling what the
//! picked one is and does, folding open and shut.

use super::*;

/// A fighter to pick: its number, name and role.
pub(super) fn spawn_tile(tiles: &mut ChildSpawnerCommands, number: usize, class: ClassId) {
    let def = class.def();
    tiles
        .spawn((
            FighterTile(class),
            Button,
            Node {
                width: px(170.0),
                flex_direction: FlexDirection::Column,
                row_gap: px(2.0),
                padding: UiRect::all(px(12.0)),
                border: UiRect::all(px(1.0)),
                ..default()
            },
            BorderColor::all(Color::NONE),
            BackgroundColor(Color::NONE),
        ))
        .with_children(|tile| {
            tile.spawn(ui_text(number.to_string(), 11.0, palette::ui::MUTED));
            tile.spawn(ui_text(def.name.clone(), 20.0, palette::ui::TEXT));
            tile.spawn(ui_text(def.role.to_uppercase(), 11.0, ACCENT));
        });
}

pub(super) const NUMBER_KEYS: [KeyCode; 9] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
    KeyCode::Digit8,
    KeyCode::Digit9,
];

/// Picks a fighter with a click or its number key (unless typing), and marks the selected tile.
/// In a room, what's picked goes to the server at once.
pub(super) fn pick_fighter(
    keys: Res<ButtonInput<KeyCode>>,
    typing: Res<Typing>,
    mut lobby: ResMut<Lobby>,
    mut chosen: ResMut<ChosenClass>,
    mut tiles: Query<(&FighterTile, &Interaction, &mut BorderColor, &mut BackgroundColor)>,
) {
    if !typing.0
        && let Some((_, id)) = NUMBER_KEYS.iter().zip(ClassId::all()).find(|(key, _)| keys.just_pressed(**key))
    {
        lobby.selected = id;
    }
    for (tile, interaction, ..) in &tiles {
        if *interaction == Interaction::Pressed {
            lobby.selected = tile.0;
        }
    }
    if chosen.0 != Some(lobby.selected) {
        chosen.0 = Some(lobby.selected);
    }
    for (tile, interaction, mut border, mut background) in &mut tiles {
        let (edge, fill) = match (tile.0 == lobby.selected, interaction) {
            (true, _) => (ACCENT, palette::ui::SELECTED),
            (false, Interaction::Hovered) => (palette::ui::MUTED.with_alpha(0.6), palette::ui::PANEL),
            (false, _) => (palette::ui::MUTED.with_alpha(0.25), palette::ui::PANEL),
        };
        border.set_if_neq(BorderColor::all(edge));
        background.set_if_neq(BackgroundColor(fill));
    }
}

/// A click on the details pane's header opens or folds it.
pub(super) fn fold_details(header: Query<Ref<Interaction>, With<DetailsHeader>>, mut open: ResMut<DetailsOpen>) {
    if header.iter().any(clicked) {
        open.0 = !open.0;
    }
}

/// Puts the selected fighter on the stage, when it changes.
pub(super) fn show_fighter(mut commands: Commands, mut lobby: ResMut<Lobby>, visuals: Res<Visuals>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let selected = lobby.selected;
    if lobby.shown.is_some_and(|(_, shown)| shown == selected) {
        return;
    }
    if let Some((fighter, _)) = lobby.shown {
        commands.entity(fighter).despawn();
    }
    // Rigged and posed like any fighter (`rig::add_rigs` picks it up by its mesh), but not a
    // player: nothing else in the game sees it.
    let fighter = commands
        .spawn((
            LobbyPart,
            selected,
            Pos(STAGE),
            AttackState::default(),
            Cooldowns::default(),
            Dashing::default(),
            Mesh3d(visuals.fighter(selected)),
            MeshMaterial3d(materials.add(arena::matte(Color::WHITE))),
            Transform::from_translation(to_world(STAGE, 0.0)),
            ChildOf(lobby.lift),
        ))
        .id();
    lobby.shown = Some((fighter, selected));
}

/// The selected fighter's details on the left, rebuilt when it changes or the pane opens or
/// folds. As the lobbies pane: the header (role over name) always; open, cards under it, plainly
/// apart.
pub(super) fn show_details(
    mut commands: Commands,
    lobby: Res<Lobby>,
    open: Res<DetailsOpen>,
    details: Single<(Entity, Ref<Details>)>,
    mut built_for: Local<Option<ClassId>>,
) {
    let (details, fresh) = details.into_inner();
    // `Lobby` changes every frame (the camera's sway), so which fighter is shown is kept here.
    if *built_for == Some(lobby.selected) && !open.is_changed() && !fresh.is_added() {
        return;
    }
    *built_for = Some(lobby.selected);
    let def = lobby.selected.def();
    commands.entity(details).despawn_children().with_children(|column| {
        column.spawn((DetailsHeader, pane_header(open.0))).with_children(|header| {
            header.spawn(Node { flex_direction: FlexDirection::Column, row_gap: px(2.0), ..default() }).with_children(|left| {
                left.spawn(ui_text(def.role.to_uppercase(), 12.0, ACCENT));
                left.spawn(ui_text(def.name.clone(), 36.0, palette::ui::TEXT));
            });
            header.spawn(fold_chip(open.0));
        });
        if !open.0 {
            return;
        }
        stat_frame::spawn_frame(column, lobby.selected, stat_frame::FrameStyle::Card);
        column.spawn(card()).with_children(|card| {
            if let Some(passive) = stat_frame::passive_blurb(def) {
                power(card, "P", palette::TORCH_FLAME, passive);
                card.spawn(Node { height: px(GAP), ..default() });
            }
            for (i, (slot, ability)) in def.abilities().enumerate() {
                if i > 0 {
                    card.spawn(Node { height: px(GAP), ..default() });
                }
                power(card, crate::casting::key_label(slot), palette::SPIRIT, stat_frame::ability_blurb(def, slot, ability));
            }
        });
    });
}

/// A passive or the Q: a badge (`badge` in `color`), its name, its cooldown if it has one, then
/// what it does.
pub(super) fn power(column: &mut ChildSpawnerCommands, badge: &str, color: Color, blurb: stat_frame::Blurb) {
    column.spawn(Node { column_gap: px(GAP * 1.5), align_items: AlignItems::Center, ..default() }).with_children(|row| {
        row.spawn(key_chip(badge, 13.0, color, color));
        row.spawn(ui_text(blurb.name, 16.0, palette::ui::TEXT));
    });
    if let Some(cooldown) = blurb.cooldown {
        column.spawn(ui_text(cooldown, 11.0, palette::ui::MUTED));
    }
    column.spawn(ui_text(blurb.description, 13.0, palette::ui::MUTED));
}
