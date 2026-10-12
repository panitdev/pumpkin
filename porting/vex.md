# Port log: Vex (vanilla 26.3)

- Vanilla reference: `world.entity.monster.Vex` and its inner classes `VexChargeAttackGoal`,
  `VexRandomMoveGoal`, `VexCopyOwnerTargetGoal`, `VexMoveControl`; summon in
  `world.entity.monster.illager.Evoker.EvokerSummonSpellGoal`
- Beads: Pumpkin-x7o.26 (Vex, partial)

## Files touched

| File | Change |
|---|---|
| `crates/pumpkin-core/src/entity/mob/vex.rs` | goals, flying move control, owner / bound origin / limited life, charging flag, block-effect override, NBT |
| `crates/pumpkin-core/src/entity/ai/control/vex_move_control.rs` | new `VexMoveControl` |
| `crates/pumpkin-core/src/entity/ai/control/mod.rs` | register the move control module |
| `crates/pumpkin-core/src/entity/mob/evoker.rs` | summon sets owner, bound origin and limited life |
| `crates/pumpkin-core/src/entity/mob/equipment.rs` | `vex` registry entry (iron sword, 0 drop) and `WeaponConfig::AlwaysNoDrop` |
| `crates/pumpkin-core/src/entity/mod.rs` | overridable `EntityBase::is_affected_by_blocks`; `noPhysics` still gates suffocation |
| `crates/pumpkin-core/src/entity/living.rs` | `stepOn` uses the overridable block-effect check |
| `crates/pumpkin-core/src/entity/mob/mod.rs` | `Mob::mob_is_affected_by_blocks` hook |

## What already matched

- Goal and target priorities: float 0, charge 4, random move 8; hurt-by-target
  (ignoring raiders, alerting others) 1, copy-owner 2, nearest player 3.
- `isFlapping` / XP+attributes / sounds come from generated entity data.

## Gaps found and fixed

1. **Wrong movement goals.** Pumpkin used `MeleeAttackGoal` plus `WanderAroundGoal`. Vanilla uses
   `VexChargeAttackGoal` (charge the target's eye, hurt on hitbox overlap, clear charging) and
   `VexRandomMoveGoal` (pick an air block within ±7/±5/±7 of the bound origin, speed 0.25).
   Added both and removed the melee/wander goals.
2. **Ground movement, gravity.** The vex was a ground mob. Added `VexMoveControl` (delta-movement
   flight, yaw from target or movement, stop within its own bounding-box size), zero mob gravity
   and `setNoGravity`, plus `noPhysics` around its own tick.
3. **No owner support.** Added an owner UUID (`get_owner_uuid` override), NBT `owner`, and
   `VexCopyOwnerTargetGoal` that copies the evoker's target with non-combat, no-line-of-sight
   conditions.
4. **No bound origin / limited life.** Added bound origin (block-offset base for random moves) and
   the limited-life drain (1 starve damage every 20 ticks after `lifeTicks`), persisted as
   `bound_pos` / `life_ticks`.
5. **No charging flag.** Added `DATA_FLAGS_ID` (bit 1) with a synced setter/getter.
6. **Evoker summon didn't set these.** The summon now spawns at a block bottom-center, fixes the
   offset roll (`nextInt(5) - 2`, previously `random % 5 - 2`, which could reach -6) and sets
   owner, bound origin and `20 * (30 + nextInt(90))` limited life.
7. **No default sword.** Vanilla `Vex.populateDefaultEquipmentSlots` puts an iron sword in the
   main hand with a 0 drop chance; its `ATTACK_DAMAGE` modifier brings melee from 4 to 7. Added a
   `vex` equipment-registry entry using a new `WeaponConfig::AlwaysNoDrop`, so summoned and other
   spawned vexes equip it through the normal spawn path; the item modifiers are applied when the
   equipment change is sent.
8. **Block effects skipped while flying.** `no_physics` around the tick made `isAffectedByBlocks`
   false, skipping powder snow freezing, `stepOn` and `entityInside`. Vanilla overrides
   `Vex.isAffectedByBlocks` to `!isRemoved()`. Added the `Mob::mob_is_affected_by_blocks` hook
   (default unchanged) and the vex override; suffocation still respects `no_physics` like vanilla
   `Entity.isInWall`.
9. **Look-at goals.** Vanilla has `LookAtPlayerGoal(player, 3.0, 1.0)` at 9 and
   `LookAtPlayerGoal(Mob.class, 8.0)` at 10. Goal 9 is now an exact `LookAtEntityGoal(player, 3.0,
   1.0)`; goal 10 keeps the range and 2% chance but targets players, see below.

## Still open

- Goal 10 `LookAtPlayerGoal(Mob.class, 8.0)` is approximated by `LookAtEntityGoal(player, 8.0)`:
  Pumpkin's look-at goal filters by `EntityType` and has no whole-`Mob`-class target.
- `isFlapping` / `FLAP_DEGREES_PER_TICK` / `TICKS_PER_FLAP`: no flap hook for mobs in Pumpkin.
- `getLightLevelDependentMagicValue() = 1.0` override: no per-entity brightness hook.
- `restoreFrom` owner copy: no entity-conversion hook.
- Not tested in game.
