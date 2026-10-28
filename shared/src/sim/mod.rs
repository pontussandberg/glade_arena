//! Gameplay rules as pure functions of (state, input, tick).
//!
//! No wall-clock time and no randomness. The server runs them; clients use the few they draw
//! with (where a projectile is at a tick, whether a player has arrived). Per-class numbers come
//! from `ClassDef`.

use bevy::prelude::*;

use crate::config::*;

mod movement;
pub use movement::*;
mod attack;
pub use attack::*;
mod shots;
pub use shots::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abilities::dash::DashDef;
    use crate::abilities::{self, AbilityDef, AbilitySlot, Cast, CastOrder, Cooldowns};
    use crate::classes::{AbilityKind, AttackDef, AttackKind, Chill, ClassId, FollowUp, common};
    use crate::map::Map;
    use crate::protocol::{AttackState, Chilled, Dashing, Hasted, LastSwing, PlayerCommand, PlayerInput, Projectile, Swing, Waygate};
    use lightyear::prelude::PeerId;

    /// The tests walk and fight on the first map.
    fn map() -> &'static Map {
        crate::map::MapId::Crossing.map()
    }

    fn spawns() -> &'static [Vec2] {
        &map().spawn_points
    }

    const SPEED: f32 = 6.0;

    fn aim(aim: Vec2, fire: bool) -> PlayerInput {
        PlayerInput { aim, fire, ..default() }
    }

    fn walk_to(t: IVec2) -> PlayerInput {
        walk_to_point(Map::center(t))
    }

    fn walk_to_point(p: Vec2) -> PlayerInput {
        PlayerInput { move_to: Some(p), ..default() }
    }

    /// The first class whose auto-attack is a projectile / melee (tests don't hard-code classes).
    fn shooter() -> ClassId {
        ClassId::all().find(|c| matches!(c.def().attack.kind, AttackKind::Projectile { .. })).unwrap()
    }
    /// A class whose shot is spent at its range (doesn't fall).
    fn bolter() -> ClassId {
        ClassId::all().find(|c| matches!(c.def().attack.kind, AttackKind::Projectile { falls: false, .. })).unwrap()
    }
    /// A class whose shot is thrown: it drops under gravity from the hand.
    fn thrower() -> ClassId {
        ClassId::all().find(|c| matches!(c.def().attack.kind, AttackKind::Projectile { falls: true, .. })).unwrap()
    }
    fn fighter() -> ClassId {
        ClassId::all().find(|c| matches!(c.def().attack.kind, AttackKind::Melee { .. })).unwrap()
    }

    /// Walks until arrival (or `max_ticks`), checking every step on the way.
    fn walk(from: Vec2, to: IVec2, max_ticks: u32) -> (Vec2, u32) {
        walk_point(from, Map::center(to), max_ticks)
    }

    fn walk_point(from: Vec2, to: Vec2, max_ticks: u32) -> (Vec2, u32) {
        let mut p = from;
        for tick in 0..max_ticks {
            let next = step_player(map(), p, &walk_to_point(to), SPEED);
            assert!(next.distance(p) <= SPEED * TICK_DT + 1e-5, "too fast at tick {tick}");
            assert!(map().walkable_at(next), "walked onto a blocked tile at {next}");
            if next == p {
                return (p, tick);
            }
            p = next;
        }
        (p, max_ticks)
    }

    #[test]
    fn walks_where_its_keys_point_overriding_a_click_and_slides_along_walls() {
        let from = spawns()[0];
        let keys = PlayerInput { walk: Vec2::new(0.0, 2.0), move_to: Some(from + Vec2::new(3.0, 0.0)), ..default() };
        let next = step_player(map(), from, &keys, SPEED);
        assert!(next.abs_diff_eq(from + Vec2::Y * SPEED * TICK_DT, 1e-5), "walked to {next}");

        // Diagonally into blocked ground: never onto it, and still moving along the free axis
        // until both are blocked.
        let mut p = from;
        for dir in [Vec2::new(1.0, 1.0), Vec2::new(-1.0, 1.0), Vec2::new(1.0, -1.0), Vec2::new(-1.0, -1.0)] {
            let keys = PlayerInput { walk: dir, ..default() };
            for _ in 0..2000 {
                let next = step_player(map(), p, &keys, SPEED);
                assert!(next.distance(p) <= SPEED * TICK_DT + 1e-5);
                assert!(map().walkable_at(next), "walked onto a blocked tile at {next}");
                p = next;
            }
        }
    }

    #[test]
    fn walks_to_the_clicked_tile_at_player_speed() {
        let from = spawns()[0];
        let target = Map::tile_of(from) + IVec2::new(0, 3);
        let (end, ticks) = walk(from, target, 500);
        assert_eq!(end, Map::center(target));
        // 3 m at 6 m/s is 0.5 s.
        let half_second = TICK_HZ as u32 / 2;
        assert!((half_second - 1..=half_second + 2).contains(&ticks), "took {ticks} ticks");
    }

    #[test]
    fn walks_to_exactly_the_clicked_point_even_against_a_wall() {
        let from = spawns()[0];
        let off_center = from + Vec2::new(0.27, 2.81);
        assert_eq!(walk_point(from, off_center, 500).0, off_center);
        // A click right against a wall is pulled in to where a body fits, and reached.
        let m = map();
        let wall = m.tiles().find(|(t, tile)| *tile == crate::map::Tile::Wall && m.walkable(*t + IVec2::X)).unwrap().0;
        let clicked = m.walk_target(Map::center(wall + IVec2::X) - Vec2::new(0.49, 0.0), 4).unwrap();
        let (end, ticks) = walk_point(spawns()[3], clicked, TICK_HZ as u32 * 30);
        assert!(arrived(map(), end, clicked), "stuck at {end} after {ticks} ticks, short of {clicked}");
        // A point the client didn't pull in gets pulled in by the sim the same way: never stuck,
        // and arrival is recognized.
        let hugging = Map::center(wall + IVec2::X) - Vec2::new(0.49, 0.0);
        let end = walk_point(spawns()[3], hugging, TICK_HZ as u32 * 30).0;
        assert!(end == clicked && arrived(map(), end, hugging));
        assert_eq!(step_player(map(), from, &walk_to_point(Vec2::NAN), SPEED), from);
    }

    #[test]
    fn walks_around_walls_and_across_the_river() {
        let from = spawns()[1];
        let target = Map::tile_of(spawns()[5]);
        let (end, ticks) = walk(from, target, TICK_HZ as u32 * 30);
        assert_eq!(end, Map::center(target), "stuck after {ticks} ticks");
    }

    #[test]
    fn every_spawn_point_reaches_every_other() {
        for &from in spawns() {
            for &to in spawns() {
                let target = Map::tile_of(to);
                let (end, ticks) = walk(from, target, TICK_HZ as u32 * 30);
                assert_eq!(end, Map::center(target), "{from} -> {to}: stuck after {ticks} ticks");
            }
        }
    }

    #[test]
    fn unreachable_or_missing_target_means_standing_still() {
        let from = spawns()[0];
        let water = map().tiles().find(|(_, t)| *t == crate::map::Tile::Water).unwrap().0;
        assert_eq!(step_player(map(), from, &walk_to(water), SPEED), from);
        assert_eq!(step_player(map(), from, &walk_to(IVec2::new(-50, 3)), SPEED), from);
        assert_eq!(step_player(map(), from, &PlayerInput::default(), SPEED), from);
    }

    /// The whole attack and its windup at normal attack speed.
    fn timers(class: ClassId) -> (u32, u32) {
        let attack = &class.def().attack;
        (attack.period_ticks(1.0), attack.windup_ticks(1.0))
    }

    /// Runs the attack from `from` to `to` (inclusive), clicking toward +X on each tick in
    /// `clicks`, returning the ticks at which attacks went off and the final state.
    fn attack_ticks(class: ClassId, clicks: &[u32], from: u32, to: u32) -> (Vec<u32>, AttackState) {
        let mut state = AttackState::default();
        let mut released = Vec::new();
        for tick in from..=to {
            let input = aim(Vec2::X, clicks.contains(&tick));
            let (next, attack) = step_attack(tick, PeerId::Netcode(1), class, Vec2::ZERO, &input, state);
            state = next;
            if attack.is_some() {
                released.push(tick);
            }
        }
        (released, state)
    }

    #[test]
    fn an_attack_goes_off_after_its_windup_and_is_done_after_its_timer() {
        for class in ClassId::all() {
            let (period, windup) = timers(class);
            assert!(windup < period, "{}", class.def().id);
            let (released, state) = attack_ticks(class, &[10], 10, 10 + period);
            assert_eq!(released, vec![10 + windup], "{}", class.def().id);
            assert_eq!(state.released_at, Some(10 + windup), "{}: last release", class.def().id);
            assert_eq!(state.swing, None, "{}: still attacking after its timer", class.def().id);
        }
    }

    #[test]
    fn clicks_before_the_timer_is_up_are_dropped() {
        for class in ClassId::all() {
            let (period, windup) = timers(class);
            // Clicking every tick: one attack per timer.
            let every_tick: Vec<u32> = (10..10 + 3 * period).collect();
            let (released, _) = attack_ticks(class, &every_tick, 10, 10 + 3 * period);
            assert_eq!(released, vec![10 + windup, 10 + period + windup, 10 + 2 * period + windup], "{}", class.def().id);
            // A click a tick before it's up does nothing, even once it is: it isn't queued.
            let (released, state) = attack_ticks(class, &[10, 9 + period], 10, 10 + 2 * period);
            assert_eq!(released, vec![10 + windup], "{}: a click during the timer was kept", class.def().id);
            assert_eq!(state.swing, None);
        }
    }

    /// `class`'s attack, its `follow_up` quickening it a lot (no class's has to). The quickening
    /// is tested on this, so it stays tested whatever the classes are tuned to.
    fn quickened(class: ClassId) -> AttackDef {
        let mut attack = class.def().attack.clone();
        attack.follow_up = attack.follow_up.map(|f| FollowUp { attack_speed: 1.0, ..f });
        attack
    }

    #[test]
    fn an_armed_follow_up_quickens_the_whole_attack_once_and_only_in_time() {
        let Some(class) = ClassId::all().find(|c| c.def().attack.follow_up.is_some()) else { return };
        let attack = quickened(class);
        let (period, windup) = (attack.period_ticks(1.0), attack.windup_ticks(1.0));
        let speed = attack.speed(true);
        assert!(attack.windup_ticks(speed) < windup && attack.period_ticks(speed) < period, "follow_up doesn't quicken");
        let fire = aim(Vec2::X, true);
        let start = |at: u32, quick_until| {
            let state = AttackState { quick_until: Some(quick_until), ..default() };
            step_attack(at, PeerId::Netcode(1), class, Vec2::ZERO, &fire, state).0
        };
        let quick = start(10, 10).swing.unwrap();
        assert!(quick.quick, "not quick while armed");
        assert_eq!((quick.releases_at(&attack), quick.ready_at(&attack)), (10 + attack.windup_ticks(speed), 10 + attack.period_ticks(speed)));
        let late = start(11, 10).swing.unwrap();
        assert!(!late.quick, "quick after the window");
        assert_eq!(late.releases_at(&attack), 11 + windup);
    }

    #[test]
    fn the_follow_up_lasts_until_an_attack_goes_off_not_a_cancelled_one() {
        let Some(class) = ClassId::all().find(|c| c.def().attack.follow_up.is_some()) else { return };
        let fire = aim(Vec2::X, true);
        let armed = AttackState { quick_until: Some(100), ..default() };
        let winding = step_attack(10, PeerId::Netcode(1), class, Vec2::ZERO, &fire, armed).0;
        assert!(winding.swing.is_some_and(|s| s.quick), "not empowered while armed");
        // Cancelled: still armed, and the next attack is empowered.
        let cancelled = cancel_attack(winding);
        assert_eq!(cancelled.quick_until, Some(100), "cancelling used the follow_up up");
        let again = step_attack(12, PeerId::Netcode(1), class, Vec2::ZERO, &fire, cancelled).0;
        assert!(again.swing.is_some_and(|s| s.quick), "the attack after a cancelled one wasn't empowered");
        // Gone off: used up.
        let windup = again.swing.unwrap().windup_ticks(&class.def().attack);
        let (gone, released) = step_attack(12 + windup, PeerId::Netcode(1), class, Vec2::ZERO, &PlayerInput::default(), again);
        assert!(released.is_some());
        assert_eq!(gone.quick_until, None, "an empowered attack that went off kept the follow_up");
    }

    #[test]
    fn a_hit_that_arms_the_follow_up_shortens_what_is_left_of_its_attack() {
        let Some(class) = ClassId::all().find(|c| c.def().attack.follow_up.is_some()) else { return };
        let attack = quickened(class);
        let (period, windup) = (attack.period_ticks(1.0), attack.windup_ticks(1.0));
        let mut swing = Swing { started_at: 10, dir: Vec2::X, quick: false, hastened: false, haste: 0.0, primed: false };
        assert_eq!(swing.ready_at(&attack), 10 + period);
        // As the server does when the swing lands.
        swing.hastened = true;
        let backswing = (period - windup) as f32 / attack.speed(true);
        assert_eq!(swing.ready_at(&attack), 10 + windup + backswing.round() as u32);
        assert!(swing.ready_at(&attack) < 10 + period, "a hastened backswing wasn't any shorter");
    }

    #[test]
    fn a_click_shorter_than_the_windup_still_attacks() {
        let class = shooter();
        let (_, windup) = timers(class);
        let (released, _) = attack_ticks(class, &[10], 10, 10 + windup);
        assert_eq!(released, vec![10 + windup]);
    }

    #[test]
    fn only_the_windup_roots_you() {
        for class in ClassId::all() {
            let from = spawns()[0];
            let walk = walk_to(Map::tile_of(from) + IVec2::new(0, 3));
            let (_, windup) = timers(class);
            let (idle, warm) = (Dashing::default(), Chilled::default());
            let moves = |state: &AttackState| move_player(map(), from, &walk, class, state, &mut { idle }, &warm, &default(), 11) != from;
            let (_, winding) = attack_ticks(class, &[10], 10, 9 + windup);
            assert!(winding.winding_up().is_some() && !moves(&winding), "{}: moved during the windup", class.def().id);
            let (_, backswing) = attack_ticks(class, &[10], 10, 10 + windup);
            assert!(backswing.swing.is_some() && backswing.winding_up().is_none(), "{}: no backswing", class.def().id);
            assert!(moves(&backswing), "{}: rooted after the attack went off", class.def().id);
            assert!(moves(&AttackState::default()), "rooted without attacking");
        }
    }

    #[test]
    fn moving_cancels_the_windup_but_not_the_backswing() {
        for class in ClassId::all() {
            let (period, windup) = timers(class);
            // Mid-windup: it never goes off, and the next attack may start at once.
            let (_, winding) = attack_ticks(class, &[10], 10, 9 + windup);
            let cancelled = cancel_attack(winding);
            assert_eq!(cancelled.swing, None, "{}", class.def().id);
            let (_, released) = step_attack(10 + windup, PeerId::Netcode(1), class, Vec2::ZERO, &PlayerInput::default(), cancelled);
            assert!(released.is_none(), "{}: a cancelled attack went off", class.def().id);
            let restarted = start_attack(10 + windup, class, &aim(Vec2::X, true), cancelled);
            assert_eq!(restarted.swing.map(|s| s.started_at), Some(10 + windup), "{}: no attack after cancelling", class.def().id);
            // After it went off: the timer runs on.
            let (_, backswing) = attack_ticks(class, &[10], 10, 10 + windup);
            assert_eq!(cancel_attack(backswing), backswing, "{}: moving cut the attack timer short", class.def().id);
            let early = start_attack(9 + period, class, &aim(Vec2::X, true), backswing);
            assert_eq!(early.swing, backswing.swing, "{}: attacked again before the timer was up", class.def().id);
        }
    }

    #[test]
    fn an_order_to_move_drops_an_unseen_click() {
        let mut input = PlayerInput::default();
        PlayerCommand::Attack(Vec2::X).apply(&mut input);
        assert!(input.fire);
        PlayerCommand::Walk(Vec2::ZERO, 0).apply(&mut input);
        assert!(input.fire, "letting go of the walking keys isn't an order to move");
        PlayerCommand::Walk(Vec2::X, 0).apply(&mut input);
        assert!(input.fire, "the walking keys walk through an attack, so they keep the click");
        PlayerCommand::MoveTo(Vec2::ONE).apply(&mut input);
        assert!(!input.fire);
        PlayerCommand::Attack(Vec2::X).apply(&mut input);
        assert!(input.fire && input.move_to.is_some(), "only an attack that starts drops the walk (the server does)");
    }

    #[test]
    fn attack_without_aim_does_nothing() {
        let fire = aim(Vec2::ZERO, true);
        let (state, attack) = step_attack(0, PeerId::Netcode(1), shooter(), Vec2::ZERO, &fire, AttackState::default());
        assert!(attack.is_none() && state.swing.is_none());
    }

    #[test]
    fn projectile_travels_and_hits() {
        let shooter_pos = Vec2::new(-3.0, 0.0);
        let target = Vec2::new(3.0, 0.0);
        let input = aim(target - shooter_pos, true);
        let (_, windup) = timers(shooter());
        let (state, _) = step_attack(0, PeerId::Netcode(1), shooter(), shooter_pos, &input, AttackState::default());
        let Some(Attack::Projectile(proj)) = step_attack(windup, PeerId::Netcode(1), shooter(), shooter_pos, &input, state).1
        else {
            panic!("a projectile class should shoot after its windup");
        };
        let lifetime = proj.class.def().shot(None).unwrap().lifetime_ticks();
        let hit_tick = (windup..windup + lifetime).find(|&tick| projectile_hits(projectile_pos(&proj, tick as f32), &proj, tick, target, 0.0));
        assert!(hit_tick.is_some(), "projectile should hit a target 6 m away in its path");
    }

    #[test]
    fn projectile_expires_at_its_range_and_at_walls() {
        let proj = Projectile { owner: PeerId::Netcode(1), class: bolter(), origin: Vec2::ZERO, dir: Vec2::X, spawn_tick: 5, ability: None, crit: false, extra: false, climb: 0.0, lift: 0.0 };
        let lifetime = bolter().def().shot(None).unwrap().lifetime_ticks();
        assert!(!projectile_expired(map(), Vec2::ZERO, &proj, 5 + lifetime - 1));
        assert!(projectile_expired(map(), Vec2::ZERO, &proj, 5 + lifetime));
        assert!(projectile_expired(map(), Vec2::new(100.0, 0.0), &proj, 6), "left the map");
        let wall = map().tiles().find(|(_, t)| *t == crate::map::Tile::Wall).unwrap().0;
        assert!(projectile_expired(map(), Map::center(wall), &proj, 6), "hit a wall");
    }

    #[test]
    fn a_jump_goes_up_and_comes_down_and_a_level_shot_passes_under_it() {
        use crate::config::{JUMP_HEIGHT, JUMP_TICKS};
        let jump = crate::protocol::Jump { started_at: 10 };
        assert_eq!(jump.height_at(10.0), 0.0);
        assert!((jump.height_at(10.0 + JUMP_TICKS as f32 / 2.0) - JUMP_HEIGHT).abs() < 1e-5, "at the top halfway");
        assert_eq!(jump.height_at(10.0 + JUMP_TICKS as f32), 0.0);
        assert!(!jump.over(10 + JUMP_TICKS - 1) && jump.over(10 + JUMP_TICKS));
        let proj = Projectile { owner: PeerId::Netcode(1), class: bolter(), origin: Vec2::ZERO, dir: Vec2::X, spawn_tick: 0, ability: None, crit: false, extra: false, climb: 0.0, lift: 0.0 };
        let at = projectile_pos(&proj, 3.0);
        assert!(projectile_hits(at, &proj, 3, at, 0.0), "standing, it's hit");
        assert!(!projectile_hits(at, &proj, 3, at, SHOT_HEIGHT + 0.5), "jumped over it");
    }

    #[test]
    fn a_shot_over_the_top_of_a_wall_clears_it() {
        let wall = map().tiles().find(|(_, t)| *t == crate::map::Tile::Wall).unwrap().0;
        let at = Map::center(wall);
        let shot = bolter().def().shot(None).unwrap();
        // Level, it's stopped; aimed up enough to be over the wall's top by the time it's there
        // (a tick's flight out), it isn't.
        let over = (crate::map::Tile::Wall.top() + shot.radius) / (shot.speed * TICK_DT);
        for (climb, stopped) in [(0.0, true), (over, false)] {
            let proj = Projectile { owner: PeerId::Netcode(1), class: bolter(), origin: at - Vec2::X * shot.speed * TICK_DT, dir: Vec2::X, spawn_tick: 5, ability: None, crit: false, extra: false, climb, lift: 0.0 };
            assert_eq!(projectile_expired(map(), at, &proj, 5), stopped, "climbing {climb} per meter");
        }
    }

    #[test]
    fn thrown_projectile_drops_from_the_hand_and_lands() {
        let thrown = |climb| Projectile { owner: PeerId::Netcode(1), class: thrower(), origin: Vec2::ZERO, dir: Vec2::X, spawn_tick: 0, ability: None, crit: false, extra: false, climb, lift: 0.0 };
        let shot = thrower().def().shot(None).unwrap();
        let lands = |proj: &Projectile| {
            let down = (0..400).find(|&tick| projectile_expired(map(), projectile_pos(proj, tick as f32), proj, tick)).unwrap();
            let at = projectile_pos(proj, down as f32);
            assert!(!map().get(Map::tile_of(at)).blocks_shots(), "stopped by a wall at {at}, not the ground");
            assert!(down < shot.lifetime_ticks(), "given up on in the air, not down on the ground");
            assert!(projectile_height(proj, down as f32) < 0.0, "ended on the ground");
            at.x
        };
        // Thrown level: below the hand and nosing down from the first tick, down where a thing
        // falling from the hand's height at its speed lands (within the tick it lands in).
        let level = thrown(0.0);
        assert!(projectile_height(&level, 0.0) < SHOT_HEIGHT && projectile_pitch(&level, 0.0) < 0.0);
        let expected = shot.speed * (2.0 * SHOT_HEIGHT / GRAVITY).sqrt();
        let landed = lands(&level);
        assert!(landed >= expected && landed < expected + shot.speed * TICK_DT, "landed at {landed} m, expected {expected} m");
        // Thrown upward it flies past its range before coming down, and isn't spent there.
        let lobbed = thrown(0.1);
        let range_tick = (shot.range / shot.speed * TICK_HZ as f32).ceil() as u32;
        assert!(!projectile_expired(map(), projectile_pos(&lobbed, range_tick as f32), &lobbed, range_tick));
        assert!(lands(&lobbed) > shot.range, "a lob carries past the range");
    }

    const Q: AbilitySlot = AbilitySlot(0);

    /// `class`'s ability on Q.
    fn q(class: ClassId) -> &'static AbilityDef {
        class.def().ability(Q).unwrap()
    }

    /// The first class whose Q is a prime / a dash.
    fn primer() -> ClassId {
        ClassId::playable().find(|c| matches!(q(*c).kind, AbilityKind::Prime(_))).unwrap()
    }
    fn dasher() -> ClassId {
        ClassId::all().find(|c| matches!(q(*c).kind, AbilityKind::Dash(_))).unwrap()
    }
    fn dash_def() -> DashDef {
        let AbilityKind::Dash(dash) = q(dasher()).kind else { unreachable!() };
        dash
    }

    /// Q pressed toward `aim`, the cursor far off along it (a dash goes its full distance).
    fn press_q(aim: Vec2) -> PlayerInput {
        press(Q, aim * 100.0)
    }

    /// The ability in `slot` pressed, aimed `aim`.
    fn press(slot: AbilitySlot, aim: Vec2) -> PlayerInput {
        let mut input = PlayerInput::default();
        input.order_cast(slot, aim);
        input
    }

    /// A fighter's abilities: their cooldowns and its dash.
    #[derive(Clone, Copy, Default)]
    struct Kit {
        cooldowns: Cooldowns,
        dashing: Dashing,
    }

    /// One tick of `class`'s abilities at `pos` (`abilities::step`), on a copy of `kit`: the
    /// first thing cast, if anything was.
    fn step_kit(tick: u32, class: ClassId, pos: Vec2, input: &PlayerInput, chilled: &Chilled, mut kit: Kit) -> (Kit, Option<Cast>) {
        let cast = abilities::step(tick, PeerId::Netcode(1), class, pos, input, chilled, &mut kit.cooldowns, &mut kit.dashing);
        (kit, cast.into_iter().next())
    }

    #[test]
    fn a_dash_goes_until_the_body_reaches_the_cursor_within_its_least_and_most() {
        let class = dasher();
        let DashDef { distance, min_distance: Some(min), .. } = dash_def() else { panic!("the dash has no least distance") };
        let dash_to = |cursor: f32| {
            let input = press(Q, Vec2::X * cursor);
            step_kit(10, class, Vec2::ZERO, &input, &default(), default()).0.dashing.0.unwrap().left
        };
        let between = (min + distance) / 2.0;
        assert!((dash_to(between + PLAYER_RADIUS) - between).abs() < 1e-4, "the body didn't stop at the cursor");
        assert!((dash_to(min * 0.2) - min).abs() < 1e-4, "shorter than its least");
        assert!((dash_to(distance * 3.0) - distance).abs() < 1e-4, "further than its most");
    }

    #[test]
    fn with_no_cooldown_an_ability_still_waits_for_the_global_cooldown() {
        let class = dasher();
        assert!(q(class).global_cooldown);
        let free = Kit { cooldowns: Cooldowns { free: true, ..default() }, ..default() };
        let (state, thrown) = step_kit(10, class, Vec2::ZERO, &press_q(Vec2::X), &default(), free);
        assert!(thrown.is_some());
        let again = |tick| step_kit(tick, class, Vec2::ZERO, &press_q(Vec2::X), &default(), state).1;
        let global = common().global_cooldown_ticks;
        assert!(again(10 + global - 1).is_none(), "used again during the global cooldown");
        assert!(again(10 + global).is_some(), "its own cooldown should be none");
    }

    #[test]
    fn an_empty_slot_or_a_missing_aim_casts_nothing() {
        let class = dasher();
        let empty = AbilitySlot(class.def().abilities.len() as u8);
        assert!(step_kit(10, class, Vec2::ZERO, &press(empty, Vec2::X), &default(), default()).1.is_none(), "cast from an empty slot");
        assert!(step_kit(10, class, Vec2::ZERO, &press_q(Vec2::ZERO), &default(), default()).1.is_none(), "cast without an aim");
    }

    #[test]
    fn abilities_pressed_together_are_all_tried_in_order() {
        let mut input = press(Q, Vec2::X);
        input.order_cast(AbilitySlot(1), Vec2::Y);
        input.order_cast(Q, -Vec2::X);
        let orders: Vec<_> = input.casts().collect();
        assert_eq!(orders, [CastOrder { slot: Q, aim: -Vec2::X }, CastOrder { slot: AbilitySlot(1), aim: Vec2::Y }], "pressed again, Q kept its place and took the new aim");
        // A press the class has no ability for doesn't keep the one before it from going off.
        let class = primer();
        let mut both = press(AbilitySlot(class.def().abilities.len() as u8), Vec2::X);
        both.order_cast(Q, Vec2::X);
        assert!(matches!(step_kit(10, class, Vec2::ZERO, &both, &default(), default()).1, Some(Cast::Prime { .. })), "Q lost to a later press");
    }

    #[test]
    fn a_prime_needs_no_aim_and_readies_one_quicker_attack() {
        let class = primer();
        let Some(Cast::Prime { slot, prime }) = step_kit(10, class, Vec2::ZERO, &press_q(Vec2::ZERO), &default(), default()).1 else {
            panic!("no prime the tick Q is pressed")
        };
        let attack = &class.def().attack;
        let fire = aim(Vec2::X, true);
        let primed = AttackState { primed: Some((slot, prime.attack_speed)), ..default() };
        let plain = start_attack(20, class, &fire, AttackState::default()).swing.unwrap();
        let state = start_attack(20, class, &fire, primed);
        let swing = state.swing.unwrap();
        assert!(swing.primed && swing.windup_ticks(attack) < plain.windup_ticks(attack), "not quicker");
        // Cancelled, it stays ready; gone off, it's used up.
        assert_eq!(cancel_attack(state).primed, primed.primed);
        let (gone, released) = release_attack(swing.releases_at(attack), PeerId::Netcode(1), class, Vec2::ZERO, state);
        assert!(released.is_some() && gone.primed.is_none(), "still primed after it went off");
    }

    /// A dash from `from` toward `dir` run to its end: where it ended, and how many ticks it moved.
    fn dash_run(from: Vec2, dir: Vec2, chilled: &Chilled, hasted: &Hasted) -> (Vec2, u32) {
        let class = dasher();
        let (mut state, _) = step_kit(1, class, from, &press_q(dir), chilled, default());
        let (mut pos, mut tick) = (from, 1);
        while state.dashing.0.is_some() {
            tick += 1;
            assert!(tick < 1000, "the dash never ended");
            pos = move_player(map(), pos, &PlayerInput::default(), class, &AttackState::default(), &mut state.dashing, chilled, hasted, tick);
            assert!(map().walkable_at(pos), "dashed into a wall at {pos}");
            state = step_kit(tick, class, pos, &PlayerInput::default(), chilled, state).0;
        }
        (pos, tick - 1)
    }

    /// Every way out of every spawn point.
    fn dash_starts() -> impl Iterator<Item = (Vec2, Vec2)> {
        spawns().iter().copied().flat_map(|from| (0..8).map(move |i| (from, Vec2::from_angle(i as f32 * std::f32::consts::FRAC_PI_4))))
    }

    #[test]
    fn dashes_cover_their_distance_and_never_end_up_in_a_wall() {
        let distance = dash_def().distance;
        let mut longest: f32 = 0.0;
        for (from, dir) in dash_starts() {
            let (to, _) = dash_run(from, dir, &default(), &default());
            assert!(from.distance(to) <= distance + 1e-3, "dashed too far");
            longest = longest.max(from.distance(to));
        }
        assert!((longest - distance).abs() < 1e-3, "no open dash went its full distance ({longest})");
    }

    #[test]
    fn a_slow_or_a_haste_changes_how_fast_a_dash_goes_not_how_far() {
        let DashDef { distance, ticks, .. } = dash_def();
        let (from, dir) = dash_starts()
            .find(|&(from, dir)| (dash_run(from, dir, &default(), &default()).0.distance(from) - distance).abs() < 1e-3)
            .unwrap();
        let mut slowed = Chilled::default();
        slowed.apply(chiller().def().attack.chill, 0);
        let mut hasted = Hasted::default();
        hasted.start(0);
        let (end, took) = dash_run(from, dir, &default(), &default());
        assert_eq!(took, ticks);
        let (slowed_end, slowed_took) = dash_run(from, dir, &slowed, &default());
        let (hasted_end, hasted_took) = dash_run(from, dir, &default(), &hasted);
        assert!(slowed_end.distance(end) < 1e-3 && hasted_end.distance(end) < 1e-3, "went a different distance");
        assert!(slowed_took > ticks && hasted_took < ticks, "went at the same pace ({slowed_took}, {hasted_took})");
    }

    #[test]
    fn a_dash_mid_windup_moves_you_and_the_attack_still_goes_off() {
        let class = dasher();
        let from = spawns()[0];
        let (_, winding) = attack_ticks(class, &[10], 10, 10);
        let (kit, _) = step_kit(11, class, from, &press_q(Vec2::Y), &default(), default());
        assert!(kit.dashing.0.is_some(), "no dash mid-windup");
        let moved = move_player(map(), from, &PlayerInput::default(), class, &winding, &mut { kit.dashing }, &default(), &default(), 11);
        assert_ne!(moved, from, "the dash didn't move you mid-windup");
        let (_, windup) = timers(class);
        let (released, _) = (11..=10 + windup).fold((None, winding), |(released, state), tick| {
            let (next, attack) = step_attack(tick, PeerId::Netcode(1), class, moved, &PlayerInput::default(), state);
            (released.or(attack.map(|_| tick)), next)
        });
        assert_eq!(released, Some(10 + windup), "the dash cancelled the attack");
    }

    /// The first class whose auto-attack chills / whose Q is a nova.
    fn chiller() -> ClassId {
        ClassId::all().find(|c| !c.def().attack.chill.is_none()).unwrap()
    }
    fn novaer() -> ClassId {
        ClassId::all().find(|c| matches!(q(*c).kind, AbilityKind::Nova(_))).unwrap()
    }

    #[test]
    fn a_slow_starts_the_tick_after_the_hit_and_wears_off() {
        let chill = chiller().def().attack.chill;
        let mut chilled = Chilled::default();
        chilled.apply(chill, 100);
        assert_eq!(chilled.speed_factor(100), 1.0, "slowed on the tick of the hit");
        assert_eq!(chilled.speed_factor(101), 1.0 - chill.slow);
        assert_eq!(chilled.speed_factor(100 + chill.slow_ticks), 1.0 - chill.slow);
        assert_eq!(chilled.speed_factor(101 + chill.slow_ticks), 1.0, "still slowed after it wore off");

        // Slowed, a step covers that much less ground.
        let from = spawns()[0];
        let walk = walk_to(Map::tile_of(from) + IVec2::new(0, 3));
        let (attack, ability) = (AttackState::default(), Dashing::default());
        let step = |chilled: &Chilled| move_player(map(), from, &walk, chiller(), &attack, &mut { ability }, chilled, &default(), 101).distance(from);
        assert!((step(&chilled) - step(&Chilled::default()) * (1.0 - chill.slow)).abs() < 1e-5);

        // A weaker slow doesn't replace a stronger one; the same one again renews it.
        let mut again = chilled;
        again.apply(Chill { slow: chill.slow / 2.0, slow_ticks: 1000, root_ticks: 0, lift_ticks: 0 }, 110);
        assert_eq!(again, chilled);
        again.apply(chill, 110);
        assert_eq!(again.slowed, crate::protocol::Span { from: 111, until: 111 + chill.slow_ticks });
    }

    #[test]
    fn a_haste_doubles_walking_from_the_tick_after_and_wears_off() {
        let mut hasted = Hasted::default();
        hasted.start(100);
        let from = spawns()[0];
        let walk = walk_to(Map::tile_of(from) + IVec2::new(0, 3));
        let class = dasher();
        let (attack, idle, warm) = (AttackState::default(), Dashing::default(), Chilled::default());
        let step = |tick| move_player(map(), from, &walk, class, &attack, &mut { idle }, &warm, &hasted, tick).distance(from);
        let normal = class.def().move_speed * TICK_DT;
        assert!((step(100) - normal).abs() < 1e-5, "hasted on the tick it was taken");
        assert!((step(101) - normal * HASTE_FACTOR).abs() < 1e-5);
        assert!((step(100 + HASTE_TICKS) - normal * HASTE_FACTOR).abs() < 1e-5);
        assert!((step(101 + HASTE_TICKS) - normal).abs() < 1e-5, "still hasted after it wore off");
        // Dashes are sped up just as much.
        let (kit, _) = step_kit(101, class, from, &press_q(Vec2::X), &warm, default());
        let dash = |hasted: &Hasted| move_player(map(), from, &walk, class, &attack, &mut { kit.dashing }, &warm, hasted, 102).distance(from);
        assert!((dash(&hasted) - dash(&default()) * HASTE_FACTOR).abs() < 1e-5);
        assert!(touches_pickup(from, from + Vec2::X * PICKUP_RADIUS));
        assert!(!touches_pickup(from, from + Vec2::X * (PICKUP_RADIUS + 0.01)));
        // 15% of 95, rounded up.
        assert_eq!(heal_amount(95), 15);
    }

    #[test]
    fn rooted_means_no_walking_no_dashing_and_a_dash_stops() {
        let mut chilled = Chilled::default();
        chilled.apply(Chill { root_ticks: TICK_HZ as u32, ..default() }, 10);
        let from = spawns()[0];
        let walk = walk_to(Map::tile_of(from) + IVec2::new(0, 3));
        let class = dasher();
        let (attack, idle) = (AttackState::default(), Dashing::default());
        assert_eq!(move_player(map(), from, &walk, class, &attack, &mut { idle }, &chilled, &default(), 11), from, "walked while rooted");
        assert_ne!(move_player(map(), from, &walk, class, &attack, &mut { idle }, &chilled, &default(), 11 + TICK_HZ as u32), from, "still rooted after it wore off");

        let (state, _) = step_kit(11, class, from, &press_q(Vec2::X), &chilled, default());
        assert!(state.dashing.0.is_none() && state.cooldowns.used_at(Q).is_none(), "dashed (or spent the cooldown) while rooted");

        // A dash in progress when the root lands goes no further, and ends.
        let (dashing, _) = step_kit(9, class, from, &press_q(Vec2::X), &default(), default());
        assert!(dashing.dashing.0.is_some());
        assert_eq!(move_player(map(), from, &walk, class, &attack, &mut { dashing.dashing }, &chilled, &default(), 11), from, "dashed on while rooted");
        let (stopped, _) = step_kit(11, class, from, &PlayerInput::default(), &chilled, dashing);
        assert!(stopped.dashing.0.is_none(), "the dash should stop");
    }

    #[test]
    fn nova_bursts_at_once_around_the_caster_only() {
        let class = novaer();
        let AbilityKind::Nova(nova) = q(class).kind else { unreachable!() };
        let radius = nova.radius;
        let (state, cast) = step_kit(10, class, Vec2::ZERO, &press_q(Vec2::X), &default(), default());
        assert!(matches!(cast, Some(Cast::Nova { nova: cast, .. }) if cast == nova), "no nova the tick Q is pressed");
        assert_eq!(state.cooldowns.cooldown_over_at(Q), 10 + q(class).cooldown_ticks);
        let dirs = [Vec2::X, Vec2::Y, -Vec2::X, Vec2::new(0.6, -0.8)];
        // Open ground all around (walls stop novas too).
        let open = |p: Vec2| dirs.iter().all(|d| map().shot_clear(p, p + *d * (radius + 1.5)));
        let at = map().tiles().map(|(tile, _)| Map::center(tile)).find(|&p| open(p)).unwrap();
        for dir in dirs {
            assert!(nova.hits(map(), at, at + dir * (radius + PLAYER_RADIUS - 0.05)), "inside, toward {dir}");
            assert!(!nova.hits(map(), at, at + dir * (radius + PLAYER_RADIUS + 0.05)), "outside, toward {dir}");
        }
    }

    #[test]
    fn only_auto_attacks_chill() {
        let class = chiller();
        let shot = Projectile { owner: PeerId::Netcode(1), class, origin: Vec2::ZERO, dir: Vec2::X, spawn_tick: 1, ability: None, crit: false, extra: false, climb: 0.0, lift: 0.0 };
        assert_eq!(projectile_chill(&shot), class.def().attack.chill);
        assert!(projectile_chill(&Projectile { class: primer(), ability: Some(Q), ..shot }).is_none());
    }

    #[test]
    fn melee_hits_its_lane_within_range_only() {
        let class = fighter();
        let at = spawns()[0];
        let reach = class.def().attack.kind.reach() - 0.05;
        assert!(melee_hits(map(), at, Vec2::Y, (false, 0.0), class, at + Vec2::Y * reach), "straight ahead, at the edge of reach");
        assert!(!melee_hits(map(), at, Vec2::Y, (false, 0.0), class, at + Vec2::Y * (reach + 0.2)), "just out of reach");
        assert!(!melee_hits(map(), at, Vec2::Y, (false, 0.0), class, at - Vec2::Y * 1.0), "behind");
        let AttackKind::Melee { width, from, .. } = class.def().attack.kind else { unreachable!() };
        // Bodies overlapping connect when the lane starts at the attacker; one starting further
        // out misses what's closer than that.
        assert_eq!(melee_hits(map(), at, Vec2::Y, (false, 0.0), class, at - Vec2::Y * 0.3), from == 0.0, "bodies overlapping");
        if from > 0.0 {
            assert!(melee_hits(map(), at, Vec2::Y, (false, 0.0), class, at + Vec2::Y * (from - PLAYER_RADIUS + 0.05)), "touching the lane's near end");
            assert!(!melee_hits(map(), at, Vec2::Y, (false, 0.0), class, at + Vec2::Y * (from - PLAYER_RADIUS - 0.05)), "short of the lane");
        }
        let edge = width / 2.0 + PLAYER_RADIUS;
        let middle = from.max(1.0);
        assert!(melee_hits(map(), at, Vec2::Y, (false, 0.0), class, at + Vec2::new(edge - 0.05, middle)), "a body overlapping the lane");
        assert!(!melee_hits(map(), at, Vec2::Y, (false, 0.0), class, at + Vec2::new(edge + 0.05, middle)), "beside the lane");
        assert!(!melee_hits(map(), at, Vec2::Y, (false, 0.0), shooter(), at + Vec2::Y), "projectile classes don't swing");
    }

    #[test]
    fn a_rift_hastes_its_striker_in_it_until_it_closes() {
        let Some(class) = ClassId::all().find(|c| c.def().attack.rift.is_some()) else { return };
        let def = class.def().attack.rift.unwrap();
        let at = spawns()[0];
        let swing = LastSwing { tick: 10, dir: Vec2::Y, empowered: false, grown: 0.0 };
        let rift = Rift::torn(PeerId::Netcode(1), class, at, &swing).unwrap();
        let (from, range, _) = class.def().attack.lane(false).unwrap();
        let inside = at + Vec2::Y * (from + range) / 2.0;
        assert!(rift.holds(inside, 11) && rift.holds(inside, 9 + def.ticks), "not in its own rift");
        assert!(!rift.holds(inside, 10 + def.ticks), "still open after it closed");
        assert!(!rift.holds(at - Vec2::Y * 2.0, 11), "behind it");
        // Walking there is that much faster.
        let walk = PlayerInput { walk: Vec2::X, ..default() };
        let (idle, warm) = (Dashing::default(), Chilled::default());
        let plain = move_player(map(), at, &walk, class, &AttackState::default(), &mut { idle }, &warm, &default(), 11).distance(at);
        let boosted = move_player_boosted(map(), at, &walk, class, &AttackState::default(), &mut { idle }, &warm, &default(), 11, 1.0 + def.haste).distance(at);
        assert!((boosted - plain * (1.0 + def.haste)).abs() < 1e-4, "{boosted} vs {plain}");
    }

    #[test]
    fn an_empowered_swing_reaches_further_and_wider() {
        let Some(class) = ClassId::all().find(|c| c.def().attack.follow_up.is_some_and(|f| f.reach > 1.0)) else { return };
        let at = spawns()[0];
        let ((_, range, width), (near, far, wide)) = (class.def().attack.lane(false).unwrap(), class.def().attack.lane(true).unwrap());
        assert!(far > range && wide > width);
        let beyond = at + Vec2::Y * (range + PLAYER_RADIUS + 0.2);
        let aside = at + Vec2::new(width / 2.0 + PLAYER_RADIUS + (wide - width) / 4.0, (near + far) / 2.0);
        for target in [beyond, aside] {
            assert!(!melee_hits(map(), at, Vec2::Y, (false, 0.0), class, target), "a plain swing reached {target}");
            assert!(melee_hits(map(), at, Vec2::Y, (true, 0.0), class, target), "an empowered swing missed {target}");
        }
    }

    #[test]
    fn marsh_slows_walking_not_dashing() {
        use crate::map::{MARSH_SPEED, MapId, Tile};
        let keep = MapId::Keep.map();
        // A marsh tile with marsh all around, so a step stays in it.
        let bog = keep
            .tiles()
            .find(|(t, _)| (-1..=1).all(|dy| (-1..=1).all(|dx| keep.get(*t + IVec2::new(dx, dy)) == Tile::Marsh)))
            .expect("the keep has a marsh")
            .0;
        let from = Map::center(bog);
        let east = PlayerInput { walk: Vec2::X, ..default() };
        let stepped = step_player(keep, from, &east, SPEED).distance(from);
        assert!((stepped - SPEED * TICK_DT * MARSH_SPEED).abs() < 1e-5, "stepped {stepped}");
        let dry = keep.spawn_points[0];
        assert!((step_player(keep, dry, &east, SPEED).distance(dry) - SPEED * TICK_DT).abs() < 1e-5);
        // A dash goes at its own pace, marsh or not.
        let class = dasher();
        let DashDef { distance, ticks, .. } = dash_def();
        let mut dashing = Dashing(Some(dash_def().start(Q, Vec2::X, Vec2::X * 100.0, 10)));
        let warm = Chilled::default();
        let moved = move_player(keep, from, &PlayerInput::default(), class, &AttackState::default(), &mut dashing, &warm, &default(), 11);
        assert!((moved.distance(from) - distance / ticks as f32).abs() < 1e-4);
    }

    #[test]
    fn a_gate_end_is_under_whoever_stands_on_it() {
        let gate = Waygate { ends: [Vec2::new(-5.5, 2.5), Vec2::new(5.5, -2.5)], back_at: Some(150) };
        assert_eq!(gate_end_at(&gate, Vec2::new(-5.5, 2.9)), Some(0));
        assert_eq!(gate_end_at(&gate, Vec2::new(5.0, -2.5)), Some(1));
        assert_eq!(gate_end_at(&gate, Vec2::new(-5.5, 3.5)), None, "beside it");
        assert!(!gate.open_at(149) && gate.open_at(150));
    }

    #[test]
    fn spawn_point_avoids_other_players() {
        // Empty arena: any spawn point is fine.
        assert!(spawns().contains(&pick_spawn_point(map(), [])));
        // Every new player gets a free spot until the spawn points run out.
        let mut taken = Vec::new();
        for _ in 0..spawns().len() {
            let p = pick_spawn_point(map(), taken.clone());
            assert!(!taken.contains(&p), "{p} handed out twice");
            taken.push(p);
        }
        // A player standing near a spawn point makes it the least attractive one.
        let near_first = spawns()[0] + Vec2::new(0.5, 0.0);
        assert_ne!(pick_spawn_point(map(), [near_first]), spawns()[0]);
    }
}
