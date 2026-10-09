//! A simple sparring bot: drives `DesiredInput` instead of the mouse. Melee classes chase the
//! nearest enemy and swing; ranged classes keep their distance and shoot. Turn it on with
//! `ARENA_BOT=1` (native client) to have someone to fight when testing alone.

use arena_shared::classes::{AbilityKind, AttackKind};
use arena_shared::map::{Map, map};
use arena_shared::protocol::*;
use bevy::prelude::*;
use lightyear::prelude::client::input::InputSystems;
use lightyear::prelude::*;

use crate::{DesiredInput, PlayerControls};

/// How far inside its reach a melee bot starts a swing.
const MELEE_LEAD: f32 = 0.6;

/// Present when this client is played by the bot.
#[derive(Resource)]
pub struct Bot;

pub struct BotPlugin;

impl Plugin for BotPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Bot);
        app.configure_sets(Update, PlayerControls.run_if(|| false));
        // Decide once per input tick, right before inputs are written.
        app.add_systems(FixedPreUpdate, think.before(InputSystems::WriteClientInputs));
    }
}

fn think(
    me: Query<(&PlayerId, &ClassId, &Pos), With<Predicted>>,
    others: Query<(&PlayerId, &Pos, &Health), Without<Predicted>>,
    mut desired: ResMut<DesiredInput>,
) {
    let Ok((my_id, class, me)) = me.single() else { return };
    let Some(target) = others
        .iter()
        .filter(|(id, _, health)| id.0 != my_id.0 && health.alive())
        .map(|(_, p, _)| p.0)
        .min_by(|a, b| a.distance(me.0).total_cmp(&b.distance(me.0)))
    else {
        desired.0 = PlayerInput::default();
        return;
    };
    let to_target = target - me.0;
    let distance = to_target.length();
    // Where we want to stand relative to the target, and when to attack. Melee closes right in
    // and swings well inside reach, since its windup gives the target time to step away; ranged
    // keeps some distance and shoots from as far as it reaches.
    let kind = &class.def().attack.kind;
    let reach = kind.reach();
    let (preferred, attack_within) = match *kind {
        AttackKind::Melee { .. } => (0.0, reach - MELEE_LEAD),
        AttackKind::Projectile { .. } => (reach * 0.6, reach),
    };
    let stand_at = target - to_target.normalize_or_zero() * preferred;
    let wanted = ((distance - preferred).abs() > 1.0)
        .then(|| map().nearest_walkable(Map::tile_of(stand_at), 3))
        .flatten();
    // Keep the current plan unless the goal moved a couple of tiles: fewer fresh path searches.
    let move_to = match (desired.0.move_to, wanted) {
        (Some(current), Some(new)) if (new - current).abs().max_element() < 2 => Some(current),
        _ => wanted,
    };
    let clear = map().shot_clear(me.0, target);
    let fire = distance <= attack_within && clear;
    // Q whenever it would land: a dash to close in from just out of reach, a throw in range, a
    // nova when the target is right on top of us.
    let ability = clear
        && match class.def().ability.kind {
            AbilityKind::Dash { distance: dash, .. } => distance > attack_within && distance < dash * 0.9,
            AbilityKind::Projectile { range, .. } => distance < range * 0.9,
            AbilityKind::Nova { radius, .. } => distance < radius * 0.8,
        };
    desired.0 = PlayerInput { move_to, aim: to_target, fire, ability, ..default() };
}
