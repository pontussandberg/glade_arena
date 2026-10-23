//! Spell effects on the ground: a nova's shockwave and burst, the frost on a chilled fighter,
//! and effects that fade out (`Fleeting`).

use super::*;

use arena_shared::abilities::AbilityKind;

use crate::Me;
use crate::abilities::SeenCasts;
use crate::feedback::AttackClock;
use crate::arena::{self, palette, to_world};

/// A nova: its shockwave races out to the edge in `NOVA_WAVE_SECONDS`, raising shards as it
/// passes (each takes `NOVA_GROW_SECONDS` to burst up); the frost on the ground and the shards
/// stay until `NOVA_SECONDS`, then sink.
const NOVA_WAVE_SECONDS: f32 = 0.22;
const NOVA_GROW_SECONDS: f32 = 0.1;
const NOVA_SECONDS: f32 = 0.75;
/// How fast (radians per second) the frost under a slowed fighter turns.
const RUNE_TURN_RATE: f32 = 0.8;

/// A nova's part; despawned `until` (seconds).
#[derive(Component)]
pub(super) struct Fleeting {
    pub(super) until: f32,
}

/// Something that bursts out from nothing: scaled up to `size` over `grow` seconds from `started`
/// (seconds; nothing shows before), and sinking back into the ground over its last
/// `NOVA_GROW_SECONDS` before it's gone (`Fleeting`).
#[derive(Component)]
pub(super) struct Burst {
    pub(super) started: f32,
    pub(super) grow: f32,
    pub(super) size: Vec3,
}

/// Frost on a fighter (child entities): the ice prison shown while it's rooted, the frost under
/// its feet while it's slowed; and what's shown now (slowed, rooted).
#[derive(Component)]
pub(super) struct Frost {
    pub(super) prison: Entity,
    pub(super) rune: Entity,
    pub(super) shown: (bool, bool),
}

/// Each new nova's burst: the caster's own (fire, or the ground heaving: `fx::cast_nova`), or
/// else a burst of ice: a bright shockwave races out from the caster to the nova's edge, a spray
/// of ice erupts where the staff struck, frost spreads over the ground, and shards burst up in
/// two rings as the wave passes them. `Cooldowns` comes from the server (others' on the same
/// delayed timeline as their positions); each nova is drawn once, and one cast before we saw its
/// caster isn't (`SeenCasts`).
pub(super) fn show_novas(
    mut commands: Commands,
    time: Res<Time>,
    mut visuals: ResMut<Visuals>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    art: Res<crate::fx::FxArt>,
    mut casters: Query<(&PlayerId, &ClassId, &Pos, &Cooldowns, &mut SeenCasts), Changed<Cooldowns>>,
    me: Query<&Pos, With<crate::Me>>,
    mut sounds: ResMut<crate::audio::SoundQueue>,
) {
    for (id, class, pos, cooldowns, mut seen) in &mut casters {
        for slot in seen.new_casts(cooldowns) {
            let Some(AbilityKind::Nova(def)) = class.def().ability(slot).map(|ability| ability.kind) else { continue };
            let Some(nova) = visuals.novas.get(&(*class, slot)).cloned() else { continue };
            if let Ok(my) = me.single() {
                // A nova sounds like what the caster brings up out of the ground.
                let sfx = match class.def().figure() {
                    "frost_mage" => crate::audio::Sfx::FrostNova,
                    "pyre_witch" => crate::audio::Sfx::FireNova,
                    _ => crate::audio::Sfx::Slam,
                };
                sounds.push(sfx, 0.8 * crate::audio::earshot(pos.0.distance(my.0), 18.0));
            }
            let seed = cooldowns.used_at(slot).unwrap_or(0).wrapping_mul(2_654_435_761) ^ pos.0.x.to_bits();
            if crate::fx::cast_nova(&mut commands, &art, time.elapsed_secs(), class.def().figure(), pos.0, def.radius, seed) {
                continue;
            }
            burst_nova(&mut commands, &time, &mut visuals, &mut materials, id.0, pos.0, def.radius, nova);
        }
    }
}

/// One nova's burst of ice, `radius` around `at`, cast by `owner`.
#[allow(clippy::too_many_arguments)]
fn burst_nova(
    commands: &mut Commands,
    time: &Time,
    visuals: &mut Visuals,
    materials: &mut Assets<StandardMaterial>,
    owner: PeerId,
    at: Vec2,
    radius: f32,
    nova: arena::NovaMeshes<Handle<Mesh>>,
) {
    let frost = visuals.material(materials, owner, Look::Frost);
    let wave = visuals.material(materials, owner, Look::Spirit);
    let now = time.elapsed_secs();
    // Each piece starts at nothing and only grows from there (`grow_bursts`); `until` is how
    // long it stays.
    let mut burst = |mesh: &Handle<Mesh>, material: &Handle<StandardMaterial>, place: Vec3, dir: Vec2, until: f32, burst: Burst| {
        let placed = Transform::from_translation(place).with_rotation(Quat::from_rotation_y(dir.to_angle()));
        commands.spawn((
            Fleeting { until: now + until },
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            placed.with_scale(Vec3::splat(1e-3)),
            burst,
        ));
    };
    let at_once = |grow| Burst { started: now, grow, size: Vec3::ONE };
    burst(&nova.disc, &frost, to_world(at, 0.05), Vec2::X, NOVA_SECONDS, at_once(NOVA_WAVE_SECONDS));
    burst(&nova.ring, &wave, to_world(at, 0.07), Vec2::X, NOVA_WAVE_SECONDS + 0.06, at_once(NOVA_WAVE_SECONDS));
    burst(&nova.eruption, &frost, to_world(at, 0.0), Vec2::X, NOVA_SECONDS * 0.6, at_once(0.07));
    // Shards in two rings, the outer one taller; each bursts up as the wave reaches it.
    for (count, out, tall, turn) in [(9, 0.5, 0.45, 0.35), (18, 0.92, 0.7, 0.0)] {
        for i in 0..count {
            let dir = Vec2::from_angle(i as f32 / count as f32 * std::f32::consts::TAU + turn);
            let height = tall * (0.75 + 0.5 * ((i * 7) % 4) as f32 / 3.0);
            let reached = Burst { started: now + NOVA_WAVE_SECONDS * out * 0.9, grow: NOVA_GROW_SECONDS, size: Vec3::splat(height) };
            burst(&nova.shard, &frost, to_world(at + dir * radius * out, 0.0), dir, NOVA_SECONDS, reached);
        }
    }
}

/// Bursts grow out to full size (quickly at first), then sink away just before they're gone.
pub(super) fn grow_bursts(time: Res<Time>, mut bursts: Query<(&Burst, &Fleeting, &mut Transform)>) {
    let now = time.elapsed_secs();
    for (burst, fx, mut transform) in &mut bursts {
        let grown = crate::ease::ease_out(0.0, burst.grow, now - burst.started);
        let sinking = ((fx.until - now) / NOVA_GROW_SECONDS).clamp(0.0, 1.0);
        let scale = (burst.size * Vec3::new(grown, grown * sinking, grown)).max(Vec3::splat(1e-3));
        if transform.scale != scale {
            transform.scale = scale;
        }
    }
}

/// Frost on every fighter: slowed, its colors take a cold blue cast and frost turns slowly under
/// its feet; rooted, ice locks its feet instead. `Chilled` comes from the server; its spans are
/// compared with the timeline each fighter is shown on, so others'
/// frost comes and goes in step with their (delayed) bodies.
pub(super) fn show_frost(
    clock: AttackClock,
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut fighters: Query<(&Chilled, &MeshMaterial3d<StandardMaterial>, &mut Frost, Has<Me>, Option<&super::Tint>)>,
    mut parts: Query<(&mut Visibility, &mut WorldAligned)>,
) {
    let turn = Quat::from_rotation_y(time.elapsed_secs() * RUNE_TURN_RATE);
    for (chilled, material, mut frost, is_me, tint) in &mut fighters {
        let now = clock.now(is_me);
        let (slowed, rooted) = (chilled.slowed.covers(now), chilled.rooted.covers(now));
        if slowed && !rooted
            && let Ok((_, mut aligned)) = parts.get_mut(frost.rune)
        {
            aligned.0 = turn;
        }
        if frost.shown == (slowed, rooted) {
            continue;
        }
        frost.shown = (slowed, rooted);
        if let Some(mut m) = materials.get_mut(&material.0) {
            let own = tint.map_or(Color::WHITE, |t| t.0).to_linear();
            let cast = if slowed || rooted { palette::FROSTBITE.to_linear() } else { LinearRgba::WHITE };
            m.base_color = Color::from(LinearRgba::rgb(own.red * cast.red, own.green * cast.green, own.blue * cast.blue));
        }
        for (part, visible) in [(frost.prison, rooted), (frost.rune, slowed && !rooted)] {
            if let Ok((mut visibility, _)) = parts.get_mut(part) {
                visibility.set_if_neq(shown(visible));
            }
        }
    }
}

pub(super) fn end_fleeting(mut commands: Commands, time: Res<Time>, fleeting: Query<(Entity, &Fleeting)>) {
    for (entity, fx) in &fleeting {
        if time.elapsed_secs() >= fx.until {
            commands.entity(entity).despawn();
        }
    }
}
