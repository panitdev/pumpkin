# Port log: Pufferfish and Tropical Fish (vanilla 26.3)

- Vanilla reference: `animal.fish.Pufferfish`, `animal.fish.TropicalFish`,
  `animal.fish.AbstractFish`, `animal.fish.AbstractSchoolingFish`, `Pufferfish.PufferfishPuffGoal`
  (inner), `ai.goal.RandomSwimmingGoal`, `ai.goal.FollowFlockLeaderGoal`.
- Beads: Pumpkin-x7o.63 (Pufferfish), Pumpkin-x7o.72 (Tropical Fish).

## Files touched

| File | Change |
|---|---|
| `entity/passive/fish.rs` | new `apply_scaled_dimensions` helper, used by Pufferfish and Salmon |
| `entity/passive/pufferfish.rs` | rewritten onto the shared fish code: water navigation, puff state machine, `PufferfishPuffGoal`, touch/playerTouch damage + poison, scaled dimensions, NBT |
| `entity/passive/tropical_fish.rs` | rewritten onto the shared fish code: schooling, `Pattern`/`Variant` packing, synced `DATA_ID_TYPE_VARIANT`, common variants on spawn, NBT |
| `entity/passive/salmon.rs` | `apply_dimensions` now calls the shared helper |

Both mobs now register through `fish::register_fish` (Tropical Fish also
`fish::register_schooling_goals`), so the old ground navigator, `SwimGoal`, `WanderAroundGoal`,
`LookAtEntityGoal` and `RandomLookAroundGoal` were dropped in favour of the vanilla goal set:

| Priority | Goal |
|---|---|
| 0 | `PanicGoal(1.25)` -> `EscapeDangerGoal` |
| 1 | `PufferfishPuffGoal` (Pufferfish only) |
| 2 | `AvoidEntityGoal(Player, 8.0, 1.6, 1.4)` |
| 4 | `FishSwimGoal` -> `RandomSwimmingGoal(1.0, 40)` |
| 5 | `FollowFlockLeaderGoal` (Tropical Fish only) |

## Pufferfish

1. **Puff state machine** (`Pufferfish.tick`): `inflateCounter`/`deflateTimer` in atomics, `PUFF_STATE`
   synced as a `VarInt`. Small -> mid on the first inflating tick, mid -> full past 40; full -> mid
   past 60 and mid -> small past 100 while deflating, each transition sounding
   `blow_up`/`blow_out`.
2. **`PufferfishPuffGoal`.** Priority 1, inflates while a scary `LivingEntity` is within 2.0 blocks.
   `start` sets `inflateCounter = 1` and clears `deflateTimer`, `stop` clears `inflateCounter`.
3. **Scary selector** (`SCARY_MOB` + `TARGETING_CONDITIONS.forNonCombat().ignoreInvisibilityTesting().ignoreLineOfSight()`):
   creative players are ignored, everything tagged `not_scary_for_pufferfish` is ignored, and line of
   sight and invisibility are not checked. The combat/faction part of `TARGETING_CONDITIONS` has no
   Pumpkin equivalent and is not tested.
4. **Touch damage** (`Pufferfish.touch`): mobs in the 0.3 box take `1 + puffState` `MOB_ATTACK`
   damage, get poison 60 * puffState, and hear `PUFFER_FISH_STING`, gated on `doTeamsAllowDamage`.
5. **Player touch** (`Pufferfish.playerTouch`): a colliding player with a nonzero puff state takes the
   same damage, gets the poison, and gets the `PUFFER_FISH_STING` game event when the fish is not
   silent.
6. **Dimensions**: `getDefaultDimensions` scale 0.5 / 0.7 / 1.0, applied on construction and whenever
   the puff state changes (mirrors `onSyncedDataUpdated`). NBT key `PuffState`, clamped to 2.
7. **`doTeamsAllowDamage`** ported as a local helper (`Entity.doTeamsAllowDamage`): no own team ->
   allow, otherwise friendly fire is `(team.options & 1) != 0` and only applies to the same team.

## Tropical Fish

1. **Schooling** via `AbstractSchoolingFish` (`FishState`, `FollowFlockLeaderGoal`, the school
   recount in `fish_tick`), max school size 8.
2. **Packed variant.** `Pattern` (12 values, `base | index << 8`), `DyeColor` base/pattern colors,
   packed as `pattern | base << 16 | patternColor << 24`, stored in `DATA_ID_TYPE_VARIANT` and
   decoded with the vanilla sparse/KOB and white-fallback rules. Unknown pattern ids fall back to
   `KOB`; out-of-range colors fall back to white (`ByIdMap.OutOfRangeStrategy.ZERO`, which is *not*
   Pumpkin's `DyeColor::default()` = black).
3. **Common variants on spawn** (`finalizeSpawn`): 90% pick from `COMMON_VARIANTS`, 10% roll a random
   pattern and two random colors.
4. **NBT** key `Variant` as the packed int (26.3 changed this from the old base/pattern/color
   compound).
5. **Flop sound** `TROPICAL_FISH_FLOP`; hurt/death sounds already come from generated entity data.

## Still open

- **Shared spawn-group variant and `isSchool`.** Vanilla coordinates each fish's variant (and
  whether the group can grow past its size) through `TropicalFishGroupData extends
  SchoolSpawnGroupData` in `finalizeSpawn`.   Pumpkin's `SpawnGroupData` only carries spider effects, so
  every fish rolls its own variant and `TropicalFish.isMaxGroupSizeReached`/`isSchool` is not
  modelled. (The spawner's tropical-fish cap of 8 is already hardcoded in `natural_spawner`.)
- **Tropical fish spawn groups don't share a variant.** Vanilla packs the variant into
  `TropicalFishGroupData` so a whole group matches; Pumpkin's `SpawnGroupData` has no equivalent,
  so each fish rolls its own variant on spawn.
- **Initial school spawn** is the same `SchoolSpawnGroupData` limitation Cod/Salmon already note;
  schools still form at runtime.
- **Ambient sounds.** No generic ambient-sound hook exists for any fish (Cod/Salmon have the same
  gap), so `TROPICAL_FISH_AMBIENT` is not played.
- **`TARGETING_CONDITIONS` forCombat/faction checks.** Pufferfish reuses only the `SCARY_MOB` selector;
  `forNonCombat` (attackable/targeting) and `Bukkit`-style combat checks have no Pumpkin equivalent.
- **Bucket variant data.** Bucket pickup/placement works generically, but the fish bucket item carries
  no entity NBT, so the tropical pattern/colors and pufferfish puff state are not restored on
  placement. Affects every bucketable mob; out of scope here.
- **Fish move/navigation overlap** as noted in `fish.md`; unchanged.
- Boot/behaviour not run with a client; see the report for the in-game checklist.

## Separate observations

- `GameEvent::PlayPufferfishStringSound` in `pumpkin-protocol` is misspelled (vanilla
  `PUFFER_FISH_STING`); kept as-is to stay in scope, used for the player sting event.
