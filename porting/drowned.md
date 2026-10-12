# Port log: Drowned (vanilla 26.3)

- Vanilla reference: `world.entity.monster.zombie.Drowned` (inner classes
  `DrownedGoToWaterGoal`, `DrownedTridentAttackGoal`, `DrownedAttackGoal`,
  `DrownedGoToBeachGoal`, `DrownedSwimUpGoal`, `DrownedMoveControl`),
  `AmphibiousPathNavigation`, `MoveToBlockGoal`, `RangedAttackMob`, `ThrownTrident`.
- The Drowned previously inherited the Zombie goal set and registered nothing of its own,
  so trident Drowned never threw (upstream Pumpkin-MC/Pumpkin#3069).

## Files touched

| File | Change |
|---|---|
| `entity/mob/zombie/drowned.rs` | Drowned goals, trident attack, movement/navigation, `okTarget`, equipment options |
| `entity/mob/zombie/mod.rs` | skipped the Zombie goal set for `ZombieKind::Drowned`; `searchingForLand`, `wantsToSwim`, `updateSwimming` |
| `entity/mob/mod.rs` | `Mob::is_pushed_by_fluids` hook (default true) forwarded through the blanket `EntityBase` impl |

## Fixed

- `Drowned.addBehaviourGoals` replaces the Zombie behaviour goals: GoToWater 1, TridentAttack 2,
  Attack 2, GoToBeach 5, SwimUp 6, RandomStroll 7; goals 0 (Float), 4 (turtle egg) and 8 (look)
  stay from `Zombie.registerGoals`. `DrownedEntity::new` registers them once its `Arc` exists
  because the trident goal holds a weak `RangedAttackMob`; the base skips the Zombie set for
  `ZombieKind::Drowned`.
- Targets: `HurtByTargetGoal(this, Drowned.class).setAlertOthers(ZombifiedPiglin.class)`, players
  at range 10 with the `okTarget` filter, villagers, iron golems, axolotls, baby turtles on land.
- `DrownedTridentAttackGoal` is a `RangedAttackGoal` that needs a trident in the main hand and
  raises the arm (`set_attacking` + `set_active_hand`) while aiming, mirroring
  `DrownedAttackGoal` as a `ZombieAttackGoal` gated on `okTarget`.
- `Drowned.performRangedAttack`: throws a `TridentEntity` (main-hand trident or a fresh one) with
  the vanilla lead (`yd + distance * 0.2`), power 1.6 and `RangedAttackMob.rangedAttackUncertainty`
  (`14 - difficulty * 4`), plays `entity.drowned.shoot`.
- Navigation is amphibious (`Navigator::amphibious(false)`), matching
  `Drowned.createNavigation`. `searchingForLand` drives `wantsToSwim`, which in turn drives the
  `swimming` flag (`updateSwimming`), the `+0.002` buoyancy and `isPushedByFluid`.
- `DrownedGoToBeachGoal` reuses `MoveToTargetPosGoal` (the `MoveToBlockGoal` equivalent) with the
  day/water/sea-level gate; `DrownedSwimUpGoal` uses `DefaultRandomPos.getPosTowards`; both flip
  `searchingForLand`.
- `Drowned.populateDefaultEquipmentSlots` was already ported. Added `getPreferredWeaponType`
  (`drowned_preferred_weapons`) and `canReplaceCurrentItem` (nautilus shell never replaced).

## Open / not ported

- `DrownedMoveControl` is only the buoyancy part. Pumpkin's `AmphibiousPathNavigation` writes
  `movement_input` directly, so the vanilla speed-lerp and direct velocity input have no slot;
  a `TODO` names the vanilla class. The pathfinder `AmphibiousNodeEvaluator` already exists and
  is now used.
- `Drowned.updateSwimming` runs from `mob_tick`, before `Entity.update_fluid_state` in the same tick,
  while vanilla runs it in `Entity.baseTick` right after `updateFluidInteraction`. Pumpkin has no
  per-entity hook after the fluid update, so the swimming flag (and the fluid push it gates through
  `isPushedByFluid`) is one tick out of phase. Closing this needs an `Entity`-level hook after
  `update_fluid_state`.
- `Drowned.finalizeSpawn`: the offhand nautilus shell roll and the zombie-nautilus jockey are not
  ported (no `ZombieNautilus` in Pumpkin).
- Not ported, no Pumpkin hook or engine feature: ambient/hurt/death/step/swim sound overrides,
  baby dimensions, `canSpawnInLiquids`, `checkSpawnObstruction`, `rideTick`, `isVisuallySwimming`
  (mobs never enter the `Swimming` pose), `travelInWater`, `wantsToPickUp` (spears) and the
  `rangedAttackUncertainty` test setter.
- `Drowned` adds no NBT fields in vanilla; the conversion state (`InWaterTime`,
  `DrownedConversionTime`) was already saved by `ZombieEntityBase`.
- `Drowned` does not override `canBreatheUnderwater` in 26.3, so nothing to port there.
