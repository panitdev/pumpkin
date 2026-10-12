# Port log: Illagers (vanilla 26.3)

Covers the Pillager, Vindicator and Illusioner sweep. Vanilla reference: 26.3 server jar,
`world.entity.monster.illager` (`AbstractIllager`, `SpellcasterIllager`, `Pillager`, `Vindicator`,
`Illusioner` and their inner goals), `world.entity.monster.PatrollingMonster`,
`world.entity.raid.Raider`, and `world.entity.ai.goal` (`AvoidEntityGoal`, `BreakDoorGoal`,
`OpenDoorGoal`, `RangedBowAttackGoal`).

## Files touched

| File | Change |
|---|---|
| `entity/mob/pillager.rs` | AvoidEntityGoal, RevengeGoal, full goal/target parity |
| `entity/mob/vindicator.rs` | Johnny flag/goal/NBT/name trigger, raid-gated break+open doors |
| `entity/mob/illusioner.rs` | spellcaster state, mirror/blindness spells, bow attack, AvoidEntityGoal |
| `entity/mob/spellcaster_illager.rs` | new; shared `SpellcasterIllager` state + casting/use-spell goals |
| `entity/mob/equipment.rs` | Illusioner spawns holding a bow |
| `entity/mob/mod.rs` | register `spellcaster_illager`, `Mob::on_custom_name_set` hook |
| `entity/ai/goal/active_target.rs` | `unseen_memory_ticks` builder |
| `item/items/name_tag.rs` | call the mob name hook so "Johnny" is detected |
| `world/mod.rs` | `World::is_raided` (`ServerLevel.isRaided`) |

## Pillager

- Added `AvoidEntityGoal` for `Creaking` (8.0, 1.0, 1.2) at priority 1, matching
  `Pillager.registerGoals`.
- Added the missing `HurtByTargetGoal(Raider).setAlertOthers()` (Pumpkin `RevengeGoal` ignoring
  `#minecraft:raiders`, also alerting same-type mobs) at target priority 1; shifted player,
  villager and iron golem targets to 2/3/3 and the villager target to no-line-of-sight
  (`false`), all as vanilla.
- Aligned the rest of `registerGoals` with vanilla now that the list was being touched: stroll
  at 8 speed 0.6, look-at-player at 9 range 15, random look at 10, path-to-raid at 3.

## Vindicator

- `VindicatorBreakDoorGoal`: wraps `BreakDoorGoal` with break time 6 and predicate
  `Normal || Hard` (vanilla `DOOR_BREAKING_PREDICATE`), only runs during an active raid and with
  the vanilla 1-in-5 roll (`random.nextInt(reducedTickDelay(10))`). The 6 is passed through to the
  base goal, but `getDoorBreakTime` returns `Math.max(240, 6)`, so the break still takes 240 ticks
  (same as a zombie); the 6 has no effect. `custom_server_ai_step` now sets the navigator
  `can_open_doors` from `level.isRaided(blockPosition())`, which is what lets the path route
  through the door at all.
- `RaiderOpenDoorGoal` (`AbstractIllager.RaiderOpenDoorGoal`): plain `OpenDoorGoal(false)` but
  gated on an active raid, moved to priority 3 as vanilla.
- `VindicatorJohnnyAttackGoal`: `isJohnny` gates an `ActiveTargetGoal` over every attackable living
  entity that is not an `#illager_friends` type, plus players. `target.attackable()` is modelled by
  a new `LivingEntity.attackable` flag (default `true`, `false` on armor stands). The Johnny flag is
  read/written as the `Johnny` NBT byte and is set whenever the custom name becomes "Johnny": a
  name tag through `Mob::on_custom_name_set`, and NBT load / `/summon` in `mob_read_nbt`.
- Aligned goal priorities with vanilla: avoid-creaking(1), break-door(2), raider-open-door(3),
  hold-ground(4), melee(5), stroll(8, 0.6), look-at-player(9, 3), random look(10); added the
  missing `HurtByTargetGoal(Raider).setAlertOthers()` target. Same-priority ties follow the vanilla
  super-call chain, so `Raider`'s path-to-raid is inserted before raider-open-door at 3 and
  raider-celebration before melee at 5.

## Illusioner

- New `spellcaster_illager.rs` holds `SpellcasterIllagerData` (spell tick countdown + current
  spell), the `SpellcasterIllager` trait, `SpellcasterCastingSpellGoal`, and the
  `SpellcasterUseSpell` base shared by the two spells. The Evoker now uses the shared
  `SpellcasterIllagerData`, `SpellcasterIllager` impl and `SpellcasterCastingSpellGoal` instead of
  its inline copies, and its three spell goals were moved onto `SpellcasterUseSpell` as well. The
  shared `start` converts the cast warmup with `to_goal_ticks`, so the evoker warms up for the
  vanilla 20/20/40 server ticks instead of double that (the old goals assigned the raw value even
  though their `tick` runs every other game tick).
- `IllusionerMirrorSpellGoal` (casting time 20, interval 340, `DISAPPEAR`, invisibility 1200) and
  `IllusionerBlindnessSpellGoal` (casting time 20, interval 180, `BLINDNESS`, 400, once per
  target, only when regional difficulty is harder than Normal).
- `RangedBowAttackGoal` reuses `bow_attack.rs` (speed 0.5, interval 20, range 15); the bow is
  granted from the equipment registry on spawn. Added `RangedAttackMob::perform_ranged_attack`
  matching `AbstractSkeleton`'s arrow launch.
- Added `AvoidEntityGoal(Creaking)` at 3 and full `registerGoals` parity (removed the extra
  hold-ground goal vanilla does not have), plus `setUnseenMemoryTicks(300)` on the three target
  goals. `getCelebrateSound` is now `ILLUSIONER_AMBIENT` (vanilla), not the evoker's.
- Same-priority goals are inserted in vanilla order: `Raider.registerGoals` runs first, so the
  banner goal precedes casting-spell and path-to-raid precedes avoid-creaking.

## Not ported / differences

- **Arm poses** (`AbstractIllager.getArmPose` / `IllagerArmPose`) have no Pumpkin equivalent; the
  synced spell-casting id is set, but no arm-pose state is sent.
- **`noActionTime`** resets in `VindicatorBreakDoorGoal.start` and
  `VindicatorJohnnyAttackGoal.start` are not modelled (Pumpkin has no such timer); left as
  `TODO`s.
- **Illusioner clone offsets** (`aiStep`/`getIllusionOffsets`, `ILLUSIONER_MIRROR_MOVE`) are
  client-only; skipped as agreed.
- **`AbstractIllager.considersEntityAsAlly`** (illager-friends team rule) is not implemented
  globally. Johnny instead filters `#illager_friends` in its own target predicate, which gives
  the described "all non-illager living entities" behaviour without touching shared ally code.
- **Johnny from a loaded name**: Pumpkin flips Johnny when the loaded `CustomName` is "Johnny"
  (requested by review). Vanilla's `Vindicator.readAdditionalSaveData` reassigns
  `isJohnny = input.getBooleanOr("Johnny", false)` after `setCustomName`, so vanilla resets it to
  false unless the `Johnny` byte is also set; Pumpkin keeps it true.
- **Blindness effect source** (`addEffect(..., Illusioner.this)`) is dropped because Pumpkin's
  `LivingEntity::add_effect` takes no source.
- **`custom_server_ai_step` ordering**: Pumpkin runs it after navigation ticks, vanilla's
  `customServerAiStep` before; the door flag therefore applies on the next tick. Functionally
  small.
- Vindicator spawn equipment (iron axe only when no raid) is handled by the shared equipment
  registry, which gives the axe unconditionally; pre-existing, not changed here.

## Checks

- `cargo fmt --check`, `cargo clippy -p pumpkin-core --all-targets --all-features` and
  `cargo check --workspace --all-targets` pass. No in-game test run; see the play-test checklist.
