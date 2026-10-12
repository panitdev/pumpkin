# Port log: Cod and Salmon (vanilla 26.3)

- Vanilla reference: `animal.fish.AbstractFish`, `animal.fish.AbstractSchoolingFish`,
  `animal.fish.WaterAnimal`, `animal.fish.Cod`, `animal.fish.Salmon`,
  `ai.goal.FishSwimGoal` (inner), `ai.goal.RandomSwimmingGoal`, `ai.goal.FollowFlockLeaderGoal`,
  `ai.goal.PanicGoal`, `ai.goal.AvoidEntityGoal`, `AbstractFish.FishMoveControl`.
- Beads: Pumpkin-x7o.45 (Cod), Pumpkin-x7o.65 (Salmon).

## Files touched

| File | Change |
|---|---|
| `entity/passive/fish.rs` | new: `AbstractFish` / `AbstractSchoolingFish` traits, `FishState` (leader + school size), `register_fish`, `register_schooling_goals`, `fish_tick` (flop + school recount), `fish_travel_in_water` |
| `entity/passive/cod.rs` | rewritten: schooling fish, new goals, water navigation, flop, water travel |
| `entity/passive/salmon.rs` | rewritten: same as Cod plus `SalmonVariant` (weights, dimensions, NBT, synced `DATA_TYPE`) |
| `entity/ai/goal/random_swimming.rs` | new: `RandomSwimmingGoal` (`WanderAroundGoal::swimming` + `canRandomSwim`) |
| `entity/ai/goal/follow_flock_leader.rs` | new: `FollowFlockLeaderGoal` |
| `entity/ai/control/fish_move_control.rs` | new: `FishMoveControl` |
| `entity/ai/goal/wander_around.rs` | `swimming` positions via `BehaviorUtils.getRandomSwimmablePos` |
| `entity/ai/pathfinder/mod.rs` | `PathNavigationTrait::speed_modifier` getter (vanilla `MoveControl.getSpeedModifier`) |
| `entity/mob/mod.rs` | `Mob::as_schooling_fish` |
| `entity/passive/mod.rs` | `pub mod fish` |

## Goals (was: ground navigator, swim/wander/look goals)

Vanilla `AbstractFish.registerGoals` is only three goals and `AbstractSchoolingFish` adds one, so
the old set (Float, Stroll, LookAtPlayer, RandomLookAround) was removed:

| Priority | Goal |
|---|---|
| 0 | `PanicGoal(1.25)` -> `EscapeDangerGoal` |
| 2 | `AvoidEntityGoal(Player, 8.0, 1.6, 1.4)` -> `AvoidEntityGoal` |
| 4 | `FishSwimGoal` -> `RandomSwimmingGoal(1.0, 40)` |
| 5 | `FollowFlockLeaderGoal` (schooling only) |

## Gaps found and fixed

1. **Water-bound navigation.** `Navigator::water_bound(false)` (the guards already had one),
   mob dimensions, water malus 0 (`WaterAnimal` constructor) and `FishMoveControl`.
2. **`FishMoveControl`.** Eye-in-water buoyancy (+0.005 tick), lerp toward
   `speedModifier * MOVEMENT_SPEED`, a lift toward the next node's height, and a 90 degree/tick
   yaw turn. The navigator exposes its speed modifier now instead of assuming 1.0, so a panicking
   fish actually flees at 1.25.
3. **`travelInWater`.** `moveRelative(0.01)`, 0.9 drag, and a slow sink when there is no target.
4. **Flopping on land.** Out of water on the ground it hops with random X/Z, plays the cod/salmon
   flop sound and resyncs.
5. **Schooling.** Leader/`schoolSize` bookkeeping with the 8-block (cod 8, salmon 5) membership
   rules, `FollowFlockLeaderGoal` to find/keep a leader, the periodic school recount, and the
   `canRandomSwim` gate so a follower does not wander off.
6. **Salmon variant.** Small 30 / medium 50 / large 15, stored in synced `DATA_TYPE`, saved to
   NBT as `type`, and the type's box scaled by 0.5/1.0/1.5.
7. **Sounds.** Cod/salmon flop sounds are played by the flop; hurt and death sounds already come
   from generated entity data.

## Still open

- **Bucket entity data.** Pickup already works (`FilledBucketItem::use_on_entity` turns a water
  bucket into the matching fish bucket and removes the fish, with `item.bucket.fill_fish`), so the
  tracker's "Bucket pickup" stays done. But Pumpkin's bucket items carry no entity NBT for any mob,
  so the salmon size (vanilla `SALMON_SIZE` component) and `FromBucket` are not restored on
  placement. Left alone here because it affects every bucketable mob, not just fish.
- **Initial school spawn.** Vanilla links a spawn group through `SchoolSpawnGroupData` in
  `finalizeSpawn`; Pumpkin's `SpawnGroupData` only carries spider effects. Schools still form at
  runtime through `FollowFlockLeaderGoal`.
- **`AvoidEntityGoal` predicate.** Fish use `EntitySelector.NO_SPECTATORS` (creative players are
  avoided too); Pumpkin's goal hardcodes `except creative or spectator`. Reuses the shared goal.
- **Flop collision flag.** Vanilla checks `verticalCollision`; Pumpkin does not track it, so the
  flop uses `onGround()` instead. Behaviour is equivalent for a fish that just landed.
- **`WaterAnimal` remainders**: 120-tick ambient sound interval (no ambient hook), air supply 300
  with 2 drown damage out of water (Pumpkin only runs breath logic for players), base XP
  `1 + rand(3)`, `checkSpawnObstruction` = `isUnobstructed`, `isPushedByFluid false`,
  `canBeLeashed false`. None of these are wired yet for any water mob.
- **Swim sound** (`AbstractFish.getSwimSound` = `FISH_SWIM`): no generic swim sound hook exists,
  only the per-mob nautilus one.
- **Navigator/move-control overlap.** As with the guardian, the navigator writes yaw and movement
  input first and `FishMoveControl` overwrites them, so the navigator's yaw snap happens before the
  control's lerp. Only the control's value ends up on the wire.
- Boot/behaviour not run with a client; see the report for the in-game checklist.
