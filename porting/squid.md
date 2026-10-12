# Port log: Squid and Glow Squid (vanilla 26.3)

- Vanilla reference: `animal.squid.Squid` (+ `Squid.SquidRandomMovementGoal`,
  `Squid.SquidFleeGoal`), `animal.squid.GlowSquid`, `animal.AgeableWaterCreature`,
  `animal.AgeableMob`, `AgeableMob.AgeableMobGroupData`.
- Beads: Pumpkin-x7o.68 (Squid), Pumpkin-x7o.51 (Glow Squid). One commit covers both because the
  glow squid only adds dark ticks, a glow particle and its own sounds/ink particle on top.
- Note: vanilla `Squid` does not use path navigation. It moves through its own `movementVector`
  written by its two goals and applied by its `travel` override, so "water-bound navigation" here
  means porting that movement, not a navigator.

## Files touched

| File | Change |
|---|---|
| `entity/passive/squid.rs` | `SquidState`, `SquidMob`, `register_squid`, `tick_squid` (tentacle, movement vector, body rotation), `squid_travel_in_water`, `spawn_ink`, `tick_ambient_sound`, `SquidRandomMovementGoal`, `SquidFleeGoal`, `SquidEntity` |
| `entity/passive/glow_squid.rs` | uses the shared squid code, adds synced `DATA_DARK_TICKS_REMAINING`, dark tick countdown, glow particle, glow ink particle/sounds |
| `entity/mob/mod.rs` | `Mob::as_squid` accessor |

## What already matched

- Attributes (`GRAVITY` 0.08, `MAX_HEALTH` 10) and the squid/glow squid hurt and death sounds come
  from generated `entity_type` data.
- Squid surface-water and glow squid deep-water spawn rules are already in `entity/type.rs`.

## Gaps found and fixed

1. Goals were `SwimGoal`, `WanderAroundGoal`, `LookAtEntityGoal`, `RandomLookAroundGoal`, none of
   which the squid has. Vanilla only registers `SquidRandomMovementGoal` (priority 0) and
   `SquidFleeGoal` (priority 1). Replaced with those two.
2. No movement vector, no tentacle/body rotation, no `travel` override. Added `SquidState` and
   `tick_squid`, which runs after `travel` (`post_tick`) so the velocity set this tick is used by
   the next tick's movement, matching `Squid.aiStep` calling `super.aiStep()` first. `travel_in_water`
   moves by the squid's own delta movement and ignores the input, like `Squid.travel`.
3. No ink cloud. `Squid.hurtServer` spawns ink only when `getLastHurtByMob() != null`; hooked into
   `Mob::on_damage` and gated on a living attacker. Uses `Squid.rotateVector` with the previous
   body rotations for the particle direction, the squirt sound and 30 `SQUID_INK` particles. The
   squirt and ambient sounds play at `Squid.getSoundVolume()` (0.4), not the entity default.
4. No flee from the attacker. Added `SquidFleeGoal` (10-block radius, speed ramp, bubble every 10
   ticks, prioritised over the random goal).
5. No ambient sound. Added the `Mob.baseTick` ambient timer with the water-creature 120 tick
   interval, and reset it in `on_damage` like `Mob.playHurtSound`.
6. Glow squid had no dark ticks or glow particle. Added the synced `DARK_TICKS_REMAINING` countdown
   (`aiStep`), the per-tick glow particle, `DarkTicksRemaining` NBT and a 100 tick set on a hurt that
   passed `Squid.hurtServer` (damage applied and a living attacker), matching `GlowSquid.hurtServer`.
7. Neither squid was ageable. Added `AgeableMob` so babies age, the baby flag syncs and age NBT
   round-trips (the vanilla `Squid` extends `AgeableWaterCreature extends AgeableMob`).
8. `isPushedByFluid` (`AgeableWaterCreature` returns false) was not overridden, so flowing water
   pushed the squids. Overrode `Mob::is_pushed_by_fluids` to false for both.

## Still open

- Baby spawn chance: vanilla `Squid.finalizeSpawn` uses `AgeableMobGroupData(0.05F)`, where the
  first spawn of a group is never a baby. Pumpkin's `SpawnGroupData` has no ageable variant, and
  rolling a flat 5% would make spawn eggs hatch babies, which vanilla does not do. Not added.
- Baby dimensions (vanilla `Squid.BABY_DIMENSIONS` 0.5x0.5, eye height 0.37): Pumpkin has no
  per-mob `getDefaultDimensions` hook, so a baby squid keeps the adult hitbox.
- `Squid.travel` overrides travel for every fluid; Pumpkin only exposes `travel_in_water`, so in
  air (a dying squid) the default `travel_in_air` runs. Its gravity and vertical drag recurrence is
  the same, but horizontal friction (0.91 instead of zeroed) and levitation handling differ slightly.
- `SquidRandomMovementGoal` also zeroes the movement vector once `Mob.getNoActionTime` passes 100;
  Pumpkin does not track the despawn no-action time yet (TODO in the goal).
- `getMovementEmission() = EVENTS` and `Squid.canBeLeashed() = true` (overriding
  `AgeableWaterCreature`'s false) are not modelled: Pumpkin has no per-mob movement-emission or
  leash-permission hook.
- `getBreedOffspring` is unreachable (the squid has no `BreedGoal`), so it was not ported.
- `SquidFleeGoal.canUse` uses `wasTouchingWater`; Pumpkin only tracks the current tick's
  `touching_water`, which is used instead.
- Render-only squid state (`zBodyRot`, `rotateSpeed`, `tentacleAngle`, `oldTentacleMovement`,
  `oldTentacleAngle`) is not tracked; only the movement, the entity event and the ink direction
  inputs (`xBodyRot`, the previous body yaw) are kept.
- `Mth.sin`/`Mth.cos`/`Mth.atan2` are lookup-table approximations in vanilla; std float versions
  are used, so tentacle angles and rotations can drift by a small amount.
- Not tested in game.
