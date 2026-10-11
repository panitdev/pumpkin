# Port log: Blaze (vanilla 26.3)

- Pumpkin base: `Pumpkin-MC/Pumpkin@5ede4c2` (2026-10-08), `CURRENT_MC_VERSION = 26.3`
- Vanilla reference: 26.3 server jar, classes `world.entity.monster.Blaze` (+ inner
  `BlazeAttackGoal`), `world.entity.projectile.hurtingprojectile.SmallFireball`,
  `Fireball`, `AbstractHurtingProjectile`
- Setup steps: see [README.md](README.md). Took ~5 minutes to get decompiled classes; most of
  the time went to the first cargo build (~8 min cold).

## Files touched

| File | Change |
|---|---|
| `crates/pumpkin-core/src/entity/mob/blaze.rs` | goals, target goals, path malus, climbing, rain damage |
| `crates/pumpkin-core/src/entity/ai/goal/blaze_attack.rs` | follow range from attribute, silent check |
| `crates/pumpkin-core/src/entity/projectile/small_fireball.rs` | owner attribution, fire-tick restore, fire placement rules |

## What already matched

- Attack goal cycle: charge (60 ticks, sets the synced "charged" flag) → 3 fireballs 6 ticks
  apart → 100-tick cooldown. Melee within 2 blocks every 20 ticks. Chase for 5 ticks after
  losing sight. Same numbers, same branches.
- Fireball aim: triangular spread `2.297 * sqrt(sqrt(dist²)) * 0.5` on X/Z, spawned at
  mid-body height + 0.5, world event 1018 for the shoot sound.
- Slow fall: downward velocity × 0.6 per tick while airborne.
- Water damage: 1 drowning damage per tick while in water.
- Player target goal at priority 2 with visibility check; look-at-player and random look at 8.
- Snowballs deal 3 damage to blazes (`snowball.rs`).
- Sounds, XP (10), loot, fire immunity, attributes come from generated entity data.

## Gaps found and fixed

1. **Blaze never climbed toward targets above it.** Vanilla rerolls an "allowed height offset"
   every 100 ticks (triangle around 0.5, spread 6.891) and, when the target's eye is higher than
   the blaze's eye plus that offset (and the target is attackable), eases vertical speed toward
   0.3 by 30% of the gap per tick. This is what makes blazes rise up to players on fortress
   bridges. Implemented in `custom_server_ai_step` so it's skipped under NoAI like vanilla.
2. **Blaze had a swim goal.** Vanilla registers none. Pumpkin's `SwimGoal` also flips the
   navigator to `can_float`, which zeroes the water malus — blazes would path into water that
   hurts them. Removed.
3. **No revenge targeting.** Vanilla target priority 1 is hurt-by-target with alert-others, so
   hitting one blaze aggroes nearby blazes. Added `RevengeGoal::new(true).alerting_others()`.
4. **Wrong idle goals.** Vanilla: move-towards-restriction at 5, stroll at 7. Pumpkin had wander
   at 5 and no restriction goal. Fixed priorities and added the restriction goal.
5. **No pathfinding malus.** Vanilla: water −1 (never), lava 8, fire / fire-adjacent 0. Set on
   the navigator.
6. **Rain didn't hurt.** Vanilla's water-sensitive damage applies in water *or rain* (rain at
   feet or top of hitbox). Same check Enderman already uses in Pumpkin.
7. **Follow range hardcoded 48.** Now read from the `FOLLOW_RANGE` attribute, so attribute
   modifiers work.
8. **Shoot sound ignored `Silent`.** Event 1018 is now skipped for silent blazes.
9. **Small fireball damage had no attacker.** It damaged the target with the target itself as
   the caller, so no kill credit, no revenge, no "shot by blaze" death message. Now passes the
   fireball as direct source and the owner as cause.
10. **Fire ticks stuck on failed hits.** Vanilla restores the target's previous fire ticks when
    the damage doesn't land (e.g. invulnerable). Added.
11. **Fireball fire placement.** Pumpkin set fire unconditionally on the hit face, overwriting
    whatever block was there and ignoring `mobGriefing`. Now: mob-owned fireballs respect
    `mobGriefing`, fire only goes into air, and soul fire is used on soul-fire base blocks with
    the normal flammable-neighbor fire state.

## Still open (not done here)

- **Water-avoiding stroll.** Vanilla uses `WaterAvoidingRandomStrollGoal` (land target when in
  water). Pumpkin has no such goal; skeleton and others also approximate with `WanderAroundGoal`.
  Worth adding as a shared goal, then switching all its users.
- **Fireball physics.** Pumpkin's `SmallFireballEntity` rides on `ThrownItemEntity`. Vanilla
  `AbstractHurtingProjectile` keeps accelerating along its direction, has inertia/drag that
  changes in water, and can be deflected by hitting it. Needs a hurting-projectile base
  in Pumpkin; affects ghast fireballs and wither skulls too.
- **`isOnFire` = charged.** Vanilla reports a charged blaze as on fire. Mostly client-visual via
  the flag Pumpkin already syncs; any server logic that asks "is this entity on fire" won't see
  it.
- **Brightness override.** Vanilla returns full brightness for blaze light-dependent checks.
  Not used by any blaze server logic today.
- **Spawning.** Fortress spawn and magma-block spawn exist; spawner behavior not checked.
- No automated test for the climb/goal changes; verified by compile + clippy + existing tests.
  A test harness that ticks a mob with a target above it would make this testable.

## Tips for the next mob

- Start the cold `cargo check -p pumpkin` before anything else.
- `custom_server_ai_step` vs `mob_tick` matters: put vanilla `customServerAiStep` logic in the
  former. The `TODO NoAI` comment in `mob/mod.rs` lists mobs still doing it wrong.
- Check what a shared goal does to the navigator (e.g. `SwimGoal` → `can_float`) before keeping
  it on a mob that vanilla doesn't give it to.
- Clippy runs pedantic-ish lints in CI (`semicolon_outside_block` etc.); run
  `cargo clippy -p pumpkin` before pushing.
