# Port log: small passive mobs (vanilla 26.3)

- Vanilla reference: `ambient.Bat`, `animal.feline.Ocelot` (+ `Ocelot.OcelotAttackGoal`,
  `Ocelot.OcelotTemptGoal`, `Ocelot.OcelotAvoidEntityGoal`), `animal.polarbear.PolarBear`,
  `animal.rabbit.Rabbit` (+ `Rabbit.RaidGardenGoal`), `animal.equine.SkeletonHorse` +
  `SkeletonTrapGoal`, `ai.goal.{LeapAtTargetGoal, OcelotAttackGoal, ClimbOnTopOfPowderSnowGoal}`,
  `ai.goal.target.{ResetUniversalAngerTargetGoal, HurtByTargetGoal}`
- Beads: Pumpkin-x7o.40 (Bat), Pumpkin-x7o.59 (Ocelot), Pumpkin-x7o.62 (Polar Bear),
  Pumpkin-x7o.64 (Rabbit), Pumpkin-x7o.66 (Skeleton Horse).

## Files touched

| File | Change |
|---|---|
| `entity/passive/ocelot.rs` | registered `LeapAtTargetGoal(0.3)` at 7 and `OcelotAttackGoal` at 8 |
| `entity/passive/rabbit.rs` | `ClimbOnTopOfPowderSnowGoal` at 1, `RabbitRaidGardenGoal` at 5, food cooldown tick, `wants_more_food` |
| `entity/passive/polar_bear.rs` | `PolarBearAttackPlayersGoal` (cub-gated player charge) at target priority 2 |
| `entity/passive/skeleton_horse.rs` | trap flag/time + NBT + 15 minute discard, `SkeletonTrapGoal` |
| `entity/ai/goal/active_target.rs` | `follow_range_scale` builder (vanilla `getFollowDistance` override) |
| `entity/ai/goal/track_target.rs` | scaled continue range for the same builder |

No code change for the Bat; see below.

## Per mob

### Bat (Pumpkin-x7o.40) - nothing to port

Pumpkin's bat already mirrors vanilla `Bat.customServerAiStep` one-for-one: `mob_tick` splits into
`tick_roosting` and `tick_flying`, which is the random-target flight (pick a block up to 7/6/7
around, set velocity toward it, yaw from the velocity, 1 in 100 chance to roost). Vanilla does the
same, driving `deltaMovement` directly; the Bat does **not** override `createNavigation`, so there
is no flying navigator and no "navigation hook" to add. The only literal difference is vanilla's
`zza = 0.5F` forward input, which Pumpkin models through its delta-movement update instead of the
movement-input path; it does not change the observable random flight. The existing `mob/mod.rs`
TODO about moving `tick_flying` into `custom_server_ai_step` (for NoAI) still stands and is the
only real Bat gap.

### Ocelot (Pumpkin-x7o.59)

Added the two missing goals at their vanilla priorities (7 `LeapAtTargetGoal(0.3)`, 8
`OcelotAttackGoal`). Both goals were reused after checking them against the Java:

- `leap_at_target.rs` matches `LeapAtTargetGoal`: 4..16 sq range, on-ground, 1 in `reducedTickDelay(5)`,
  normalize-to-0.4 plus 20% carried momentum, `yd` vertical.
- `ocelot_attack.rs` matches `OcelotAttackGoal`: melee-radius based on `bbWidth * 2`, speed 1.33
  below 16 sq / 0.6 below 225 sq / 0.8 otherwise, 20 tick attack cooldown, 30/30 look range.

Still open: the Ocelot keeps the extra `EscapeDangerGoal`, `FollowParentGoal` and
`RandomLookAroundGoal` it already had that vanilla does not register. The tempting goal is the
plain `TemptGoal` rather than vanilla `OcelotTemptGoal` (which only breaks when the ocelot is not
trusting). The turtle target uses the plain goal, not `Turtle.BABY_ON_LAND_SELECTOR`.

### Polar Bear (Pumpkin-x7o.62)

Ported `PolarBearAttackPlayersGoal` as `ActiveTargetGoal(Player, interval 20, mustSee, mustReach)`
with `TargetCondition::Adult`, a `has_nearby_cub` gate (a baby polar bear within `inflate(8, 4, 8)`)
and the halved follow distance (`follow_range_scale(0.5)`), which needed the small
`ActiveTargetGoal`/`TrackTargetGoal` scaling added for vanilla's `getFollowDistance() * 0.5`.
`ResetUniversalAngerTargetGoal(false)` and the baby-aware `PolarBearHurtByTargetGoal` were already
present (`apply_targets` adds the reset goal at 5; the revenge goal uses `raising_alarm_when(is_baby)`
and `alerting_only(!is_baby)`), so those checklist items need no code.

Still open: `PolarBearMeleeAttackGoal`'s standing warning (`setStanding` / `playWarningSound` while
the target is in the wider band, cooldown-aware) is not ported, so `set_standing` is still never
called. The PanicGoal damage-tag split (baby `PANIC_CAUSES`, adult `PANIC_ENVIRONMENTAL_CAUSES`) is
not modelled; Pumpkin only has the `PANIC_CAUSES` path. The cub gate is also re-checked every tick
through `ActiveTargetGoal::should_continue`, so a bear drops the player if the cub leaves the
8x4x8 box, where vanilla only checks the cub when acquiring the target.

### Rabbit (Pumpkin-x7o.64)

- Registered `ClimbOnTopOfPowderSnowGoal` at 1.
- Added `RabbitRaidGardenGoal` at 5, built on `MoveToTargetPosGoal` like `DestroyEggGoal`. It is
  gated on `mob_griefing` and `wants_more_food`, searches `#minecraft:supports_crops` with a mature
  carrot above, walks there, then eats one age stage (age 0 -> air, else age - 1 plus the
  `BLOCK_CHANGE` game event and the 2001 destroy-block event) and sets `more_carrot_ticks = 40`.
  Like vanilla, `mob_griefing` is only checked on the search tick, so the cooldown keeps running.
- Added the `Rabbit.customServerAiStep` food cooldown (`more_carrot_ticks -= random(0..2)`) so the
  rabbit eventually raids again.

Still open: vanilla `RaidGardenGoal` sets a `wantsToRaid`/`canRaid` pair that only matches a target
while not already raiding; because `MoveToTargetPos::is_target_pos` is `&self`, the goal recomputes
crop maturity each tick instead of caching `canRaid`, and stops by clearing `wants_to_raid` after
eating. The age-0 path sets air without dropping the block (vanilla calls `destroyBlock`, which is a
no-op on the air it just placed). The Rabbit's custom `JumpControl`/`MoveControl` (hop timing,
`RabbitPanicGoal` speed push) are not modelled, and the avoid goal is on `Monster`/`Fox` rather than
vanilla `Monster.class` at range 4.

### Skeleton Horse (Pumpkin-x7o.66)

- Added `is_trap`/`trap_time` fields, `SkeletonTrap`/`SkeletonTrapTime` NBT, and the `aiStep`
  18000-tick discard for untethered trap horses.
- Added `SkeletonTrapGoal`: near an alive non-spectator player within 10, it clears the trap flag,
  tames the horse, spawns a visual-only lightning bolt, mounts a skeleton on the trapped horse, then
  spawns three more tamed skeleton horses each carrying a skeleton, pushed apart with
  `triangle(0, 1.1485)`. Riders get an iron helmet if bare, and their weapon and helmet have any
  spawn-roll enchantments cleared before the `mob_spawn_equipment` enchantments are applied.

Still open: the goal is always registered and gated on `is_trap` instead of vanilla's dynamic
`setTrap` add/remove, because the goal selector is `mem::take`n while goals run. `setInvulnerableTime(60)`
has no engine equivalent (TODO in the goal). `finalizeSpawn` is called without a difficulty instance.
The trigger is "nearest player within 10 with `!is_spectator && is_alive`", which matches
`hasNearbyAlivePlayer` (creative counts).

## What already matched

- Bat roosting/flight/spawn rules and the polar bear standing flag/sync.
- Ocelot trust/tempt/avoid, chicken and turtle targeting.
- Rabbit variants, tempt, breed.
- Skeleton horse taming/saddle.

## Checks

`cargo check -p pumpkin-core`, `cargo clippy -p pumpkin-core --all-targets --all-features`,
`cargo check --workspace --all-targets` and `cargo fmt` all pass. Not tested in game.
