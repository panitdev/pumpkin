# Port log: Beds (vanilla 26.3)

- Issue: `Pumpkin-747.1`; vanilla: `BedBlock`, `AbstractBedBlock`, `ServerPlayer.startSleepInBed`
- File: `crates/pumpkin-core/src/block/blocks/bed.rs`

## Matched already
- Explode when the dimension bed rule says so (nether/end), occupied message, obstructed check,
  respawn set before the sleep-time check, fall damage x0.5 and 0.66 bounce, head/foot breaking
  (creative skips drops), half removal on state replace.

## Fixed
- Reach check: vertical tolerance is 2 (was 3), as in `isReachableBedBlock`.
- Monster-nearby check is skipped for creative players.

## Open
- Villager wake-up on an occupied bed (`kickVillagerOutOfBed`): no villager sleeping state exists; TODO kept with precise reference.
- Monster check uses a fixed entity-type list; vanilla `isPreventingPlayerRest` is conditional for piglins/endermen/zombified piglins (anger).
- Obstruction uses `is_solid` rather than collision-free (`freeAt`); `sleep.not_possible` overlay not sent; bed-rule error message on explosion not sent.
