# Port log: Fishing rod (vanilla 26.3)

- Tracker issue: `Pumpkin-cx6.1`
- Vanilla reference: `FishingRodItem.use`, `FishingHook` (constructor, `catchingFish`, `calculateOpenWater`, `retrieve`)
- Files: `crates/pumpkin-core/src/item/items/fishing_rod.rs`, `entity/projectile/fishing_bobber.rs`,
  `entity/passive/villager/mod.rs` (`enchant_with_levels`, split out of the trade enchanting)

## Fixed

1. Casting reads Luck of the Sea and Lure from the rod, plays the throw sound at vanilla volume and pitch,
   and launches the bobber with the `FishingHook` constructor's velocity instead of look * 1.5.
2. `retrieve` follows vanilla: a hooked entity is pulled by 0.1 * delta (status 31 broadcast, 3 damage for
   items, 5 otherwise); a bite rolls the fishing loot, throws each item toward the player with the vanilla
   velocity, spawns an XP orb of 1 to 6 per item, counts `FISH_CAUGHT` for items in `#minecraft:fishes`
   and takes 1 durability; a bobber on the ground costs 2. The rod is damaged through `damage_held_item`.
3. The bite timing is the `catchingFish` state machine (lure, hook and nibble phases, rain and sky speed
   modifiers, Lure reduction, splash and fishing particles, splash sound) replacing the old random countdown.
4. `calculateOpenWater` decides whether treasure can be rolled; it is evaluated like vanilla while a fish is
   approaching and reset when idle.
5. Luck weights the top level choice: `floor(weight + quality * luck)` with junk 10/-2, treasure 5/2 (open
   water only) and fish 85/-1, where luck is the rod bonus plus the player's `LUCK` attribute.
6. Sub table functions: junk boots and rod get `set_damage` up to 0.9, junk potions are water bottles, treasure
   bow, rod and book are `enchant_with_levels` 30 from `#minecraft:on_random_loot` (rod and bow also damaged
   up to 0.25), and the bamboo entry is only kept in jungle biomes.

## Open

- The generated `gameplay/fishing` static table flattens nested tables and drops quality, conditions and
  functions, so the top level choice and the functions above are applied in `fishing_bobber.rs` with the
  vanilla numbers as constants. Extending the loot codegen would remove them.
- Bobber physics still use the old simplified water and gravity model; the `FishHookState` machine
  (flying, bobbing, hooked) and the 1200 tick ground despawn are not ported.
- `FISHING_ROD_HOOKED` and treasure related advancement triggers are not wired; only `FishedItem` is.
- Item use stats, use vibration and the `shouldStopFishing` checks (distance, rod not held) are not ported.
