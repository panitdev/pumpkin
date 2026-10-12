# Port log: Phantom (vanilla 26.3)

- Pumpkin base: `sweep/phantom` worktree, `CURRENT_MC_VERSION = 26.3`
- Vanilla reference: 26.3 server jar, class `world.entity.monster.Phantom` (+ inner
  `PhantomAttackStrategyGoal`, `PhantomSweepAttackGoal`, `PhantomCircleAroundAnchorGoal`,
  `PhantomAttackPlayerTargetGoal`, `PhantomMoveTargetGoal`, `PhantomMoveControl`,
  `PhantomLookControl`, `PhantomBodyRotationControl`, `AttackPhase`)
- Setup steps: see [README.md](README.md).

## Files touched

| File | Change |
|---|---|
| `crates/pumpkin-core/src/entity/mob/phantom.rs` | state (anchor, move target, phase, size), goals, controls, NBT, `finalizeSpawn`, size/damage/dimensions |
| `crates/pumpkin-core/src/entity/ai/goal/phantom_attack_strategy.rs` | `PhantomAttackStrategyGoal` |
| `crates/pumpkin-core/src/entity/ai/goal/phantom_sweep_attack.rs` | `PhantomSweepAttackGoal` + cat avoidance |
| `crates/pumpkin-core/src/entity/ai/goal/phantom_circle_around_anchor.rs` | `PhantomCircleAroundAnchorGoal` |
| `crates/pumpkin-core/src/entity/ai/goal/phantom_attack_player_target.rs` | `PhantomAttackPlayerTargetGoal` |
| `crates/pumpkin-core/src/entity/ai/goal/mod.rs` | register the four goals |
| `crates/pumpkin-core/src/entity/ai/control/phantom_move_control.rs` | `PhantomMoveControl` |
| `crates/pumpkin-core/src/entity/ai/control/mod.rs` | register the control |
| `crates/pumpkin-core/src/entity/ai/control/look_control.rs` | `disabled` flag for the no-op phantom look control |
| `crates/pumpkin-core/src/entity/passive/cat.rs` | `CatEntity::hiss` (used by the sweep goal) |

## What the phantom now does

- Goal selector: attack strategy (1), sweep (2), circle-around-anchor (3); target selector:
  attack-player (1). The old look goals were removed; vanilla registers none.
- `PhantomAttackPlayerTargetGoal`: every 20 (then 60) reduced ticks, scans players in the
  phantom's bounding box inflated by (16, 64, 16), sorts by Y descending, and targets the first
  one that passes combat targeting.
- `PhantomAttackStrategyGoal`: while circling, counts down then flips to the SWOOP phase, moves
  the anchor 20-39 blocks above the target (never below sea level) and plays `PHANTOM_SWOOP`.
  On stop, the anchor is moved to the first available block above the motion-blocking heightmap
  (`getHeightmapPos().getY()`, i.e. heightmap + 1) + 10-29.
- `PhantomCircleAroundAnchorGoal`: orbits the anchor with a randomized distance (5-15),
  height (-4 to +5) and direction, re-rolling on the vanilla timers, nudging the height up/down
  when the orbit point is blocked below/above, and resetting the point when it is reached.
- `PhantomSweepAttackGoal`: dives at the target's mid-body, hits it when overlapping (with the
  phantom bite world event 1039 when not silent), then returns to the circle phase. While
  swooping it looks for alive cats within 16 blocks, makes them hiss, and aborts the swoop if
  any are nearby. Also aborts on horizontal collision or a recent hurt.
- `PhantomMoveControl`: steers toward `move_target_point` by writing yaw/body-yaw/head-yaw,
  pitch and velocity directly (vanilla math), reversing 180° on horizontal collision.
- Size: `ID_SIZE` is synced; `setPhantomSize` clamps 0-64 and, only when the value actually
  changes (mirroring `onSyncedDataUpdated(ID_SIZE)`, which `SynchedEntityData.set` skips when
  unchanged), scales the hitbox/eye height by `1 + 0.15 * size` and sets attack damage to
  `6 + size`. A size-0 phantom keeps its registry attack damage. `finalizeSpawn` sets the anchor
  five blocks above and size 0; NBT stores `anchor_pos` (int array) and `size`.
- NBT: `anchor_pos` as an `[I; x y z]` int array (vanilla `BlockPos.CODEC`) and `size`.
- Movement: gravity 0 (as before) plus a Y drag of 0.91 so `travel_in_air` scales the whole
  movement vector like vanilla `travelFlying(input, 0.2)`.

## What already matched / came from generated data

- Sun burning: the phantom is in the `minecraft:burn_in_daylight` entity tag, so the shared
  `sun_burn::tick` already lights it up under open sky.
- No fall damage: the phantom is in `minecraft:fall_damage_immune`.
- XP reward 5, hurt/death sounds, attributes, tracking range all come from generated data.

## Not ported / still open

- **Client-only flap and mycelium particles.** Vanilla `Phantom.tick` plays `PHANTOM_FLAP` and
  spawns `MYCELIUM` particles only when `level.isClientSide()`, so the server never runs them.
  Nothing to hook server-side.
- **Ambient sound.** Vanilla `getAmbientSound` returns `PHANTOM_AMBIENT`, but Pumpkin has no
  generic mob ambient-sound timer (only the bat implements one by hand). Left out rather than
  adding a shared hook in this change.
- **`PhantomBodyRotationControl`.** Vanilla swaps in a body control whose `clientTick` sets
  `yHeadRot = yBodyRot; yBodyRot = getYRot()`. Pumpkin has no `tickHeadTurn`/body-control hook
  (`BodyRotationControl` is unused), so the move control sets yaw, body yaw and head yaw
  directly instead. Same net rotation for the phantom.
- **Water/lava travel.** Vanilla `travelFlying` has water/lava speed branches; Pumpkin routes a
  phantom touching fluid through the generic `travel_in_fluid`. Only matters if a phantom is
  floating in fluid, which its movement goals avoid.
- **Natural spawning.** No phantom spawner work; the issue only asked for behavior.

## Checks

- `cargo fmt --check`, `cargo clippy -p pumpkin-core --all-targets --all-features` (clean),
  `cargo check --workspace --all-targets` (clean).
- No unit test added: the behavior is movement/AI that needs a live world; verified by build +
  clippy + a boot-review of the goal wiring. Play test under "do X, expect Y" is in the issue
  handoff.
