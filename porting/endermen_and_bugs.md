# Port log: Enderman, Endermite, Silverfish (vanilla 26.3)

- Vanilla reference: `monster.Enderman` (+ inner goals), `monster.Endermite`, `monster.Silverfish`
  (+ `SilverfishMergeWithStoneGoal`, `SilverfishWakeUpFriendsGoal`),
  `ai.goal.ClimbOnTopOfPowderSnowGoal`, `LivingEntity.randomTeleport`, `InfestedBlock`
- Beads: Pumpkin-x7o.9 (Enderman), Pumpkin-x7o.10 (Endermite), Pumpkin-x7o.21 (Silverfish)

## Enderman

### What already matched

- `ChasePlayerGoal` is vanilla `EndermanFreezeWhenLookedAt` (priority 1, JUMP and MOVE, 256 squared
  distance, stops navigation, looks at the target). The tracker line asking for a freeze goal is
  satisfied by it.
- `ResetUniversalAngerGoal::new(false)` is registered on the target selector at priority 4.
- Block pick up and place goals, projectile dodge, speed modifier on target change.

### Gaps found and fixed

| File | Change |
|---|---|
| `entity/mob/enderman.rs` | `teleport_to` follows `randomTeleport`: walks down from the target to the first `entities_can_teleport_to` block, rejects `enderman_does_not_teleport_to` blocks (floor and any block inside the box), no collision, no liquid, no water below. Passengers never teleport. Plugin event fires before the particles and sounds. |
| `entity/mob/enderman.rs` | Daylight avoidance in `custom_server_ai_step`: bright outside, 600 ticks after the last target change, brightness above 0.5, sky visible, random roll `(brightness - 0.4) * 2 / 30`. Drops the target and teleports. |
| `entity/mob/enderman.rs` | `drop_custom_death_loot` drops the carried block's loot table with a diamond axe enchanted from the `enderman_loot_drop` provider (silk touch). |
| `entity/mob/mod.rs`, `entity/mod.rs`, `entity/living.rs` | New `drop_custom_death_loot` hook (vanilla `dropCustomDeathLoot`), called from `LivingEntity::on_death` right after the loot table. |

### Still open

- `randomTeleport` clamps the target to the world border and requires a loaded chunk; Pumpkin does neither.
- Teleport game event, level event 2018 and the entity event 46 are replaced by the old portal particle and sound pair.
- `EndermanLookForPlayerGoal` is `TeleportTowardsPlayerGoal` and was not compared again.
- Persistent anger NBT, `requiresCustomPersistence` for a carried block, thrown potion handling.

## Endermite

- Added `ClimbOnTopOfPowderSnowGoal` (new `entity/ai/goal/climb_on_top_of_powder_snow.rs`, priority 1).
  Runs while in or just out of powder snow for mobs in `powder_snow_walkable_mobs` when the block
  above is powder snow or has no collision shape, and holds the jump flag every tick.
- Open: goal priorities differ from vanilla (melee 4 instead of 2, no alert-others revenge target).

## Silverfish

- Added `ClimbOnTopOfPowderSnowGoal`, `WakeUpFriendsGoal` and `MergeWithStoneGoal` in `entity/mob/silverfish.rs`.
- Goal set now matches vanilla: float and climb at 1, wake up friends 3, melee 4, merge 5. The look
  goals were dropped because vanilla silverfish has none.
- Wake up: hurt by any entity or by a `always_triggers_silverfish` damage type starts a 20 tick
  timer. Then the 21x11x21 box is scanned in vanilla order; infested blocks are destroyed without a
  break event (mob griefing on: loot, then the block's `spawn_after_break` hook spawns the silverfish) or reverted to the host block.
  Each hit has a 50% chance to end the scan.
- Merge: random stroll with interval 10 that, with mob griefing, 1 in 10 per check picks a random
  neighbouring block and turns a host block into its infested twin, then the silverfish vanishes.
- Infested block mapping: `pumpkin-data` has no host to infested table, so the twin is looked up
  by the `infested_` name prefix and block properties are copied (deepslate axis).

### Still open

- `getWalkTargetValue` preference for host blocks, `checkSilverfishSpawnRules`, and the yBodyRot override are untouched.
- Only `block/blocks/infested.rs` (not touched here) decides what a player breaking an infested block does.
