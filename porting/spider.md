# Port log: Spider and Cave Spider (vanilla 26.3)

- Vanilla reference: `monster.spider.Spider` (+ `SpiderAttackGoal`, `SpiderTargetGoal`,
  `SpiderEffectsGroupData`), `CaveSpider`, `ai.goal.LeapAtTargetGoal`, `AvoidEntityGoal`,
  `ai.navigation.WallClimberNavigation`
- Beads: Pumpkin-x7o.23 (Spider), Pumpkin-x7o.4 (Cave Spider). One commit covers both because
  the cave spider only adds poison and an empty `finalizeSpawn` on top of the spider.

## Files touched

| File | Change |
|---|---|
| `entity/mob/spider.rs` | shared `register_spider_goals` and `tick_climbing`, wall climber navigator |
| `entity/mob/cave_spider.rs` | uses the shared goals and climbing, `finalize_spawn` returns the group data untouched |
| `entity/ai/goal/avoid_entity.rs` | `with_filter` (vanilla `avoidPredicate`) |
| `entity/ai/goal/leap_at_target.rs` | near-zero horizontal delta keeps the raw delta like vanilla |
| `entity/ai/goal/spider_attack.rs` | does not start while the spider has a passenger |
| `entity/ai/control/move_control.rs` | `should_jump` extracted, adds vanilla's block collision shape part (not doors/fences) |
| `entity/living.rs` | `climbs_walls` flag keeps `climbing` set after `check_climbing` (vanilla `onClimbable` override) |
| `entity/ai/pathfinder/mod.rs` | wall climber jump uses the shared `MoveControl` jump check |

## What already matched

- Light-gated targeting: `TargetCondition::NoDaylight` on the player and iron golem target
  goals is vanilla `SpiderTargetGoal` (eye-position light level dependent value >= 0.5).
- `finalizeSpawn` for the spider: 1 in 100 skeleton jockey, hard difficulty effect shared by the
  spawn group. Cobweb immunity (`Entity::slow_movement`), synced climbing flag, cave spider poison
  by difficulty (0 / 7 s / 15 s).

## Gaps found and fixed

1. No leap goal. Added `LeapAtTargetGoal(0.4)` at priority 3. Fixed its fallback when the target
   is almost exactly above/below (horizontal delta length squared <= 1e-7): vanilla keeps the raw
   tiny delta, Pumpkin kept the old motion.
2. No armadillo avoidance. Added `AvoidEntityGoal(armadillo, 6, 1.0, 1.2)` at priority 2, only
   for armadillos that are not scared.
3. Attack goal had priority 3 and `pause_when_mob_idle = false`. Vanilla: priority 4, `true`.
   Also not usable while the spider carries a rider (jockey).
4. Climbing flag was set from "velocity is zero". Vanilla uses horizontal collision. It now also
   sets the living entity's `climbs_walls` flag so the spider gets the climbing movement rules
   (clamped speed, up-velocity 0.2 against walls, no fall damage accumulation).
5. The spider used the plain ground navigator. It now uses the existing wall climber navigator.
6. Cave spider ran the spider's jockey/effect roll and the base follow-range setup. Vanilla
   returns the group data unchanged.
7. Cave spider had no climbing flag or tick.

## Still open

- Wall climber navigation drives movement directly instead of through `MoveControl.setWantedPosition`
  (the navigator tick has no access to the mob's move control). Behaviour is close, but the
  rotation is not rate limited.
- `canBeAffected`: spiders are immune to poison in vanilla. Pumpkin has no per-entity effect
  immunity hook yet.
- `getVehicleAttachmentPoint` (jockey rider height: spider 0.3125, cave spider 0.21875 times scale)
  is not ported; riders use the default offset.
- `WaterAvoidingRandomStrollGoal` is approximated by `WanderAroundGoal`, as for other mobs.
- `AvoidEntityGoal` uses a radius search with a creative/spectator filter, not vanilla's
  `TargetingConditions.forCombat` (line of sight and attackability are not checked).
- Not tested in game.
