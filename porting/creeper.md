# Port log: Creeper (vanilla 26.3)

- Vanilla reference: `monster.Creeper` (`explodeCreeper`, `spawnLingeringCloud`)
- Beads: Pumpkin-x7o.6

## Files touched

| File | Change |
|---|---|
| `entity/mob/creeper.rs` | `spawn_lingering_cloud` after the explosion |

## Gaps found and fixed

- On explosion a creeper with active effects leaves an area effect cloud: radius 2.5, radius on
  use -0.5, wait time 10, duration 300, radius shrinking evenly to zero, copies of all active
  effects. Spawned after the explosion and before the creeper is removed, like vanilla.

## Still open

- Vanilla `potionDurationScale` (0.25) is not a cloud field in Pumpkin. The cloud applies 0.25
  to durations for `PotionApplicationSource::AreaEffectCloud`, which gives the same result.
- The shared cloud tick used to scale effects by `1 - dist / radius`, which vanilla does not do.
  It now applies the same 0.25 duration scale (0.5 for instant effects) to everyone in range.
  Lingering potion clouds change the same way, no other caller uses the cloud source.
- `triggerOnDeathMobEffects` (effects with on-death hooks, e.g. raid omen) is not ported.
