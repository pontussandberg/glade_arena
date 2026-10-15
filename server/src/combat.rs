//! Attacks and what they do: winding up and going off (`attack`), who's a foe of whom, hits
//! dealing damage (`damage`, each one noted in the `HitLog` for what reacts to hits: items,
//! monsters' twists), and the rifts a hit can tear in the ground.

use super::*;

use arena_shared::abilities::AbilitySlot;
use arena_shared::classes::{Chill, common};
use arena_shared::items::Item;
use arena_shared::sim;

use crate::rooms::{InRoom, Lobby};

/// What a hit does: damage, and crowd control on top. Only an auto-attack can crit: its
/// `crit_roll`, rolled once for the whole attack, so a swing that crits crits everyone it hits
/// (each target still judged against its own odds: frozen or not). `empowered` by the attack's
/// `follow_up`, its odds are better (`Crit::empowered`); primed (`AttackState::primed`), it's a
/// `sure_crit`.
#[derive(Clone, Copy)]
pub(super) struct Blow {
    pub(super) amount: i32,
    pub(super) chill: Chill,
    pub(super) crit_roll: Option<f32>,
    pub(super) empowered: bool,
    pub(super) sure_crit: bool,
}

impl Blow {
    /// An auto-attack's blow, its crit rolled (a `primed` one's sure).
    pub(super) fn attack(amount: i32, chill: Chill, empowered: bool, primed: bool) -> Blow {
        Blow { amount, chill, crit_roll: Some(fastrand::f32()), empowered, sure_crit: primed }
    }

    /// An ability's: never a crit.
    pub(super) fn ability(amount: i32, chill: Chill) -> Blow {
        Blow { amount, chill, crit_roll: None, empowered: false, sure_crit: false }
    }
}

/// Who dealt a hit, of which class (whose crit odds it rolls), and `with` what (for the log).
pub(super) type Attacker<'a> = (PeerId, ClassId, &'a str);

/// Server-only: how much harder than its class's numbers a fighter hits (a monster grown with
/// the waves), or a shot (a bounced or split one, a share of its first's).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub(crate) struct Might(pub(crate) f32);

/// What `Might` a fighter or a shot has (1 without one).
pub(crate) fn might_of(might: Option<&Might>) -> f32 {
    might.map_or(1.0, |m| m.0)
}

/// `amount` of damage, `might` times as hard.
pub(crate) fn mighty(amount: i32, might: f32) -> i32 {
    (amount as f32 * might).round() as i32
}

/// Heals a living fighter `amount`, up to `max`, shown as a heal. How much it got back.
pub(crate) fn mend(health: &mut Health, max: &MaxHealth, hits: &mut RecentHits, amount: i32) -> i32 {
    let healed = (health.0 + amount).min(max.0);
    if health.0 <= 0 || healed <= health.0 {
        return 0;
    }
    let gained = healed - health.0;
    hits.push(gained, HitKind::Heal);
    health.0 = healed;
    gained
}

/// Server-only: no one can hurt it (the shopkeeper).
#[derive(Component)]
pub(crate) struct Untouchable;

/// What dealt a hit: an auto-attack, an ability, an item's effect, or a monster's trick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Source {
    Attack,
    /// The burst round an auto-attack's mark (`AttackDef::burst`): the attack's damage, but it
    /// sets off no on-hit items (only the one it struck does).
    Splash,
    Ability(AbilitySlot),
    Item(Item),
    Trick,
}

/// A hit that landed, for what reacts to hits (`items::react`) once the tick's attacks are done.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Landed {
    pub(crate) attacker: PeerId,
    pub(crate) class: ClassId,
    pub(crate) side: Side,
    pub(crate) victim: Entity,
    pub(crate) victim_id: PeerId,
    /// Where the victim was.
    pub(crate) at: Vec2,
    pub(crate) amount: i32,
    pub(crate) killed: bool,
    pub(crate) source: Source,
    /// A primed auto-attack's (Deadeye's): an ability's doing, as much as the attack's.
    pub(crate) primed: bool,
    /// The shot that hit, if one did: what bounces off it.
    pub(crate) shot: Option<ShotHit>,
}

/// A shot that hit: it, its might, and how many bounces it has left (`None`: it's never bounced).
#[derive(Clone, Copy, Debug)]
pub(crate) struct ShotHit {
    pub(crate) projectile: Projectile,
    pub(crate) might: f32,
    pub(crate) bounces: Option<u8>,
    /// It pierced others before this one (`AttackDef::pierces`): what bounces off a shot does so
    /// off its first hit only.
    pub(crate) pierced: bool,
}

/// Server-only: the hits landed this tick, in order, until `items::react` has seen them.
#[derive(Resource, Default)]
pub(crate) struct HitLog(pub(crate) Vec<Landed>);

/// Where a fighter (or a projectile) fights: which room, which side.
pub(super) type Side = (InRoom, Team);

/// Whether something on `side` can hurt someone on `other`: same room, not allies.
pub(super) fn foes(side: Side, other: Side) -> bool {
    side.0 == other.0 && !side.1.allied(other.1)
}

/// Apply a hit dealt by `attacker` (on `side`, with `source`) to a fighter at `at`; at zero
/// health it's out of the fight for `RESPAWN_TICKS`. A blow that can crit rolls on the attacker's
/// class's odds, against whether the target is frozen now: a root this very hit applies starts
/// next tick, so it doesn't count. Noted in `log`.
#[allow(clippy::too_many_arguments)]
pub(super) fn damage(
    commands: &mut Commands,
    log: &mut HitLog,
    now: u32,
    (player, id, at): (Entity, PeerId, Vec2),
    (health, chilled, hits): (&mut Health, &mut Chilled, &mut RecentHits),
    blow: Blow,
    (by, class, with): Attacker,
    (side, source, shot): (Side, Source, Option<ShotHit>),
) {
    // Killed already this tick (its `Dead` is still a command to apply): later hits do nothing,
    // so the kill stays the first killer's.
    if health.0 == 0 || blow.amount <= 0 && blow.chill.is_none() {
        return;
    }
    let crit = blow.sure_crit || blow.crit_roll.is_some_and(|roll| roll < class.def().crit.chance_against(chilled.rooted_at(now), blow.empowered));
    let amount = if crit { (blow.amount as f32 * common().crit_multiplier).round() as i32 } else { blow.amount };
    health.0 = (health.0 - amount).max(0);
    // A blow of crowd control alone (a slow, a freeze) shows no number.
    if amount > 0 {
        hits.push(amount, if crit { HitKind::Crit } else { HitKind::Damage });
    }
    if !blow.chill.is_none() {
        chilled.apply(blow.chill, now);
    }
    debug!(target: "arena_server", attacker = ?by, victim = ?id, with, amount, crit, health = health.0, "hit");
    let killed = health.0 == 0;
    if killed {
        // A player's falls are worth knowing (and a monster's kills of them); monsters' deaths aren't.
        if matches!(id, PeerId::Netcode(_)) {
            info!(target: "arena_server", killer = ?by, killer_class = class.def().name, victim = ?id, with, amount, "kill");
        } else {
            debug!(target: "arena_server", killer = ?by, victim = ?id, with, "kill");
        }
        commands.entity(player).insert(Dead { respawn_at: now + RESPAWN_TICKS, placed: false, killer: by });
    }
    log.0.push(Landed { attacker: by, class, side, victim: player, victim_id: id, at, amount, killed, source, primed: blow.sure_crit, shot });
}

pub(super) type Targets<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PlayerId,
        (&'static InRoom, &'static Team),
        &'static Pos,
        &'static mut Health,
        &'static mut Chilled,
        &'static mut RecentHits,
        &'static MaxHealth,
        Has<Sheltered>,
    ),
    (InPlay, Without<Untouchable>),
>;

/// Deals `blow` to every foe of `attacker` (on `side`) that `hits` says is hit, judged against
/// where they are now (no lag compensation, as in League: what the server sees is what counts).
/// Returns how many.
#[allow(clippy::too_many_arguments)]
pub(super) fn hit_foes(
    commands: &mut Commands,
    log: &mut HitLog,
    now: u32,
    targets: &mut Targets,
    attacker: Attacker,
    (side, source): (Side, Source),
    blow: Blow,
    mut hits: impl FnMut(PeerId, Vec2) -> bool,
) -> usize {
    let mut count = 0;
    for (target, target_id, (room, team), target_pos, mut health, mut chilled, mut recent, _, sheltered) in targets {
        // One killed already this tick (`damage`) isn't hit: a swing that only struck a body
        // didn't land. Nor is one just back and sheltered.
        if health.0 > 0 && !sheltered && target_id.0 != attacker.0 && foes(side, (*room, *team)) && hits(target_id.0, target_pos.0) {
            let hit = (&mut *health, &mut *chilled, &mut *recent);
            damage(commands, log, now, (target, target_id.0, target_pos.0), hit, blow, attacker, (side, source, None));
            count += 1;
        }
    }
    count
}

/// Auto-attacks, as they go off after their windup: projectiles are spawned; melee swings are
/// resolved right here. Starting an attack drops the destination: afterwards the fighter stands
/// still until its player orders it somewhere new. Walking keys still held walk on once the
/// windup is over (it roots the fighter meanwhile: `sim::move_player`); nothing would resend
/// them.
pub(super) fn attack(
    mut commands: Commands,
    timeline: Res<LocalTimeline>,
    lobby: Res<Lobby>,
    mut attackers: Query<
        (&PlayerId, &ClassId, &Pos, &mut PlayerInput, &mut AttackState, &mut LastSwing, (&InRoom, &Team), &Frenzy, &mut Cooldowns, Option<&Might>),
        InPlay,
    >,
    mut targets: Targets,
    mut rifts: ResMut<Rifts>,
    mut log: ResMut<HitLog>,
) {
    let now = timeline.tick();
    for (id, class, pos, mut input, mut state, mut last_swing, (room, team), frenzy, mut cooldowns, might) in &mut attackers {
        let might = might_of(might);
        let side = (*room, *team);
        // A primed attack going off this tick is a sure crit (`primer`: the ability that readied it).
        let primer = state.primed.filter(|_| state.winding_up().is_some_and(|swing| swing.primed)).map(|(slot, _)| slot);
        let primed = primer.is_some();
        // In two halves (`sim::step_attack`): a swing that lands arms a quick attack in between.
        let (mut next, released) = sim::release_attack(now.0, id.0, *class, pos.0, *state);
        // The prime that readied it is used up now: its cooldown starts.
        if released.is_some()
            && let Some(slot) = primer
            && let Some(ability) = class.def().ability(slot)
        {
            cooldowns.start_own(slot, ability, now.0);
        }
        match released {
            None => {}
            Some(sim::Attack::Projectile(projectile)) => {
                spawn_projectile(&mut commands, Projectile { crit: primed, ..projectile }, side, might);
            }
            Some(sim::Attack::Melee(swing)) => {
                *last_swing = swing;
                if let Some(rift) = sim::Rift::torn(id.0, *class, pos.0, &swing) {
                    rifts.0.push((*room, rift));
                }
                let amount = mighty(class.def().attack.damage, might);
                let blow = Blow::attack(amount, class.def().attack.chill, swing.empowered, primed);
                let with = (id.0, *class, "auto-attack");
                let landed = hit_foes(&mut commands, &mut log, now.0, &mut targets, with, (side, Source::Attack), blow, |_, at| {
                    sim::melee_hits(lobby.map(room), pos.0, swing.dir, swing.empowered, *class, at)
                });
                if landed > 0
                    && let Some(follow_up) = class.def().attack.follow_up
                {
                    next.quick_until = Some(now.0 + follow_up.within_ticks);
                    // The rest of this attack's timer runs at the empowered speed already.
                    if let Some(swing) = next.swing.as_mut() {
                        swing.hastened = true;
                    }
                }
            }
        }
        // A frenzy speeds up the attack it starts, all through.
        let next = sim::start_attack_hasted(now.0, *class, &input, next, frenzy.at(now.0 as f32));
        if next.swing.is_some_and(|w| w.started_at == now.0) {
            input.move_to = None;
        }
        // Only on change: AttackState is replicated.
        state.set_if_neq(next);
    }
}

/// Server-only: knocked back along `dir` (a monster hit by a player's auto-attack), `left` meters
/// of it still to go.
#[derive(Component)]
pub(crate) struct Shoved {
    pub(crate) dir: Vec2,
    pub(crate) left: f32,
}

/// How far a player's auto-attack knocks a monster back (a bigger one less; bosses not at all),
/// and how fast it slides.
pub(crate) const KNOCKBACK: f32 = 0.55;
const SHOVE_SPEED: f32 = 7.0;

/// The knocked back slide, a step a tick, until it's gone its way or meets a wall.
pub(super) fn shove(mut commands: Commands, lobby: Res<Lobby>, mut shoved: Query<(Entity, &mut Pos, &InRoom, &mut Shoved)>) {
    for (entity, mut pos, room, mut shove) in &mut shoved {
        let step = shove.left.min(SHOVE_SPEED * TICK_DT);
        let next = pos.0 + shove.dir * step;
        if lobby.map(room).line_walkable(pos.0, next) {
            pos.0 = next;
            shove.left -= step;
        } else {
            shove.left = 0.0;
        }
        if shove.left <= 1e-4 {
            commands.entity(entity).remove::<Shoved>();
        }
    }
}

/// Server-only: ground torn open by strikes (`RiftDef`), in their rooms, while they're open.
#[derive(Resource, Default)]
pub(super) struct Rifts(pub(super) Vec<(InRoom, sim::Rift)>);

/// Rifts close when their time is up.
pub(super) fn close_rifts(timeline: Res<LocalTimeline>, mut rifts: ResMut<Rifts>) {
    let now = timeline.tick().0;
    rifts.0.retain(|(_, rift)| rift.until > now);
}
