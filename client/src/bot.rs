//! A simple sparring bot: drives `DesiredInput` instead of the mouse. Melee classes chase the
//! nearest enemy and swing; ranged classes keep their distance and shoot. It clicks to attack
//! whenever it's in range (the server drops clicks until its last attack is done), and stands
//! still while one winds up, since moving would cancel it. Turn it on with
//! `ARENA_BOT=1` (native client) to have someone to fight when testing alone.

use arena_shared::abilities::AbilityKind;
use arena_shared::protocol::*;
use bevy::prelude::*;
use lightyear::prelude::client::RemoteTimeline;
use lightyear::prelude::*;

use crate::rooms::CurrentMap;
use crate::{DesiredInput, Me, PlayerControls, SendCommands};

/// How far inside its lane's ends (`AttackDef::hit_span`) a melee bot starts a swing.
const MELEE_LEAD: f32 = 0.6;

/// Present when this client is played by the bot.
#[derive(Resource)]
pub struct Bot;

pub struct BotPlugin;

impl Plugin for BotPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Bot);
        app.configure_sets(Update, PlayerControls.run_if(|| false));
        // Decide every frame, right before what changed is sent.
        app.add_systems(Update, think.before(SendCommands));
    }
}

fn think(
    me: Query<(&PlayerId, &ClassId, &Pos, &AttackState, &Cooldowns), With<Me>>,
    server: Query<&RemoteTimeline, With<Client>>,
    others: Query<(&PlayerId, &Pos, &Health), Without<Me>>,
    mut desired: ResMut<DesiredInput>,
    map: Res<CurrentMap>,
) {
    let map = map.map();
    let Ok((my_id, class, me, attack, cooldowns)) = me.single() else { return };
    // The server tick our news is from: an ability whose cooldowns were over by then is ready.
    let latest = server.single().ok().and_then(|timeline| timeline.last_received_tick()).map_or(0, |tick| tick.0);
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
    // and swings inside reach, since its windup gives the target time to step away; ranged
    // keeps some distance and shoots from as far as it reaches.
    // A melee lane that starts out ahead (`hit_span`) can't hit a target closer than that: the
    // bot backs off to swing rather than swinging at nothing.
    let attack_def = &class.def().attack;
    let (preferred, (attack_from, attack_within)) = match attack_def.hit_span(false) {
        // Stands where the target is in the middle of its lane.
        Some((nearest, farthest)) => {
            let lead = |edge: f32| if edge > 0.0 { edge + MELEE_LEAD } else { edge };
            ((nearest + farthest) / 2.0, (lead(nearest), farthest - MELEE_LEAD))
        }
        None => (attack_def.kind.reach() * 0.6, (0.0, attack_def.kind.reach())),
    };
    let in_reach = (attack_from..=attack_within).contains(&distance);
    let stand_at = target - to_target.normalize_or_zero() * preferred;
    let wanted = (!in_reach || (distance - preferred).abs() > 1.0)
        .then(|| map.walk_target(stand_at, 3))
        .flatten();
    // Keep the current plan unless the goal moved a couple of tiles: fewer fresh path searches.
    let move_to = match (desired.0.move_to, wanted) {
        (Some(current), Some(new)) if (new - current).abs().max_element() < 2.0 => Some(current),
        _ => wanted,
    };
    let clear = map.shot_clear(me.0, target);
    let fire = in_reach && clear;
    let move_to = move_to.filter(|_| !fire && attack.winding_up().is_none());
    // The first ready ability that would land: a dash to close in from just out of reach, a
    // throw in range, a nova when the target is right on top of us.
    let cast = class
        .def()
        .abilities()
        .find(|&(slot, ability)| {
            clear
                && cooldowns.usable_at(slot, ability) <= latest
                && match ability.kind {
                    AbilityKind::Dash(dash) => distance > attack_within && distance < dash.distance * 0.9,
                    AbilityKind::Projectile(throw) => distance < throw.range * 0.9,
                    AbilityKind::Nova(nova) => distance < nova.radius * 0.8,
                    AbilityKind::Prime(_) => distance < attack_within && attack.primed.is_none(),
                }
        })
        .map(|(slot, _)| slot);
    desired.0 = PlayerInput { move_to, aim: to_target, fire, ..default() };
    if let Some(slot) = cast {
        desired.0.order_cast(slot, to_target);
    }
}
