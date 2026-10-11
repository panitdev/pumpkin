# Fire (FireBlock / BaseFireBlock)

Pumpkin: `crates/pumpkin-core/src/block/blocks/fire/fire.rs`. Vanilla: `FireBlock`, `BaseFireBlock`.

## Already matched
- Tick delay 30-39, reschedule each tick, age increment (`nextInt(3)/2`), rain extinguish chance
  `0.2 + age*0.03`, infiniburn per dimension, lingering rules for fuel-less fire, burn-out ring
  (300/250 odds), spread box (-1..1, -1..4, 100 + 100*(y-1) rate), difficulty term, portal creation.

## Fixed
- TODO: increased-burnout biomes now follow the vanilla `increased_fire_burnout` attribute
  (jungle, bamboo jungle, mushroom fields, swamp, mangrove swamp, dappled forest, snowy slopes).
  The old list had frozen/jagged peaks (not vanilla) and omitted dappled forest; the Nether was
  wrongly treated as increased burnout.
- Burnout is sampled at the fire's own position for both the burn-out ring and the spread halving.
- `fire_spread_radius_around_player` gates the whole fire tick (as vanilla's `canSpreadFireAround`),
  not just spreading.
- Waterlogged blocks have zero burn and ignite odds (was: any adjacent fluid blocked ignite odds).
- Burned-block replacement only checks rain at the block itself, not neighbours.
- Neighbour updates keep the fire's age (was reset to 0) and fire spreading/replacing onto soul
  base blocks yields soul fire.
- Spread fire is placed with full notify flags so it schedules its own tick.

## Open
- Dimension infiniburn is matched by dimension id to the three infiniburn tags rather than read
  from dimension type data.
- Flammability uses `burn_chance`/`spread_chance` from block data, not a per-fire map.
