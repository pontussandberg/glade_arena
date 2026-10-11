//! Target dummies, in practice: "Place target dummies" in the ESC menu starts placing; each left
//! click then stands one where it's clicked (the server takes each class in turn), until a right
//! click or ESC stops it. A hint says so meanwhile. The clicks themselves are read in
//! `render::read_local_input`, so a placing click never also attacks.

use arena_shared::protocol::RoomRequest;
use bevy::prelude::*;
use lightyear::prelude::client::*;
use lightyear::prelude::*;

use crate::arena::palette;
use crate::render::{GameUi, ui_text};
use crate::rooms::{Screen, request};

pub struct DummiesPlugin;

impl Plugin for DummiesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Placing>();
        app.add_systems(Update, (place_dummies, show_hint).chain().run_if(in_state(Screen::InGame)));
        app.add_systems(OnExit(Screen::InGame), |mut placing: ResMut<Placing>| *placing = Placing::default());
    }
}

/// Whether we're placing target dummies, and where the latest click (not yet sent) asked for one.
#[derive(Resource, Default)]
pub(crate) struct Placing {
    pub(crate) on: bool,
    pub(crate) at: Option<Vec2>,
}

/// Says where to stand each clicked dummy.
fn place_dummies(mut placing: ResMut<Placing>, mut sender: Single<&mut MessageSender<RoomRequest>, With<Client>>) {
    if let Some(at) = placing.at.take() {
        request(&mut sender, RoomRequest::PlaceDummy(at));
    }
}

/// The hint at the top of the screen while placing.
#[derive(Component)]
struct Hint;

fn show_hint(mut commands: Commands, placing: Res<Placing>, hint: Query<Entity, With<Hint>>) {
    if !placing.is_changed() {
        return;
    }
    match (placing.on, hint.single().ok()) {
        (true, None) => {
            commands.spawn((
                Hint,
                GameUi,
                Node {
                    position_type: PositionType::Absolute,
                    top: px(56.0),
                    width: percent(100.0),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                Pickable::IGNORE,
                children![ui_text("Click to place a target dummy  ·  right-click or ESC to stop", 14.0, palette::ui::SPROUT)],
            ));
        }
        (false, Some(hint)) => commands.entity(hint).despawn(),
        _ => {}
    }
}
