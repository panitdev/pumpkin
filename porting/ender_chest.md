# Port log: Ender Chest (vanilla 26.3)

- Tracker issue: `Pumpkin-747.8`
- Vanilla reference: `EnderChestBlock`, `EnderChestBlockEntity`, `ContainerOpenersCounter`, `ChestLidController`
- Files: `crates/pumpkin-core/src/block/blocks/ender_chest.rs`, `crates/pumpkin-core/src/entity/mob/piglin_ai.rs`

## Matched already

- Open/close counting, lid block event (`LID_ANIMATION_EVENT_TYPE`, payload = viewer count), open/close
  sounds with the vanilla volume/pitch, and the `OPEN_ENDERCHEST` stat.
- Player ender chest inventory, blocked opening when a conductive block sits above (the shape check the
  26.3 class uses is `isRedstoneConductor`, which Pumpkin's solid-block-above test matches).
- `isPathfindable = false`.

## Fixed

1. Removed the stale "cat on top blocks opening" TODO. 26.3 `EnderChestBlock.useWithoutItem` only checks
   a conductive block above; the cat check belongs to normal chests, not the ender chest.
2. Piglin anger: added `PiglinAi::anger_nearby_piglins`, called from `normal_use` (`angerNearbyPiglins(level, player, true)`).
   It turns idle piglins within 16 blocks that can see the player against them by setting their attack target.

## Open

- Vanilla `setAngerTarget` writes the brain `ANGRY_AT` memory with a 600 tick expiry; Pumpkin piglins have
  no brain-driven anger, so the closest available behaviour is `Mob::set_mob_target` (no timer).
- `canSee` is `BehaviorUtils.canSee`, which reads the brain's nearest-visible-living-entities memory.
  Approximated with `Entity::has_line_of_sight` because Pumpkin has no such memory.
- `ChestLidController` animation is client-side in Pumpkin; only the block event is sent.
- Not tested in game.
