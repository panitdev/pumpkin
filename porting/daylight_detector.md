# Port log: Daylight Detector (vanilla 26.3)

- Tracker issue: `Pumpkin-747.29`; vanilla: `DaylightDetectorBlock`, `DaylightDetectorBlockEntity`
- Files: `crates/pumpkin-core/src/block/blocks/redstone/daylight_detector.rs`,
  `crates/pumpkin-core/src/block/entities/daylight_detector.rs`

## Already matched (no code change needed)
- Signal formula: effective sky brightness, sun angle pull toward 0 / 2pi by 0.2, cosine, round,
  clamp; inverted is `15 - brightness` (covered by existing tests against vanilla outputs).
- Use toggles `inverted` (flags 2), fires `BLOCK_CHANGE`, then recomputes the signal (flags 3);
  gated on build permission.
- Block entity ticks every 20 game ticks and only in dimensions with skylight.
- Emits redstone power with the `power` state value; block entity is added/removed with the block.

## Open
- The `BLOCK_CHANGE` game event carries no source entity / state context (the Pumpkin
  `emit_game_event` helper has none).
