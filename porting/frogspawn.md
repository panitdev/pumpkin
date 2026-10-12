# Port log: Frogspawn (vanilla 26.3)

- Tracker issue: `Pumpkin-747.10`
- Vanilla reference: `FrogspawnBlock`, `Level.scheduleTick`, `ChestLidController` (n/a)
- Files: `crates/pumpkin-core/src/block/blocks/frogspawn.rs` (new),
  `crates/pumpkin-core/src/block/blocks/mod.rs`, `crates/pumpkin-core/src/block/registry.rs`,
  `crates/pumpkin-world/src/tick/mod.rs`, `crates/pumpkin-world/src/tick/scheduler.rs`,
  `crates/pumpkin-world/src/level.rs`, `crates/pumpkin-core/src/world/mod.rs`,
  `crates/pumpkin-world/src/chunk/format/mod.rs`

## Implemented

- Support check (`mayPlaceOn`): water below (`supports_frogspawn` fluid tag, or the block tag), no fluid in
  the frogspawn's own cell.
- Placement refused when unsupported; `updateShape` equivalent returns air when the support goes away.
- `onPlace` schedules a hatch tick with a random delay in `[3600, 12000)`.
- Hatch: destroy without drops, play `FROGSPAWN_HATCH`, spawn 2-5 tadpoles at the vanilla random offsets,
  yaw 1-360, persistence required.
- `entityInside`: destroyed by a falling block (26.3 only checks `EntityTypes.FALLING_BLOCK`).

## Scheduler change (required)

Vanilla schedules the hatch up to 12000 ticks, but the tick scheduler's ring is 256 buckets with a `u8`
delay, so anything past 255 wrapped. Widened `ScheduledTick::delay` to `u32` and taught
`ChunkTickScheduler` to carry a `remaining` count across ring passes (a tick hop is at most one pass; the
rest is re-queued each pass). This keeps the fixed 256-bucket memory footprint. The existing
`schedule_block_tick` keeps its `u8` signature; a new `schedule_block_tick_long` takes the `u32` delay and
is what frogspawn uses. Two regression tests cover a short delay and a multi-pass delay.

## Open

- `onPlace` is approximated by Pumpkin's `PlacedArgs` (block added), not a true `onPlace` with the previous
  state; fine for frogspawn, which only schedules.
- The task brief mentioned `entityInside` breaking for non-frog entities, but 26.3 only breaks on falling
  blocks; the decompiled class was followed.
- `getShape` (the thin 16x1.5 shape) is not added; Pumpkin's block data does not carry a per-behaviour shape.
- Not tested in game.
