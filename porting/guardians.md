# Port log: Guardian and Elder Guardian (vanilla 26.3)

- Vanilla reference: `monster.Guardian` (+ `GuardianAttackGoal`, `GuardianAttackSelector`,
  `GuardianMoveControl`), `monster.ElderGuardian`, `ai.goal.MoveTowardsRestrictionGoal`,
  `ai.goal.LookAtPlayerGoal`, `ai.goal.RandomStrollGoal`
- Beads: Pumpkin-x7o.12 (Guardian), Pumpkin-x7o.8 (Elder Guardian).

## Files touched

| File | Change |
|---|---|
| `entity/mob/guardian.rs` | `GuardianMob` trait + `GuardianState`, shared registration, flop, thorns, water travel, walk target value |
| `entity/mob/elder_guardian.rs` | attack duration 60, curse via `addEffectToPlayersAround` rules, home, persistence, sounds |
| `entity/ai/goal/guardian_attack.rs` | `Guardian.GuardianAttackGoal` |
| `entity/ai/control/guardian_move_control.rs` | `Guardian.GuardianMoveControl` |
| `entity/ai/goal/wander_around.rs` | shareable `trigger` flag (`RandomStrollGoal.trigger`), controls override |
| `entity/ai/goal/move_towards_restriction.rs` | controls override |
| `entity/ai/goal/look_at_entity.rs` | `also_matching` (vanilla matches `Guardian.class` including elders) |
| `entity/ai/control/look_control.rs` | `is_looking_at_target`, `wanted_position` |
| `entity/mob/mod.rs`, `entity/living.rs` | `Mob::as_guardian`, `Mob::travel_in_water` hook (vanilla `travelInWater` override) |

## Gaps found and fixed

1. **No beam attack.** `GuardianAttackGoal` at priority 4: starts at attack time -10, tracks the
   target through the synced `DATA_ID_ATTACK_TARGET` (set at 0, cleared on stop), plays the
   attack sound via entity event 21, and after 80 ticks (elder 60) deals indirect magic damage
   (1, +2 on hard, +2 elder) then the normal melee attack and drops the target. Loses the target
   without line of sight. Non-elders stop when the target is within 3 blocks.
2. **Wrong goal set.** Removed the swim goal. Vanilla: attack 4, move towards restriction 5,
   stroll 7 (interval 80, elder 400, MOVE|LOOK), look at player 8, look at guardians 8
   (12 blocks, chance 0.01), random look around 9.
3. **Wrong targeting.** One target goal at priority 1 for living entities: players, squid (incl.
   glow squid) and axolotls more than 3 blocks away, with line of sight. Elder previously only
   targeted players.
4. **Ground navigation.** Now `WaterBoundPathNavigation` with the water malus at 0, and
   `getWalkTargetValue` (water 10 + light cost, otherwise dark-preferring like `Monster`).
5. **Movement.** `GuardianMoveControl` (wobble push, speed lerp, look-ahead, synced
   `DATA_ID_MOVING`) and the `travelInWater` override (0.1 input, 0.9 drag, slow sink while idle).
6. **Thorns.** Vanilla applies thorns before the damage and only when the guardian is not
   moving, the damage type is not in `avoids_guardian_thorns`/thorns, and the direct source is
   a living entity. The old code ran after damage, always, and with the victim as the source.
   Also triggers the stroll goal on hurt.
7. **Land flop.** Out of water on the ground: random hop, random yaw, and the yaw follows the
   head while a beam is charging.
8. **Sounds.** `as_custom_sound` was never overridden, so the guardian sounds were unused. Both
   mobs now return the water/land hurt and death sounds (elder variants for the elder).
9. **Elder.** The curse used `age % 1200` and hit every player in range; it now uses
   `(age + id) % 1200`, survival/adventure non-allied players within 50 blocks whose fatigue is
   missing, lower than level III or under 1200 ticks left, and only those get the appearance
   packet (silent: 0). Runs in `custom_server_ai_step` so NoAI skips it. Home is set to the
   current block with radius 16 when missing, and persistence is required.

## Still open

- **Smooth turning.** Pumpkin's navigators write yaw and movement input directly instead of
  using the move control. The guardian control reads the next path node as the wanted position
  and overwrites the navigator's movement input, but the navigator's yaw snap still happens
  first, so path turns are not lerped at 90 degrees per tick. A move-control-driven navigator
  would fix this for all mobs.
- **Navigator speed modifier.** The control assumes 1.0, which is what every guardian goal uses.
- **Ambient sound** (interval 160, water/land variants): no ambient sound hook for this mob.
- **`getMaxHeadXRot` 180** has no Pumpkin counterpart.
- **Air supply.** Vanilla refills it in water; Pumpkin only runs breath logic for players.
- **Client-only parts** (tail and spike animation, beam particles, flop sound) need no server work.
- Boot test: summoned both on land for 25 s, no panics. The water path and beam were not run
  with a client.
