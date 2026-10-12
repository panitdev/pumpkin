# Port log: Strider (vanilla 26.3)

- Vanilla reference: `monster.Strider` (+ inner `StriderGoToLavaGoal`, `StriderPathNavigation`),
  `ai.goal.MoveToBlockGoal`, `ai.goal.PanicGoal`, `ai.goal.TemptGoal`, `ai.goal.RandomStrollGoal`
- Beads: Pumpkin-x7o.69.

## Files touched

| File | Change |
|---|---|
| `entity/passive/strider.rs` | pathfinding malus, corrected goal list, `StriderGoToLavaGoal`, suffocation tick + speed modifier, `floatStrider`, walk-target value, tick sounds |
| `entity/ai/goal/move_to_target_pos.rs` | `MoveToTargetPos::get_move_to_target` and `should_recalculate_path` overrides (vanilla `getMoveToTarget` / `shouldRecalculatePath`) |
| `entity/ai/goal/goal_selector.rs` | `is_goal_running::<G>()`, so the strider can ask whether its tempt/panic goal is running |

## What already matched

- Spawn rule (`checkStriderSpawnRules`) in `entity/type.rs`, item tag food, saddle NBT, mount/interact,
  `ItemSteerable::boost` (warped fungus on a stick), base movement speed 0.175 from generated data.
- `WalkNodeEvaluator` is the navigator default, matching `StriderPathNavigation`'s evaluator.

## Gaps found and fixed

1. No pathfinding malus. Vanilla sets water `-1`, lava `0`, fire (`FIRE_IN_NEIGHBOR`, `FIRE`) `0`.
   Same mapping as the blaze (`Water`/`Lava`/`DangerFire`/`DamageFire`), now set in `new()`.
2. Goal list diverged from vanilla: an extra `SwimGoal(0)` (vanilla `Strider.registerGoals` never
   calls `super`, so there is no `FloatGoal`), `FollowParentGoal` was 1.1 (vanilla 1.0),
   `RandomStrollGoal` interval was 120 (vanilla 60), the player look goal was 6.0 (vanilla 8.0),
   the strider look goal was missing. Rewrote the list to vanilla priorities 1/2/3/4/5/7/8/8/9.
3. No `StriderGoToLavaGoal`. Added it at priority 4 as a `MoveToTargetPosGoal` wrapper: only runs
   while not in lava, targets a lava block whose block above is pathfindable (`LAVA` + `isPathfindable(LAND)`),
   moves to the lava block itself (`get_move_to_target`) and repaths every 20 ticks.
4. Suffocation state was stored but never computed. Added vanilla `Strider.tick`'s check: suffocating
   when not in/on a `#strider_warm_blocks` block, no lava height and not riding a non-suffocating
   strider. `set_suffocating` now also applies the vanilla `suffocating` modifier
   (`ADD_MULTIPLIED_BASE -0.34` on `MOVEMENT_SPEED`) and syncs it, only on change.
5. No lava floating. Added `floatStrider` in `post_tick` (after movement, matching `super.tick()`
   then `floatStrider()`): stands on the lava surface, otherwise halves velocity and bobs up 0.05.
6. No walk-target preference. Added `get_walk_target_value`: lava is 10, otherwise
   `NEGATIVE_INFINITY` while in lava, else 0, so strolls prefer lava pools.
7. No tick sounds. Added the tempted-happy (1/140) and panicking-retreat (1/60) sounds, using a new
   `GoalSelector::is_goal_running` to answer `isBeingTempted`/`isPanicking`.

## Still open

- `canStandOnFluid(LAVA)` and `getLiquidCollisionShape` are approximated: `LivingEntity::tick_movement`
  skips `travel_in_fluid` for `EntityType::STRIDER` and `floatStrider` bobs it; a general
  per-entity fluid-standing rule does not exist yet (see the TODO in `strider.rs`).
- `StriderPathNavigation.isStableDestination` (lava counts as a stable path target) has no
  navigator hook.
- Ridden movement: Pumpkin has no controlled-vehicle engine, so `getRiddenInput` (always forward),
  `getRiddenSpeed` (`MOVEMENT_SPEED * (suffocating ? 0.35 : 0.55) * boostFactor`), `tickRidden`,
  `getControllingPassenger` (player holding warped fungus on a stick) and
  `getDismountLocationForPassenger` are not ported. `DATA_BOOST_TIME` is not synced.
- `nextStep` (+0.6), `playStepSound` (lava vs normal), `checkFallDamage` (reset fall distance in
  lava; the engine only halves it), `canAddPassenger` (blocked while eye is in lava),
  `isSensitiveToWater` (water/rain damage), `isOnFire` false, `getAmbientSound` null while
  tempted/panicking, `getLeashOffset` and the baby `getDefaultDimensions` are not ported (no hooks).
- `finalizeSpawn` jockey rolls (zombified piglin with saddle + warped fungus on a stick, or baby
  strider jockey) are not ported.
- Not tested in game.
