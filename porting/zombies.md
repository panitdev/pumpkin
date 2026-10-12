# Port log: Zombie family (vanilla 26.3)

- Vanilla reference: `world.entity.monster.zombie.{Zombie, Husk, ZombieVillager, Drowned,
  ZombifiedPiglin}`, goals `SpearUseGoal`, `ZombieAttackGoal`, `ResetUniversalAngerTargetGoal`,
  `ConversionTracker`, `Mob.convertTo`, `ConversionType.SINGLE`, `KineticWeapon.damageEntities`.

## Files touched

| File | Change |
|---|---|
| `entity/mob/conversion.rs` | shared `convert_to` for `ConversionType.SINGLE`, used by skeleton, zombie, husk, zombie villager |
| `entity/mob/zombie/mod.rs` | `ZombieKind`, drowning conversion, `on_attack`, goal set, `zombie_mob!` forwarding macros |
| `entity/ai/goal/spear_use.rs` | `SpearUseGoal` |
| `item/items/spear.rs`, `entity/living.rs`, `pumpkin-data` `KineticConditionImpl` | kinetic attack runs for any living user, mob action factor 0.2 |
| `entity/ai/util/land_random_pos.rs` | `get_pos_away` (vanilla `LandRandomPos.getPosAway`) |
| `entity/mob/zombified_piglin.rs` | vanilla goal set, lava malus, attacking speed modifier |

## Fixed

- Goal set matches `Zombie.registerGoals/addBehaviourGoals`: spear 2, attack 3 (was 2), egg 4,
  stroll 7, look 8. Villager target covers wandering traders without line of sight, turtle target
  is baby-on-land only (shared `ActiveTargetGoal::baby_turtle_on_land`, also used by skeletons).
- Water conversion (`ConversionTracker`): 600 ticks with the eyes in water, then 300 ticks of
  shaking, then zombie -> drowned and husk -> zombie. Drowned and zombie villagers do not convert.
  Synced flag, `InWaterTime` and `DrownedConversionTime` saved, world events 1040 and 1041.
- `Mob.convertTo` helper copies the SINGLE fields (rotation, motion, equipment with drop chances,
  fall distance, absorption, effects, baby, loot pickup, handedness, NoAI, persistence, name,
  fire, invulnerable, no gravity, portal cooldown, silent, tags) and keeps `canBreakDoors`.
  The skeleton to stray conversion now uses it.
- `Zombie.doHurtTarget`: an empty-handed burning zombie ignites its target. Husk adds Hunger
  (140 ticks per point of effective difficulty). Both run through `Mob::on_attack`.
- Husk and drowned now get the zombie default equipment (they forwarded nothing before).
- Zombie villager cure goes through the helper (keep equipment off) and drops guaranteed-drop
  slots like `dropPreservedEquipment`; cure speed-up uses the `speeds_up_zombie_villager_curing` tag.
- `SpearUseGoal`: charge, hold the spear out for the kinetic damage window, back off, repeat.
  Mobs use the same kinetic attack as players with the 0.2 action factor.
- Zombified piglin: `Zombie` goal set (spear 1, attack 2, egg 4, stroll 7, look 8), lava malus 8,
  +0.05 speed while angry and adult. Group anger and universal-anger reset were already ported.

## Open

- `MoveThroughVillageGoal`: no village/POI data in Pumpkin (only portal POIs). TODO left in
  `ZombieEntityBase::with_can_break_doors`.
- `WaterAvoidingRandomStrollGoal` does not exist; `WanderAroundGoal` stands in.
- Not ported from `Zombie`: reinforcements on hurt, `handleAttributes` (leader bonus, follow range,
  knockback resistance), `finalizeSpawn` rolls (loot pickup, door breaking, Halloween head, chicken
  jockeys), baby speed modifier and 2.5x XP, `killedEntity` villager infection, camel husk riders.
- Zombified piglin is not on `ZombieEntityBase`: it lacks the `doHurtTarget` ignite, the first anger
  sound and `wantsToPickUp`.
- `convert_to` skips vehicle, passengers, leash, sleeping position, team and `ANGRY_AT` memory.
- Spear: mobs do not wear the spear down, no camel charge speed, the head turns by look control
  instead of `Mob.lookAt`, so the stab direction follows the body yaw.
- Zombie villager cure: `PREVENT_ARMOR_CHANGE` slots are not moved to the villager.
