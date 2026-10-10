//! F2: a panel about the fighter the camera is on (you, or whom the free camera watches, see
//! `camera`): health, what it's doing (walking, winding up, dashing...) and where.

use std::fmt::Write;

use arena_shared::protocol::*;
use bevy::prelude::*;
use lightyear::prelude::*;

use crate::camera::CameraMode;
use crate::feedback::AttackClock;
use crate::glade::palette;
use crate::render::shown;

pub struct InspectPlugin;

impl Plugin for InspectPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_panel);
        app.add_systems(Update, describe);
    }
}

#[derive(Component)]
struct InspectPanel;

fn spawn_panel(mut commands: Commands) {
    commands.spawn((
        InspectPanel,
        Text::new(""),
        TextFont { font_size: FontSize::Px(14.0), ..default() },
        TextColor(palette::HAZE),
        BackgroundColor(palette::INK.with_alpha(0.7)),
        Node {
            position_type: PositionType::Absolute,
            left: px(8.0),
            bottom: px(8.0),
            padding: UiRect::axes(px(10.0), px(6.0)),
            ..default()
        },
        Visibility::Hidden,
    ));
}

/// F2 shows or hides the panel; while shown, it says who the camera is on and what they're doing.
fn describe(
    keys: Res<ButtonInput<KeyCode>>,
    mut on: Local<bool>,
    mode: Res<CameraMode>,
    clock: AttackClock,
    fighters: Query<(Entity, &PlayerId, &ClassId, &Pos, Option<&Health>, Option<&Chilled>, &AttackState, &AbilityState, Has<Predicted>)>,
    panel: Single<(&mut Text, &mut Visibility), With<InspectPanel>>,
) {
    if keys.just_pressed(KeyCode::F2) {
        *on = !*on;
    }
    let (mut text, mut visibility) = panel.into_inner();
    visibility.set_if_neq(shown(*on));
    if !*on {
        return;
    }
    let target = match mode.watching() {
        Some(watched) => fighters.get(watched).ok(),
        None => fighters.iter().find(|(.., me)| *me),
    };
    let Some((_, id, class, pos, health, chilled, attack, ability, is_me)) = target else { return };
    let def = class.def();
    let now = clock.now(is_me);
    let chilled = chilled.copied().unwrap_or_default();
    let doing = if health.is_some_and(|h| !h.alive()) {
        "dead".to_string()
    } else if chilled.rooted.covers(now) {
        "frozen in place".to_string()
    } else if ability.dash.is_some() {
        format!("dashing ({})", def.ability.name)
    } else if let Some(windup) = attack.windup {
        format!("winding up {:.0}%", windup.progress(now, *class) * 100.0)
    } else {
        "ready".to_string()
    };
    let mut panel = String::new();
    let you = if is_me { " (you)" } else { "" };
    let hp = health.map_or("?".to_string(), |h| h.0.to_string());
    let _ = writeln!(panel, "{} {}{you}  {hp}/{} hp", def.name, id.0.to_bits(), def.max_hp);
    let slowed = if chilled.slowed.covers(now) { format!("  slowed {:.0}%", chilled.slow * 100.0) } else { String::new() };
    let _ = write!(panel, "{doing}{slowed}  at ({:.1}, {:.1})", pos.0.x, pos.0.y);
    if text.0 != panel {
        text.0 = panel;
    }
}
