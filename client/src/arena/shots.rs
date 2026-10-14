//! What shots and attacks look like: each class's shot (`shot_look`), the glint on a thrown
//! weapon's point, a thrown spear's wind and ground streak, bolts and melee lanes.

use super::*;

use arena_shared::classes::{ClassDef, Shot};

/// A flat strip along world +X from `from` to `to`, `width` wide: the lane a shot will fly down.
pub fn lane_mesh(from: f32, to: f32, width: f32) -> Mesh {
    let mut b = FlatMesh::default();
    let side = Vec3::Z * width / 2.0;
    let (near, far) = (Vec3::X * from, Vec3::X * to);
    b.quad([near + side, far + side, far - side, near - side], Color::WHITE);
    b.build()
}

/// How a class's shot looks (see `SHOT_LOOKS`). Never in its owner's colors: a shot looks the
/// same whoever throws it.
#[derive(Clone)]
pub struct ShotLook<M = Mesh> {
    pub mesh: M,
    pub glow: ShotGlow,
    /// A thrown spear: leaves the thrower's hand where the javelin is held (gripped `GRIP_TO_TIP`
    /// behind its point) and trails wind (`wind_mesh`).
    pub thrown: bool,
    /// A see-through glow around it and trailing behind, faded by its vertex alpha (`frost_aura`).
    pub aura: Option<M>,
    /// How fast (radians per second) it turns about its flight.
    pub spin: f32,
}

/// What a shot's mesh is drawn in.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ShotGlow {
    /// Its own colors (a real weapon, drawn plain), with a white spark on its point.
    Weapon,
    /// Spectral blue: an ability.
    Spirit,
    /// Glowing ice, its colors in the mesh.
    Ice,
    /// A plain pale glow.
    Pale,
}

/// Turns a mesh pointing up (+Y) to point along +X: how a shot flies.
pub(super) const ALONG_X: Quat = Quat::from_xyzw(0.0, 0.0, -std::f32::consts::FRAC_1_SQRT_2, std::f32::consts::FRAC_1_SQRT_2);

/// A class's shot (auto-attack or, `ability`, Q), flying along world +X, the point that hits at
/// the origin, picked by class id: the javelinist throws the very javelin it carries (and its Q
/// is a plain spectral spear, drawn glowing); the frost mage a frostbolt; the rest round bolts of
/// the shot's radius.
pub fn shot_look(class: &ClassDef, shot: Shot, ability: bool) -> ShotLook {
    let plain = |mesh, glow| ShotLook { mesh, glow, thrown: false, aura: None, spin: 0.0 };
    match (class.id.as_str(), ability) {
        ("javelinist", false) => ShotLook {
            thrown: true,
            ..plain(carried_javelin().rotated_by(ALONG_X), ShotGlow::Weapon)
        },
        ("frost_mage", false) => ShotLook {
            aura: Some(frost_aura(shot.radius)),
            spin: FROSTBOLT_SPIN,
            ..plain(frostbolt(shot.radius), ShotGlow::Ice)
        },
        _ => plain(gem_mesh(shot.radius), ShotGlow::Pale),
    }
}

/// The white glow on a thrown spear's point: a small bright spark stretched along +X, at the
/// origin.
pub fn shot_tip_mesh() -> Mesh {
    ball(0.04).scaled_by(Vec3::new(3.0, 1.0, 1.0))
}

/// The wind behind a thrown spear: one thin, straight streak, 1 m long along -X from the origin
/// (stretched to length as it flies), two crossed ribbons so it shows from any angle. It keeps
/// nearly its width all the way and fades out toward the back.
pub fn wind_mesh() -> Mesh {
    let mut b = FlatMesh::default();
    // Stations down the streak: how far along it (0..1), its width and opacity there.
    let stations = [(0.0, 1.0, 0.2), (0.12, 1.0, 1.0), (0.6, 0.8, 0.45), (1.0, 0.5, 0.0)];
    for side in [Vec3::Z, Vec3::Y] {
        let at = |(t, w, a): (f32, f32, f32)| (Vec3::X * -t, side * 0.022 * w, a);
        for pair in stations.windows(2) {
            let ((near, near_half, near_alpha), (far, far_half, far_alpha)) = (at(pair[0]), at(pair[1]));
            b.tri_faded([near - near_half, far - far_half, near + near_half], [near_alpha, far_alpha, near_alpha]);
            b.tri_faded([near + near_half, far - far_half, far + far_half], [near_alpha, far_alpha, far_alpha]);
            // Both faces: it's seen from either side.
            b.tri_faded([near + near_half, far - far_half, near - near_half], [near_alpha, far_alpha, near_alpha]);
            b.tri_faded([near + near_half, far + far_half, far - far_half], [near_alpha, far_alpha, far_alpha]);
        }
    }
    b.build()
}

/// A thrown spear's shadow, flat on the ground along -X from under its point at the origin: a
/// thin streak as long as the spear, darkest just behind the point, fading out toward the butt.
pub fn spear_shadow_mesh() -> Mesh {
    let mut b = FlatMesh::default();
    // Stations down the streak: how far along it (0..1), its half width and opacity there.
    let stations = [(0.0, 0.0, 0.6), (0.1, 0.05, 1.0), (0.35, 0.045, 0.8), (1.0, 0.02, 0.0)];
    for pair in stations.windows(2) {
        let [(near, near_half, a), (far, far_half, z)] = [pair[0], pair[1]].map(|(t, w, a)| (Vec3::X * -t * SPEAR_LENGTH, Vec3::Z * w, a));
        b.tri_faded_both([near - near_half, far - far_half, near + near_half], [a, z, a]);
        b.tri_faded_both([near + near_half, far - far_half, far + far_half], [a, z, z]);
    }
    b.build()
}

/// A small faceted ball: round shots, flower heads.
pub fn gem_mesh(radius: f32) -> Mesh {
    faceted(Sphere::new(radius).mesh().ico(0).unwrap())
}
