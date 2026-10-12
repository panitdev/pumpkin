# Port log: Skeleton family (vanilla 26.3)

- Vanilla reference: `world.entity.monster.skeleton.{AbstractSkeleton, Skeleton, Stray, Bogged,
  WitherSkeleton}`, goals `RangedBowAttackGoal`, `RestrictSunGoal`, `FleeSunGoal`,
  `ProjectileUtil.getMobArrow`, `ConversionTracker`.

## Files touched

| File | Change |
|---|---|
| `entity/mob/skeleton/mod.rs` | one `SkeletonEntityBase` with a `SkeletonKind`; wrappers forward through `skeleton_mob!` |
| `entity/ai/goal/bow_attack.rs` | shoots through `RangedAttackMob`, either hand, power from draw time |
| `entity/ai/goal/restrict_sun.rs`, `flee_sun.rs` | avoid-sun navigation, vanilla hide position search |
| `entity/ai/goal/melee_attack.rs`, `mob/mod.rs` | `try_attack` returns whether it hurt, melee calls `Mob::on_attack` |
| `entity/living.rs`, `mob/mod.rs` | `Mob::can_be_affected` checked in `add_effect` |

## Fixed

- Goal set and priorities now match `AbstractSkeleton.registerGoals`: restrict sun 2, flee sun 3,
  wolf avoidance 3, stroll 5, look 6, plus iron golem and baby-turtle-on-land targets. The swim goal
  is gone (vanilla has none). Wither skeletons add piglin targets and a lava malus of 8.
- Bow and melee goals are mutually exclusive at priority 4 and swapped when the held item changes
  (`reassessWeaponGoal`). There is no equipment-change hook, so `mob_tick` compares the held item.
  Bow interval is 20 on hard, 40 otherwise (bogged 50 and 70).
- Arrow: `performRangedAttack` lives on the mob. Stray adds slowness (600 ticks), bogged poison
  (100 ticks) as potion contents on a plain arrow, wither skeleton sets it on fire. The old
  "burning mob shoots flame arrows" rule is not in 26.3 and was removed. Shoot sound has the vanilla
  pitch roll.
- Skeleton in powder snow: 140 ticks, then a 300 tick shaking conversion into a stray that keeps
  equipment and name (`FreezingTime`, `StrayConversionTime` saved).
- Bogged: shears drop the `shearing/bogged` loot, `sheared` is saved and synced.
- Wither skeleton: wither effect (200 ticks) on hit, attack damage 4 at finalize, immune to wither (`Mob::can_be_affected`, checked in `LivingEntity::add_effect`).
- Mobs prefer dark spots (`Monster.getWalkTargetValue`), which FleeSun and stroll use.

## Open

- Stray conversion copies equipment, name and visibility, effects, absorption, fall distance, fire,
  gravity, invulnerability, body yaw and the mob flags. Leash, hurt time, entity tags and
  `CUSTOM_DATA` are not copied. No shared conversion helper exists.
- Halloween pumpkin head on spawn (`SpecialDates`) is not ported.
- Wither skeleton `canHoldItem` (disliked weapons) has no hook.
- Shearing by dispenser, shear game event, `Skeleton.isShaking` ground-truth for Parched are skipped.
- Turning on `Mob::on_attack` from the melee goal also activates the existing hoglin and zoglin
  throw hooks, which were never called before.
- `populate_default_equipment_*` on the skeleton wrappers were dead (equipment comes from the
  registry in `equipment.rs`) and were removed.
