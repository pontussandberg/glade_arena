//! A simple sparring bot: drives `DesiredInput` instead of the mouse. Melee classes chase the
//! nearest enemy and swing; ranged classes keep their distance and shoot. It clicks to attack
//! whenever it's in range (the server drops clicks until its last attack is done), and stands
//! still while one winds up, since moving would cancel it. Turn it on with
//! `ARENA_BOT=1` (native client) to have someone to fight when testing alone. Against the waves,
//! between them it walks up to the merchant, buys what it can afford (its class's own items
//! first, then the cheapest), and says it's ready.

use arena_shared::abilities::AbilityKind;
use arena_shared::items::{Item, Satchel};
use arena_shared::protocol::*;
use bevy::prelude::*;
use lightyear::prelude::client::RemoteTimeline;
use lightyear::prelude::*;

use crate::rooms::{CurrentMap, CurrentRoom, request};
use crate::{DesiredInput, Me, PlayerControls, SendCommands};

/// How far inside its lane's ends (`AttackDef::hit_span`) a melee bot starts a swing.
const MELEE_LEAD: f32 = 0.6;
/// How many foes this close make the bot back off rather than fight, once it's hurt this badly
/// (a share of its health).
const HURT: f32 = 0.4;
const CROWDED_WITHIN: f32 = 3.5;
const CROWDED_BY: usize = 3;

/// Present when this client is played by the bot.
#[derive(Resource)]
pub struct Bot;

pub struct BotPlugin;

impl Plugin for BotPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Bot);
        app.configure_sets(Update, PlayerControls.run_if(|| false));
        // Decide every frame, right before what changed is sent.
        app.add_systems(Update, (think, go_shopping).chain().before(SendCommands));
    }
}

fn think(
    me: Query<(&PlayerId, &ClassId, &Pos, &AttackState, &Cooldowns, Option<&Team>, (Option<&Health>, Option<&MaxHealth>)), With<Me>>,
    server: Query<&RemoteTimeline, With<Client>>,
    // Not the shopkeeper: no one can hurt them.
    others: Query<(&PlayerId, &Pos, &Health, Option<&Team>), (Without<Me>, Without<Shopkeeper>)>,
    mut desired: ResMut<DesiredInput>,
    map: Res<CurrentMap>,
) {
    let map = map.map();
    let Ok((my_id, class, me, attack, cooldowns, my_team, (health, max))) = me.single() else { return };
    let hurt = health.zip(max).is_some_and(|(h, m)| (h.0 as f32) < HURT * m.0 as f32);
    // The server tick our news is from: an ability whose cooldowns were over by then is ready.
    let latest = server.single().ok().and_then(|timeline| timeline.last_received_tick()).map_or(0, |tick| tick.0);
    let Some(target) = others
        .iter()
        // Foes only: not allies (the party, against the waves).
        .filter(|(id, _, health, team)| id.0 != my_id.0 && health.alive() && !my_team.zip(*team).is_some_and(|(mine, theirs)| mine.allied(*theirs)))
        .map(|(_, p, ..)| p.0)
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
    // Hurt and crowded (a pack all round, against the waves): back away from the middle of it.
    let near: Vec<Vec2> = others
        .iter()
        .filter(|(id, p, health, team)| id.0 != my_id.0 && health.alive() && p.0.distance(me.0) < CROWDED_WITHIN && !my_team.zip(*team).is_some_and(|(mine, theirs)| mine.allied(*theirs)))
        .map(|(_, p, ..)| p.0)
        .collect();
    if hurt && near.len() >= CROWDED_BY && attack.winding_up().is_none() {
        let middle = near.iter().copied().sum::<Vec2>() / near.len() as f32;
        let away = (me.0 - middle).normalize_or(-to_target.normalize_or_zero());
        let flee = [away, away.perp(), -away.perp()].into_iter().find_map(|dir| map.walk_target(me.0 + dir * 4.0, 2));
        desired.0 = PlayerInput { move_to: flee, aim: to_target, ..default() };
        return;
    }
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

/// How often the bot asks the merchant for something (seconds), so a refusal isn't repeated
/// every frame.
const ASK_EVERY: f32 = 0.5;

/// Between waves: to the merchant, buying what's affordable, then ready.
#[allow(clippy::too_many_arguments)]
fn go_shopping(
    time: Res<Time>,
    room: Res<CurrentRoom>,
    guest: Option<Res<crate::rooms::Me>>,
    me: Query<(&Pos, &ClassId, &Satchel), With<Me>>,
    merchant: Query<&Pos, With<Shopkeeper>>,
    mut desired: ResMut<DesiredInput>,
    mut sender: Query<&mut MessageSender<RoomRequest>, With<Client>>,
    mut asked_at: Local<f32>,
) {
    let Some(run) = room.0.as_ref().filter(|view| view.started).and_then(|view| view.run.as_ref()) else { return };
    if !matches!(run.phase, arena_shared::rooms::Phase::Shop { .. }) {
        return;
    }
    let (Ok((pos, class, satchel)), Some(shop)) = (me.single(), merchant.iter().next()) else { return };
    let Ok(mut sender) = sender.single_mut() else { return };
    let now = time.elapsed_secs();
    if now < *asked_at + ASK_EVERY {
        return;
    }
    // Its class's own items first, then the cheapest it can afford.
    let mut wanted: Vec<(Item, u32)> = Item::ALL.into_iter().filter_map(|item| Some((item, satchel.price_of(item, *class).ok()?))).filter(|(_, cost)| *cost <= satchel.gold).collect();
    wanted.sort_by_key(|(item, cost)| (item.class().is_none(), *cost));
    let ready = guest.is_some_and(|guest| run.ready.contains(&guest.guest_id));
    match wanted.first() {
        Some(_) if pos.distance(shop.0) > SHOP_RADIUS - 1.0 => desired.0.move_to = Some(shop.0),
        Some((item, _)) => {
            *asked_at = now;
            request(&mut sender, RoomRequest::Buy(*item));
        }
        None if !ready => {
            *asked_at = now;
            request(&mut sender, RoomRequest::Ready(true));
        }
        None => {}
    }
}
