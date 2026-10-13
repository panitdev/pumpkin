# Port log: Turtle (vanilla 26.3)

- Vanilla reference: `world.entity.animal.turtle.Turtle` (inner classes `TurtleLayEggGoal`,
  `TurtleGoHomeGoal`, `TurtleTravelGoal`, `TurtleGoToWaterGoal`, `TurtleRandomStrollGoal`,
  `TurtlePanicGoal`, `TurtleBreedGoal`, `TurtleMoveControl`, `TurtlePathNavigation`),
  `MoveToBlockGoal`, `DefaultRandomPos`, `AmphibiousPathNavigation`.
- Before this change the turtle only had breed, tempt, try-find-water, panic, stroll and look
  goals. It never laid eggs, never went home and never travelled.

## Files touched

| File | Change |
|---|---|
| `entity/passive/turtle.rs` | `home_pos`/`going_home`/`travel_pos`/`lay_egg_counter` state, NBT, `finalize_spawn`, amphibious navigation, `get_walk_target_value`, `is_stable_destination`, `is_pushed_by_fluids`, `travel_in_water`, `TurtleMoveControl`, `TurtleBreedGoal`, `TurtleLayEggGoal`, `TurtleGoHomeGoal`, `TurtleTravelGoal`, `TurtleGoToWaterGoal`, `TurtlePanicGoal`, `TurtleRandomStrollGoal` |
| `entity/mob/mod.rs` | `Mob::is_stable_destination` hook (default `!blockState(pos.below()).is_solid()`, the previous `GoalUtils.isNotStable` behaviour) |
| `entity/ai/util/goal_utils.rs` | `is_not_stable` now delegates to `Mob::is_stable_destination` |
| `entity/ai/util/default_random_pos.rs`, `.../land_random_pos.rs` | pass the mob to `is_not_stable` |
| `entity/ai/goal/breed.rs` | `BreedGoal::with_breed_action`, a `fn` pointer that swaps the vanilla-overridable `breed` |
| `entity/passive/animal.rs` | `Animal::can_fall_in_love` (default `!isInLove`), used when feeding |
| `entity/ai/goal/escape_danger.rs` | expose `should_panic`/`look_for_water`/`find_random_position`/`set_target` for `TurtlePanicGoal` (`look_for_water` now takes the search range) |
| `block/blocks/turtle_egg.rs` | hatchlings get `home_pos` set to the egg block on hatch (`TurtleEggBlock.randomTick`) |

## Fixed

- `registerGoals` mirrors vanilla: panic 0, breed 1, lay-egg 1, tempt 2, go-to-water 3, go-home 4,
  travel 7, look-at-player 8, stroll 9. The old `SwimGoal` (float) and `RandomLookAroundGoal` are
  dropped because vanilla `Turtle` has neither.
- `TurtleGoToWaterGoal` is a `MoveToTargetPosGoal` (Pumpkin's `MoveToBlockGoal`) with search range
  24, `verticalSearchStart = -1` (`lowest_y`), give-up at 1200 ticks and a 160-tick path
  recalculation. Babies always seek water; adults only when not going home and not carrying an egg.
  The water block is the target and the block above it the move-to point.
- `TurtlePanicGoal` wraps `EscapeDangerGoal` (`PanicGoal`): on a panic it always searches water
  within 7 blocks (the base only does that when on fire, within 5) and falls back to a random
  position. `EscapeDangerGoal`'s water search, random-position pick and panic check are now shared.
- `TurtleRandomStrollGoal` wraps `WanderAroundGoal` and gates the stroll on
  `!inWater && !goingHome && !hasEgg`, so it cannot overwrite the go-home or travel navigation.
- `Turtle.travelInWater` is ported: `moveRelative(0.1)`, damp by 0.9, and the near-home downward
  drift while not targeting an entity. `TurtleMoveControl` applies `updateSpeed` (water buoyancy,
  half speed beyond 16 blocks from home, slower babies, half speed on the ground) and drives the
  movement input from the next path node, because Pumpkin's amphibious navigator writes the input
  itself and never hands a wanted position to the move control.
- `TurtleBreedGoal` wraps `BreedGoal` with the `!hasEgg` gate and a `breed` action that sets
  `has_egg` on the mother, sets a 6000-tick cooldown on both parents, resets their love and awards
  the breeding stat/advancements, instead of spawning a child. `Turtle.canFallInLove` also refuses
  food while an egg is carried, so feeding no longer starts a second breeding.
- `TurtleLayEggGoal` reuses `MoveToTargetPosGoal` (Pumpkin's `MoveToBlockGoal`). It walks to sand
  with an air block above, within 16 blocks and no more than 9 blocks from home, then after
  200 ticks places a `turtle_egg` with 1-4 eggs above the target, clears `has_egg`, resets love and
  plays the lay-egg sound at the block centre. `Turtle.aiStep`'s every-5-tick sand dust runs from
  `mob_tick`.
- Turtle eggs set each hatchling's `home_pos` to the egg block on hatch, matching
  `TurtleEggBlock.randomTick`, so `TurtleGoHomeGoal`/`TurtleLayEggGoal` no longer path to origin.
- `TurtleGoHomeGoal` sets `goingHome` while it runs, gives up after 600 ticks or when stuck, and
  re-rolls a `DefaultRandomPos.getPosTowards` target (retrying via water when it is not already
  close to home). `TurtleTravelGoal` picks a random water target within 512/4/512 blocks below sea
  level and abandons it when the target chunks are not loaded.
- `home_pos` is saved/read as the vanilla int-array tag and set to the block position on
  `finalizeSpawn`; `lay_egg_counter` follows `setLayingEgg` (`1` on, `0` off). `has_egg` uses the
  vanilla key, and the old `HasEgg` key is still read so pre-port worlds keep their egg state.
- Navigation is `Navigator::amphibious(false)` (`TurtlePathNavigation`); `isStableDestination`
  returns only water while `travel_pos` is set, otherwise "not air below". Turtles also opt out of
  fluid pushing (`Turtle.isPushedByFluid`).

## Open / not ported

- `TurtleMoveControl` follows Pumpkin's navigator-driven movement rather than vanilla's
  wanted-position handoff, so its speed lerp is approximated by reading the next path node (the
  same adaptation `AbstractFish.FishMoveControl` uses).
- `getAmbientSoundInterval`, land/baby ambient sounds, `playSwimSound`/`getSwimSound`, `thunderHit`,
  baby dimensions and `getAgeScale` (render only) are not wired up.
- `ageBoundaryReached` dropping from the `TURTLE_GROW` loot table is a separate pre-existing gap.
