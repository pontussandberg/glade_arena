# Art direction: The Glade

A low-poly forest clearing at dusk, medieval-fantasy ruins in nature: a river with a stone bridge
and fords, ruined walls, boulders and torches. Faceted shapes, muted greens and grey stone.
Fighters and their projectiles carry the only full-strength color, so a fight reads at a glance.
The layout lives in `shared/src/map.rs`; the scene is built from it, so what you see is exactly
what blocks movement and shots.

Live reference with a 3D preview (still shows the earlier square arena in afternoon light):
https://claude.ai/artifact/WgUKXJpxy2GkzWjWRVUfXY
Code: `client/src/glade.rs` (palette, scene, mesh helpers), used by `client/src/render.rs`.

## Three rules

1. **Facets, not detail.** Flat shading, few sides, no textures, one color per face.
2. **Saturation is earned.** The world stays below ~50% saturation and between 25% and 72%
   lightness. Only fighters and projectiles go above that.
3. **Warm light, cool shade.** Low dusk sun from the upper left, cool blue sky fill, and torches
   for warm pools of light.

## Palette

| Name | Hex | Role |
|---|---|---|
| Moss | `#6F8F4E` | Fight floor |
| Meadow | `#8AA05A` | Lighter floor facets, clearings |
| Fern | `#4E7748` | Foliage, lit side |
| Pine | `#2E4B3C` | Foliage, shade side |
| Slate | `#6C7684` | Boulders, rubble |
| Stone | `#848A93` | Bridge paving, stepping stones |
| Wall | `#69717D` | Ruined walls, parapets |
| Bark | `#5E4838` | Trunks, roots |
| Path | `#958C6E` | The road, bare earth |
| Bank | `#5A5141` | River banks |
| Riverbed | `#3D4A44` | Under the water |
| Pond | `#4F8682` | River surface, kept quiet |
| Heather | `#9A86B6` | Flower accents |
| Haze | `#A9B8B6` | Sky, fog, background (dusk) |
| Ink | `#1E2A23` | HUD text |
| **You** | `#4C9EE0` | Your fighter and shots |
| **Ember** | `#E8803A` | Rival |
| **Marigold** | `#E6B23A` | Rival |
| **Coral** | `#E35F5A` | Rival |

Warm saturated hues mean an opponent, and you are the only saturated blue. Keep world colors
away from both: no autumn oranges, and the river stays a quiet teal. The one exception is fire:
torch flames (`#FFB257`, light `#FF9443`) are small, static and flicker, so they never read as a
fighter.

## Light & air

- Sun `#FFE2C4`, ~22° elevation (dusk), from the upper left, shadows on, one cascade.
- Sky fill `#8FA4C2` as ambient light, kept low so torches matter.
- Torches: warm point lights (range 12 m, no shadows) that flicker, on the bridge and the ruins.
- Haze `#A9B8B6`: linear fog 40–95 m, same color as the clear color.
- Everything matte (roughness 0.95, no metal). Only projectiles glow (owner color × 4).

## Shape language

| Asset | Build | Budget |
|---|---|---|
| Ground | Flat 1 m tiles, one color per tile (moss toward meadow, path on the road) plus a faint ±3% checker; earthen banks down to the river | ≈ 4k tris |
| River | Flat water strip following the river's curve, banks and riverbed below | low |
| Bridge | Paving slabs per tile, parapets with end caps, piers into the river | blocks |
| Ruined wall | Stone blocks 0.9–1.7 m, some with a block on top; rubble at the foot | blocks |
| Forest ring | Terrain rising outside the clearing edge, up to about 3.5 m; trees merged into one prop mesh | ≈ 5.5k tris terrain |
| Tree | 5-sided trunk, 2–3 stacked 7-sided cones | ≤ 60 tris |
| Rock | Unsubdivided icosahedron, stretched unevenly | 20 tris |
| Fighter | One silhouette per class, readable from above, ≤ 2 m tall: Shade slim and hooded, Warden broad with a shoulder bar, Javelinist lean and hooded with an upright javelin rising above the head (the one thing taller than 2 m: a long, thin sniper silhouette), Ranger lean with a quiver | ≤ 400 tris |
| Fighter ring | Flat ring in the owner's color under the feet, so fighters read even in deep shade | 40 tris |
| Melee swing | Flat translucent fan in the owner's color, showing the real reach and arc, for 0.16 s | 12 tris |
| Windup telegraph | The attack's real shape (swing fan, or a strip down the shot's lane) in the owner's color, faint, while the attack winds up | 12 / 2 tris |
| Cast bar | Pale sunlit fill in a thin ink frame, hanging under the health bar while winding up | UI |
| Projectile | Glowing icosahedron sized to the class's projectile radius; the Javelinist's is a long javelin pointing the way it flies | 20 / ~30 tris |

- Anything that blocks movement or shots must sit on a blocking tile in `map.rs`; decoration on
  walkable tiles stays below knee height (flowers, rubble, stepping stones).
- Vary color between faces, never within one. On the fight floor, vary it per tile so a tile
  never looks split. No gradients, outlines or smooth shading.
- No pure white or black in the world. The one exception is the hit flash: a fighter turns
  white for 0.12 s when damage lands.
- UI (HUD, health bars, the join screen) sits on dark `Ink` panels with light `Haze` text.
