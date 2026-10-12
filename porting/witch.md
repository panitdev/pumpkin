# Port log: Witch (vanilla 26.3)

- Vanilla reference: `world.entity.monster.Witch`,
  `world.entity.ai.goal.target.NearestHealableRaiderTargetGoal`,
  `world.entity.ai.goal.target.NearestAttackableWitchTargetGoal`, `world.entity.raid.Raider`
- Beads: Pumpkin-x7o.29 (Witch, partial)

## Files touched

| File | Change |
|---|---|
| `crates/pumpkin-core/src/entity/mob/witch.rs` | healing-raider target goal, attack-players target goal, raid cooldown in `aiStep`, raider branch in `performRangedAttack` |

## What already matched

- Throw splash potions: harming, slowness (dist >= 8), poison (health >= 8), weakness (dist <= 3,
  25%), throw speed 0.45/0.75, uncertainty 8.0.
- Drink potions to heal / resist: water breathing, fire resistance, healing, swiftness, using time
  32, drink sound.
- Magic damage reduction (0.15 against `WITCH_RESISTANT_TO`) and no self damage.
- Revenge goal ignoring raiders and player targeting, raid goals (`PathfindToRaidGoal`,
  `RaiderMoveThroughVillageGoal`, `RaiderCelebrationGoal`), celebration sound, no leader.

## Gaps found and fixed

1. **No healing-raider targeting.** Vanilla `NearestHealableRaiderTargetGoal` targets any
   `Raider` except the witch, only while `hasActiveRaid()`, on a 50% roll, and on a 200-tick
   (reduced to 100) cooldown. Added as a wrapper over `ActiveTargetGoal::predicated` (class
   agnostic raider tag minus witch) at target priority 2.
2. **Attack-players target goal was not gated.** Vanilla uses `NearestAttackableWitchTargetGoal`,
   whose `canUse` is `canAttack && super.canUse()`. Added it at target priority 3; `canAttack` is
   driven from `aiStep`.
3. **Cooldown not ticked.** Vanilla `Witch.aiStep` decrements `healRaidersGoal` every tick and
   sets `attackPlayersGoal.setCanAttack(cooldown <= 0)`. Added the same at the top of `mob_tick`
   with two atomics on the witch.
4. **Throwing at a raider used the harming choice.** Vanilla throws healing (health <= 4) or
   regeneration otherwise at a `Raider`, then clears its own target. Added the `Raider` branch to
   `throw_potion`.

## Still open

- Goal-set differences carried over from `Raider`: vanilla uses `WaterAvoidingRandomStrollGoal`
  (Pumpkin `WanderAroundGoal`), the two `LookAtPlayerGoal`s and `ObtainRaidLeaderBannerGoal`.
  Not part of this issue.
- `SPEED_MODIFIER_DRINKING` (-0.25 movement speed while drinking) is not applied; Pumpkin has no
  transient attribute modifier port for it here.
- `handleEntityEvent(15)` witch particles and the `gameEvent(DRINK)` call are not ported (no
  particle/event port in this file).
- Equipment/attributes, sounds, XP and loot come from generated data.
- Not tested in game.
