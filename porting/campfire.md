# Port log: Campfire (vanilla 26.3)

- Tracker issue: `Pumpkin-747.3`; vanilla: `CampfireBlock`, `CampfireBlockEntity`
- Files: `crates/pumpkin-core/src/block/blocks/campfire.rs`, `crates/pumpkin-core/src/item/items/bucket.rs`

## Already matched
- Placement state (waterlogged / lit / signal fire / facing), signal fire update from the block below.
- Cooking (4 slots, recipe time, cool-down at 2x when unlit), placing food, item scatter on break.
- Shovel dousing, igniters (flint and steel / fire charge), soul campfire damage 2.

## Fixed
- `onProjectileHit`: a burning projectile relights an unlit, non-waterlogged campfire.
- Entity damage no longer exempts Frost Walker boots (not in vanilla 26.3); fire resistance is
  already handled by the `is_fire` damage pipeline.
- Water bucket placed into a campfire now extinguishes it (state previously stayed lit until a
  neighbour update).

## Open
- `mayInteract` check (mob griefing / owner) for the projectile relight is not modelled.
- Smoke particles and crackle sounds are client-side in vanilla; hay-bale signal smoke is only the
  `signal_fire` state flag. Flowing water extinguishing is via the existing neighbour update path.
