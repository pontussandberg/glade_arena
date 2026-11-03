//! Practice replay bots: a player's whole input recorded tick by tick and saved as a take
//! (`RoomRequest::Record`), then played back by a bot fighter (`RoomRequest::PlaceBot`) that
//! loops it from where it began -- the same moves, throws and casts, to spar against. The bot
//! runs through the very systems a player does (`combat::attack` and `orders` treat it as one),
//! so the take plays out as it was played.

use super::*;

use arena_shared::map::MapId;

use crate::rooms::{InRoom, Lobby};

/// Where the saved take lives, across restarts.
const TAKE_PATH: &str = "recordings/practice-bot.ron";

/// The longest take (ticks): recording stops and saves by itself there.
const LONGEST_TAKE: usize = 60 * TICK_HZ as usize;

/// A saved take: whose class, on which map and from where it plays, and the fighter's whole
/// input, tick by tick.
#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct Take {
    pub(crate) map: MapId,
    pub(crate) class: ClassId,
    pub(crate) start: Vec2,
    pub(crate) ticks: Vec<PlayerInput>,
}

/// Server-only: this player's inputs are being recorded (`record_inputs`); where the take began
/// is noted on its first tick.
#[derive(Component, Default)]
pub(crate) struct Recording {
    began: Option<(Vec2, ClassId, MapId)>,
    ticks: Vec<PlayerInput>,
}

/// Server-only: asked to stop recording and save the take (`RoomRequest::Record(false)`).
#[derive(Component)]
pub(crate) struct StopRecording;

/// Server-only: a bot playing a take back: its fighter takes `ticks[at]` as its input each tick,
/// and starts the take over, fresh at `start`, when it runs out or the bot is killed.
#[derive(Component)]
pub(crate) struct Replay {
    start: Vec2,
    ticks: Vec<PlayerInput>,
    at: usize,
}

/// Writes down each recording fighter's input, just as the tick's systems are about to take it
/// (after `take_due_orders` has brought the due orders in, before anything walks or attacks on
/// it), and saves the take when it's stopped or as long as one may be.
pub(super) fn record_inputs(
    mut commands: Commands,
    lobby: Res<Lobby>,
    mut recording: Query<(Entity, &mut Recording, &PlayerInput, &Pos, &ClassId, &InRoom, Has<StopRecording>)>,
) {
    for (player, mut recording, input, pos, class, room, stop) in &mut recording {
        if !stop {
            if recording.began.is_none() {
                recording.began = Some((pos.0, *class, lobby.map_id(room)));
            }
            recording.ticks.push(*input);
        }
        if stop || recording.ticks.len() >= LONGEST_TAKE {
            if let Some((start, class, map)) = recording.began {
                let take = Take { map, class, start, ticks: std::mem::take(&mut recording.ticks) };
                match save(&take) {
                    Ok(()) => info!(ticks = take.ticks.len(), class = class.def().name, "take saved"),
                    Err(why) => warn!(%why, "the take couldn't be saved"),
                }
            }
            commands.entity(player).remove::<(Recording, StopRecording)>();
        }
    }
}

fn save(take: &Take) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(std::path::Path::new(TAKE_PATH).parent().expect("a folder in the path"))?;
    std::fs::write(TAKE_PATH, ron::to_string(take)?)?;
    Ok(())
}

/// The saved take, if there's one to be read.
pub(crate) fn load_take() -> Result<Take, &'static str> {
    let text = std::fs::read_to_string(TAKE_PATH).map_err(|_| "No take saved: record one first")?;
    let take: Take = ron::from_str(&text).map_err(|_| "The saved take couldn't be read")?;
    if take.ticks.is_empty() {
        return Err("The saved take is empty");
    }
    Ok(take)
}

/// A bot fighter playing `take` in `room`: on no one's side (foe to all, like a dummy), hittable
/// and hitting back just as the take did.
pub(crate) fn spawn_replay(commands: &mut Commands, id: PeerId, room: InRoom, take: Take) -> Entity {
    commands
        .spawn((
            Name::from("Replay bot"),
            Replay { start: take.start, ticks: take.ticks, at: 0 },
            fighter(id, take.class, room, Team(arena_shared::rooms::NO_TEAM), take.start),
            (PlayerInput::default(), DashHits::default(), OnGate::default(), arena_shared::items::Satchel::default(), ItemState::default()),
            replicated_npc(),
        ))
        .id()
}

/// Each replay bot takes its take's next input, in `record_inputs`' own place in the tick. The
/// take run out, or the bot killed, it starts the take over: back where it began, fresh as a
/// restored dummy.
#[allow(clippy::type_complexity)]
pub(super) fn drive_replays(
    mut commands: Commands,
    lobby: Res<Lobby>,
    mut bots: Query<(
        Entity,
        &mut Replay,
        &InRoom,
        &MaxHealth,
        &mut PlayerInput,
        (&mut Pos, &mut arena_shared::protocol::Motion, &mut Health, &mut Chilled, &mut Hasted),
        (&mut AttackState, &mut Cooldowns, &mut Dashing, &mut Walking, &mut LastSwing),
        Has<Dead>,
    )>,
) {
    for (bot, mut replay, room, max, mut input, (mut pos, mut motion, mut health, mut chilled, mut hasted), (mut attack, mut cooldowns, mut dashing, mut walking, mut last_swing), dead) in &mut bots {
        if dead || replay.at >= replay.ticks.len() {
            replay.at = 0;
            let start = replay.start;
            pos.set_if_neq(Pos(start));
            motion.set_if_neq(arena_shared::protocol::Motion { height: lobby.map(room).height_at(start), ..default() });
            health.0 = max.0;
            chilled.set_if_neq(Chilled::default());
            hasted.set_if_neq(Hasted::default());
            attack.set_if_neq(AttackState::default());
            cooldowns.set_if_neq(Cooldowns::default());
            dashing.set_if_neq(Dashing::default());
            walking.set_if_neq(Walking::default());
            *last_swing = LastSwing::default();
            commands.entity(bot).remove::<Dead>();
        }
        *input = replay.ticks[replay.at];
        replay.at += 1;
    }
}
