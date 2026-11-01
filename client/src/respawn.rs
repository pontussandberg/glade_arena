//! Dead and waiting to come back (`Respawning`): across the middle, how long until we're back,
//! and in practice a button to be back at once (`RoomRequest::RespawnNow`).

use arena_shared::protocol::{Respawning, RoomRequest};
use bevy::prelude::*;
use lightyear::prelude::client::*;
use lightyear::prelude::*;

use crate::Me;
use crate::arena::palette;
use crate::arena::palette::ui::ACCENT;
use crate::render::{GameUi, HudButton, button, clicked, ui_text};
use crate::rooms::{CurrentRoom, request};

pub struct RespawnPlugin;

impl Plugin for RespawnPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_panel);
        app.add_systems(Update, (show_panel, respawn_now).chain());
    }
}

/// The panel across the middle while we're dead.
#[derive(Component)]
struct Panel;

/// Its "Respawn now" button, in practice.
#[derive(Component)]
struct RespawnNowButton;

fn spawn_panel(mut commands: Commands) {
    commands
        .spawn((GameUi, Node { position_type: PositionType::Absolute, width: percent(100.0), top: percent(30.0), justify_content: JustifyContent::Center, ..default() }))
        .with_child((
            Panel,
            HudButton,
            Node {
                display: Display::None,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(8.0),
                padding: UiRect::axes(px(40.0), px(16.0)),
                ..default()
            },
            BackgroundColor(palette::ui::BACKDROP.with_alpha(0.8)),
        ));
}

/// Shows the panel while our fighter is `Respawning`, rebuilt only as the seconds tick down (or
/// we come and go from practice).
fn show_panel(
    mut commands: Commands,
    room: Res<CurrentRoom>,
    me: Query<&Respawning, With<Me>>,
    panel: Single<(Entity, &mut Node), With<Panel>>,
    mut shown: Local<Option<(Respawning, bool)>>,
) {
    let now = me.single().ok().map(|respawning| (*respawning, room.practice()));
    if now == *shown {
        return;
    }
    *shown = now;
    let (panel, mut node) = panel.into_inner();
    node.display = if now.is_some() { Display::Flex } else { Display::None };
    commands.entity(panel).despawn_children();
    let Some((respawning, practice)) = now else { return };
    commands.entity(panel).with_children(|panel| {
        panel.spawn(ui_text("YOU DIED", 30.0, palette::ENEMY));
        panel.spawn(ui_text(format!("Back in {}", respawning.secs), 16.0, palette::ui::TEXT));
        if practice {
            panel.spawn((RespawnNowButton, HudButton, button("RESPAWN NOW", 14.0, ACCENT, palette::ui::PANEL, ACCENT)));
        }
    });
}

/// A click on "Respawn now" asks the server to bring us back at once.
fn respawn_now(
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    buttons: Query<Ref<Interaction>, With<RespawnNowButton>>,
    mut sender: Single<&mut MessageSender<RoomRequest>, With<Client>>,
) {
    if buttons.iter().any(clicked) {
        mouse.reset(MouseButton::Left);
        request(&mut sender, RoomRequest::RespawnNow);
    }
}
