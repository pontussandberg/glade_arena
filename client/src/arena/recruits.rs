//! The recruits' looks: the five classes that joined after the first three. Each is built from
//! the same parts language as the others (rings swept into tubes, belts, hoods, booted legs),
//! with its own silhouette and weapon:
//!
//! - The warden: a knight in pale plate under a cream tabard, a crested round helm, an arming
//!   sword, and a round shield strapped to the lead arm.
//! - The pyre witch: a burgundy robe charred at the hem, a long hood swept back to a point, and
//!   a gnarled staff cradling an ember that burns (`pyre_staff_glow`).
//! - The shade: wrapped in near-black, a short hood and a shrouded scarf, a knife in each hand
//!   (the lead one held reversed).
//! - The breaker: a hulk in oxblood and fur over bare arms, a flat-topped horned helm, and a
//!   long-hafted maul.
//! - The stormweaver: a storm-grey knee-length coat, a tall hood blown back to a long point, and
//!   a forked staff holding a storm gem (`storm_staff_glow`).

use super::*;

use crate::sculpt::{Section, bevel_box, smoothed, sweep, tube};

/// A plain hanging arm: a shoulder ring down a sleeve to a fist, each section its own color.
fn sleeved_arm(shoulder: Color, sleeve: Color, wrist: Color, hand: Color) -> Mesh {
    let arm = tube(
        &smoothed(
            &[
                ring(0.0, 0.03, (0.068, 0.068), shoulder),
                ring(0.0, -0.1, (0.058, 0.058), sleeve),
                ring(0.0, -0.22, (0.052, 0.052), sleeve),
                ring(0.0, -0.34, (0.047, 0.047), wrist),
                ring(0.0, -0.4, (0.043, 0.043), wrist),
            ],
            2,
        ),
        12,
        (true, false),
    );
    sculpted(vec![arm, fist(hand, -0.42, 0.044)])
}

// ---------------------------------------------------------------- the warden

/// Plate and tabard: a steel cuirass flaring at the shoulders over iron faulds, a cream tabard
/// panel with a gold stripe down the front, belted in dark leather with a gold buckle.
pub(super) fn warden_body() -> Mesh {
    let (steel, iron) = (palette::STEEL, palette::IRON);
    let cuirass = tube(
        &smoothed(
            &[
                ring(0.01, 0.6, (0.155, 0.19), iron),
                ring(0.01, 0.78, (0.165, 0.2), iron),
                ring(0.01, 0.98, (0.16, 0.21), steel),
                ring(0.02, 1.18, (0.155, 0.225), steel).squared(2.2),
                ring(0.0, 1.31, (0.13, 0.255), steel).squared(2.6),
                ring(-0.01, 1.38, (0.105, 0.22), iron).squared(2.2),
            ],
            2,
        ),
        24,
        (true, true),
    );
    let collar = band(1.35, 1.42, 0.105, 10, iron);
    let tabard = bevel_box(Vec3::new(0.035, 0.52, 0.17), 0.012, palette::AWNING_CREAM).translated_by(Vec3::new(0.165, 0.95, 0.0));
    let stripe = bevel_box(Vec3::new(0.038, 0.52, 0.045), 0.012, palette::ui::ACCENT).translated_by(Vec3::new(0.166, 0.95, 0.0));
    let girdle = belt(0.01, (0.76, 0.83), (0.17, 0.205), palette::DARK_LEATHER);
    let buckle = bevel_box(Vec3::new(0.02, 0.055, 0.055), 0.008, palette::GOLD).translated_by(Vec3::new(0.182, 0.795, 0.0));
    sculpted(vec![cuirass, collar, faceted(tabard), faceted(stripe), girdle, faceted(buckle)])
}

/// A round helm with a visor slit and a low gold crest running front to back.
pub(super) fn warden_helm() -> Mesh {
    let steel = palette::STEEL;
    let dome = faceted(tube(
        &smoothed(
            &[
                ring(0.0, -0.02, (0.09, 0.09), steel),
                ring(0.01, 0.07, (0.115, 0.115), steel),
                ring(0.01, 0.17, (0.12, 0.12), steel),
                ring(0.0, 0.27, (0.1, 0.1), steel),
                ring(-0.01, 0.33, (0.05, 0.05), steel),
                ring(-0.01, 0.35, (0.0, 0.0), steel),
            ],
            1,
        ),
        14,
        (true, true),
    ));
    let slit = tinted(ball(1.0).scaled_by(Vec3::new(0.028, 0.016, 0.082)).translated_by(Vec3::new(0.108, 0.165, 0.0)), palette::DARK_LEATHER);
    let crest = faceted(bevel_box(Vec3::new(0.2, 0.07, 0.022), 0.008, palette::ui::ACCENT).translated_by(Vec3::new(0.0, 0.33, 0.0)));
    sculpted(vec![dome, slit, crest])
}

pub(super) const WARDEN_EYES: Vec3 = Vec3::new(0.106, 0.167, 0.038);

/// The sword arm: a pauldron over mail down to a bracer and a gloved fist.
pub(super) fn warden_arm() -> Mesh {
    let arm = sleeved_arm(palette::SLATE, palette::SLATE, palette::IRON, palette::LEATHER);
    let pauldron = tinted(ball(1.0).scaled_by(Vec3::new(0.085, 0.07, 0.095)).translated_by(Vec3::new(0.0, 0.0, 0.0)), palette::IRON);
    sculpted(vec![arm, pauldron])
}

/// The shield arm: the same arm with a round shield strapped along the forearm, boss out.
pub(super) fn warden_shield_arm() -> Mesh {
    let out = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
    let face = faceted(cylinder(0.26, 0.03, 14).rotated_by(out).translated_by(Vec3::new(0.085, -0.3, 0.0)));
    let rim = faceted(cylinder(0.275, 0.015, 14).rotated_by(out).translated_by(Vec3::new(0.075, -0.3, 0.0)));
    let boss = tinted(ball(0.05).scaled_by(Vec3::new(0.7, 1.0, 1.0)).translated_by(Vec3::new(0.115, -0.3, 0.0)), palette::GOLD);
    sculpted(vec![warden_arm(), tinted(face, palette::IRON), tinted(rim, palette::ui::ACCENT), boss])
}

/// An arming sword: a straight pale blade out of an ash crossguard, a leather grip, a gold pommel.
pub(super) fn arming_sword() -> Mesh {
    let steel = palette::STEEL;
    let blade = faceted(tube(
        &[
            Section::new(Vec3::Y * 0.18, Vec2::new(0.011, 0.05), steel).squared(1.0).exact(),
            Section::new(Vec3::Y * 0.65, Vec2::new(0.01, 0.042), steel).squared(1.0).exact(),
            Section::new(Vec3::Y * 0.92, Vec2::new(0.009, 0.03), palette::SILVER).squared(1.0).exact(),
            Section::new(Vec3::Y * 1.02, Vec2::new(0.0, 0.0), palette::SILVER).exact(),
        ],
        4,
        (true, true),
    ));
    let guard = faceted(bevel_box(Vec3::new(0.045, 0.03, 0.22), 0.01, palette::ASH).translated_by(Vec3::Y * 0.16));
    let grip = band(-0.02, 0.14, 0.02, 8, palette::DARK_LEATHER);
    let pommel = tinted(ball(0.03).translated_by(Vec3::Y * -0.04), palette::GOLD);
    sculpted(vec![blade, guard, grip, pommel])
}

// ------------------------------------------------------------ the pyre witch

/// A long burgundy robe flaring to the ground, charred at the hem, sashed in red.
pub(super) fn pyre_witch_body() -> Mesh {
    let coat = palette::MERCHANT_COAT;
    let robe = tube(
        &smoothed(
            &[
                ring(0.0, 0.03, (0.235, 0.265), palette::ASH).folded(0.035, 7),
                ring(0.01, 0.28, (0.195, 0.215), coat).folded(0.02, 7),
                ring(0.01, 0.7, (0.15, 0.17), coat),
                ring(0.01, 1.0, (0.14, 0.17), coat),
                ring(0.02, 1.2, (0.14, 0.2), coat).squared(2.2),
                ring(0.0, 1.33, (0.12, 0.23), coat).squared(2.6),
                ring(-0.01, 1.39, (0.1, 0.19), palette::ASH),
            ],
            2,
        ),
        28,
        (true, true),
    );
    let sash = belt(0.01, (0.8, 0.88), (0.155, 0.185), palette::AWNING_RED);
    let knot = tinted(ball(1.0).scaled_by(Vec3::new(0.035, 0.05, 0.035)).translated_by(Vec3::new(0.14, 0.82, 0.1)), palette::AWNING_RED);
    sculpted(vec![robe, sash, knot])
}

/// A deep hood swept far back into a witch's point.
pub(super) fn pyre_witch_hood() -> Mesh {
    let coat = palette::MERCHANT_COAT;
    let top = [
        ring(-0.05, 0.22, (0.19, 0.19), coat),
        ring(-0.08, 0.36, (0.15, 0.14), coat),
        ring(-0.14, 0.48, (0.07, 0.06), coat),
        ring(-0.26, 0.58, (0.022, 0.02), coat),
        ring(-0.36, 0.6, (0.0, 0.0), coat),
    ];
    sculpted(deep_hood(coat, palette::AWNING_RED, palette::DARK_LEATHER, &top))
}

/// The witch's arm: a coat sleeve flaring open and torn at the wrist, a pale hand.
pub(super) fn pyre_witch_arm() -> Mesh {
    let coat = palette::MERCHANT_COAT;
    let sleeve = tube(
        &smoothed(
            &[
                ring(0.0, 0.03, (0.07, 0.07), coat),
                ring(0.0, -0.14, (0.06, 0.06), coat),
                ring(0.0, -0.26, (0.062, 0.062), coat),
                ring(0.01, -0.36, (0.088, 0.088), coat).torn(0.04),
            ],
            2,
        ),
        14,
        (true, false),
    );
    sculpted(vec![sleeve, fist(palette::SKIN, -0.44, 0.04)])
}

/// A gnarled dark staff whose top splits into three prongs cupping the ember (the ember itself
/// glows: `pyre_staff_glow`).
pub(super) fn pyre_staff() -> Mesh {
    let wood = palette::BARK.darker(0.12);
    let shaft = sweep(&[Vec3::Y * -0.25, Vec3::new(0.012, 0.5, 0.0), Vec3::new(-0.01, 1.18, 0.0)], &[0.024, 0.021, 0.018], 7, wood);
    let mut parts = vec![shaft];
    for k in 0..3 {
        let around = Quat::from_rotation_y(k as f32 / 3.0 * std::f32::consts::TAU);
        let prong = sweep(
            &[Vec3::new(0.0, 1.16, 0.0), Vec3::new(0.055, 1.26, 0.0), Vec3::new(0.045, 1.4, 0.0)],
            &[0.014, 0.011, 0.0],
            6,
            wood,
        );
        parts.push(prong.rotated_by(around));
    }
    parts.push(band(-0.05, 0.13, 0.026, 8, palette::OXBLOOD));
    sculpted(parts)
}

/// The ember the staff cradles, drawn glowing.
pub(super) fn pyre_staff_glow() -> Mesh {
    tinted(gem_mesh(0.055).scaled_by(Vec3::new(0.9, 1.4, 0.9)).translated_by(Vec3::Y * 1.3), Color::WHITE)
}

// ----------------------------------------------------------------- the shade

/// Wrapped in near-black: a slim torn tunic, a chest strap of old blood, a shrouded scarf.
pub(super) fn shade_body() -> Mesh {
    let cloth = palette::HUNTER;
    let tunic = tube(
        &smoothed(
            &[
                ring(0.01, 0.58, (0.125, 0.155), cloth).torn(0.05),
                ring(0.01, 0.8, (0.125, 0.155), cloth),
                ring(0.01, 1.0, (0.12, 0.16), cloth),
                ring(0.02, 1.2, (0.12, 0.175), cloth).squared(2.0),
                ring(0.0, 1.32, (0.105, 0.195), cloth).squared(2.4),
                ring(-0.01, 1.38, (0.09, 0.165), palette::HUNTER_DARK),
            ],
            2,
        ),
        22,
        (true, true),
    );
    let strap = belt(0.01, (1.0, 1.06), (0.125, 0.165), palette::OXBLOOD);
    let waist = belt(0.01, (0.76, 0.81), (0.13, 0.16), palette::DARK_LEATHER);
    let scarf = band(1.32, 1.42, 0.1, 10, palette::SHROUD);
    sculpted(vec![tunic, strap, waist, scarf])
}

/// A short, tight hood ending in a low point.
pub(super) fn shade_hood() -> Mesh {
    let cloth = palette::HUNTER;
    let top = [
        ring(-0.04, 0.2, (0.17, 0.17), cloth),
        ring(-0.07, 0.32, (0.13, 0.12), cloth),
        ring(-0.12, 0.42, (0.06, 0.055), cloth),
        ring(-0.18, 0.47, (0.0, 0.0), cloth),
    ];
    sculpted(deep_hood(cloth, palette::HUNTER_ASH, palette::HUNTER_DARK, &top))
}

/// The knife arm, wrapped forearm to shoulder.
pub(super) fn shade_arm() -> Mesh {
    sleeved_arm(palette::HUNTER, palette::HUNTER_ASH, palette::HUNTER, palette::LEATHER)
}

/// The lead arm holds its knife reversed: the same arm, the blade running down past the fist.
pub(super) fn shade_offhand() -> Mesh {
    let blade = sweep(&[Vec3::new(0.02, -0.46, 0.0), Vec3::new(0.04, -0.72, 0.0)], &[0.02, 0.0], 4, palette::SILVER);
    sculpted(vec![shade_arm(), faceted(blade)])
}

/// A knife: a short bright blade (silver, or it vanishes against the wraps), an ash guard, a
/// dark grip.
pub(super) fn shade_knife() -> Mesh {
    let blade = faceted(tube(
        &[
            Section::new(Vec3::Y * 0.1, Vec2::new(0.011, 0.042), palette::SILVER).squared(1.0).exact(),
            Section::new(Vec3::Y * 0.32, Vec2::new(0.01, 0.032), palette::SILVER).squared(1.0).exact(),
            Section::new(Vec3::new(0.018, 0.48, 0.0), Vec2::new(0.0, 0.0), palette::SILVER).exact(),
        ],
        4,
        (true, true),
    ));
    let guard = faceted(bevel_box(Vec3::new(0.035, 0.02, 0.12), 0.007, palette::ASH).translated_by(Vec3::Y * 0.085));
    let grip = band(-0.02, 0.08, 0.015, 8, palette::DARK_LEATHER);
    let pommel = tinted(ball(0.02).translated_by(Vec3::Y * -0.035), palette::IRON);
    sculpted(vec![blade, guard, grip, pommel])
}

// --------------------------------------------------------------- the breaker

/// A hulk: oxblood over a leather skirt, a fur collar, heavy straps and a big iron buckle.
pub(super) fn breaker_body() -> Mesh {
    let torso = tube(
        &smoothed(
            &[
                ring(0.01, 0.58, (0.165, 0.195), palette::LEATHER),
                ring(0.01, 0.82, (0.175, 0.215), palette::OXBLOOD),
                ring(0.02, 1.04, (0.18, 0.235), palette::OXBLOOD),
                ring(0.02, 1.24, (0.175, 0.265), palette::OXBLOOD).squared(2.0),
                ring(0.0, 1.34, (0.14, 0.275), palette::FUR).squared(2.2),
                ring(-0.01, 1.41, (0.11, 0.215), palette::FUR),
            ],
            2,
        ),
        24,
        (true, true),
    );
    let girdle = belt(0.01, (0.74, 0.82), (0.175, 0.205), palette::DARK_LEATHER);
    let buckle = faceted(bevel_box(Vec3::new(0.025, 0.07, 0.07), 0.01, palette::IRON).translated_by(Vec3::new(0.19, 0.78, 0.0)));
    let strap = belt(0.02, (1.0, 1.08), (0.182, 0.24), palette::DARK_LEATHER);
    sculpted(vec![torso, girdle, buckle, strap])
}

/// A rounded iron helm with short horns swept up off its crown, and a dark eye slit.
pub(super) fn breaker_helm() -> Mesh {
    let iron = palette::IRON;
    let dome = faceted(tube(
        &smoothed(
            &[
                ring(0.0, -0.02, (0.095, 0.095), iron),
                ring(0.01, 0.08, (0.12, 0.12), iron),
                ring(0.01, 0.2, (0.122, 0.122), iron),
                ring(0.0, 0.3, (0.095, 0.095), iron),
                ring(-0.01, 0.35, (0.045, 0.045), iron),
                ring(-0.01, 0.37, (0.0, 0.0), iron),
            ],
            1,
        ),
        12,
        (true, true),
    ));
    let mut parts = vec![dome];
    for side in [1.0f32, -1.0] {
        // Up and out off the crown, curving back: a bull's sweep, not bolts at the ears.
        let out = Quat::from_rotation_x(-side * 0.8) * Quat::from_rotation_z(-0.25);
        let horn = cone(0.034, 0.17, 6).rotated_by(out).translated_by(Vec3::new(-0.02, 0.3, side * 0.09));
        parts.push(tinted(faceted(horn), palette::ASH));
    }
    parts.push(tinted(ball(1.0).scaled_by(Vec3::new(0.026, 0.015, 0.08)).translated_by(Vec3::new(0.112, 0.14, 0.0)), palette::DARK_LEATHER));
    sculpted(parts)
}

pub(super) const BREAKER_EYES: Vec3 = Vec3::new(0.11, 0.142, 0.042);

/// A bare arm under a fur shoulder, an iron bracer at the wrist.
pub(super) fn breaker_arm() -> Mesh {
    let arm = tube(
        &smoothed(
            &[
                ring(0.0, 0.03, (0.08, 0.08), palette::FUR),
                ring(0.0, -0.1, (0.067, 0.067), palette::SKIN),
                ring(0.0, -0.22, (0.06, 0.06), palette::SKIN),
                ring(0.0, -0.3, (0.055, 0.055), palette::SKIN),
            ],
            2,
        ),
        12,
        (true, false),
    );
    let bracer = band(-0.4, -0.3, 0.057, 10, palette::IRON);
    sculpted(vec![arm, bracer, fist(palette::SKIN, -0.42, 0.05)])
}

/// The maul: a long dark haft under a great iron head.
pub(super) fn maul() -> Mesh {
    let wood = palette::BARK.darker(0.15);
    let shaft = sweep(&[Vec3::Y * -0.22, Vec3::Y * 0.5, Vec3::Y * 0.96], &[0.026, 0.023, 0.021], 7, wood);
    let collar = band(0.88, 0.96, 0.03, 8, palette::ASH);
    let head = faceted(bevel_box(Vec3::new(0.2, 0.26, 0.44), 0.035, palette::IRON).translated_by(Vec3::Y * 1.06));
    let grip = band(-0.06, 0.14, 0.028, 8, palette::DARK_LEATHER);
    sculpted(vec![shaft, collar, head, grip])
}

// ----------------------------------------------------------- the stormweaver

/// The stormweaver's colors: a storm-grey coat with a darker lining, a sea-glass sash, and the
/// pale charged blue its gem and eyes burn with.
pub(super) const STORM_COAT: Color = Color::srgb_u8(0x26, 0x36, 0x42);
pub(super) const STORM_DARK: Color = Color::srgb_u8(0x14, 0x1E, 0x26);
pub(super) const STORM_GLOW: Color = Color::srgb_u8(0x9C, 0xD8, 0xFF);

/// A knee-length coat whipped open at the hem, sashed in sea-glass green over dark legs.
pub(super) fn stormweaver_body() -> Mesh {
    let coat = STORM_COAT;
    let body = tube(
        &smoothed(
            &[
                ring(0.0, 0.42, (0.185, 0.215), coat).torn(0.06).folded(0.025, 6),
                ring(0.01, 0.68, (0.15, 0.17), coat),
                ring(0.01, 0.94, (0.14, 0.165), coat),
                ring(0.02, 1.16, (0.14, 0.19), coat).squared(2.2),
                ring(0.0, 1.31, (0.12, 0.225), coat).squared(2.6),
                ring(-0.01, 1.38, (0.1, 0.185), STORM_DARK),
            ],
            2,
        ),
        26,
        (true, true),
    );
    // The legs showing under the coat, down into the boots.
    let legs = tube(&[ring(0.0, 0.1, (0.13, 0.15), STORM_DARK), ring(0.0, 0.46, (0.13, 0.15), STORM_DARK)], 14, (true, false));
    let sash = belt(0.01, (0.82, 0.9), (0.148, 0.178), palette::POND);
    sculpted(vec![body, legs, sash])
}

/// A tall hood blown back into a long point.
pub(super) fn stormweaver_hood() -> Mesh {
    let coat = STORM_COAT;
    let top = [
        ring(-0.05, 0.22, (0.18, 0.18), coat),
        ring(-0.1, 0.4, (0.12, 0.11), coat),
        ring(-0.2, 0.54, (0.05, 0.045), coat),
        ring(-0.33, 0.63, (0.015, 0.013), coat),
        ring(-0.43, 0.65, (0.0, 0.0), coat),
    ];
    sculpted(deep_hood(coat, STORM_GLOW, STORM_DARK, &top))
}

/// A coat sleeve gathered into folds at the wrist.
pub(super) fn stormweaver_arm() -> Mesh {
    let sleeve = tube(
        &smoothed(
            &[
                ring(0.0, 0.03, (0.068, 0.068), STORM_COAT),
                ring(0.0, -0.16, (0.058, 0.058), STORM_COAT),
                ring(0.01, -0.32, (0.072, 0.072), STORM_COAT).folded(0.018, 5),
            ],
            2,
        ),
        14,
        (true, false),
    );
    sculpted(vec![sleeve, fist(palette::SKIN, -0.42, 0.038)])
}

/// A forked iron staff; the storm gem floating in the fork glows (`storm_staff_glow`).
pub(super) fn storm_staff() -> Mesh {
    let iron = palette::IRON;
    let shaft = sweep(&[Vec3::Y * -0.25, Vec3::Y * 0.5, Vec3::Y * 1.14], &[0.022, 0.02, 0.018], 7, iron);
    let mut parts = vec![shaft];
    for side in [1.0f32, -1.0] {
        let prong = sweep(
            &[Vec3::new(0.0, 1.12, 0.0), Vec3::new(0.0, 1.24, side * 0.075), Vec3::new(0.0, 1.4, side * 0.055)],
            &[0.015, 0.012, 0.0],
            6,
            iron,
        );
        parts.push(prong);
    }
    parts.push(band(-0.04, 0.14, 0.024, 8, palette::DARK_LEATHER));
    sculpted(parts)
}

/// The gem in the fork, drawn glowing.
pub(super) fn storm_staff_glow() -> Mesh {
    tinted(gem_mesh(0.05).scaled_by(Vec3::new(0.8, 1.5, 0.8)).translated_by(Vec3::Y * 1.3), Color::WHITE)
}
