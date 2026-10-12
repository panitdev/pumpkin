# Port log: Note Block (vanilla 26.3)

- Tracker issue: `Pumpkin-747.14`; vanilla: `NoteBlock`, `NoteBlockInstrument`
- Files: `crates/pumpkin-core/src/block/blocks/note.rs`

## Already matched
- Instrument from the block above (mob heads) or below, re-evaluated on vertical neighbour updates.
- Tune on use (25 notes), play on attack, redstone rising edge plays before the state update,
  `TUNE_NOTEBLOCK` / `PLAY_NOTEBLOCK` stats, plugin `NotePlayEvent`.
- Note particle is spawned by the client from the block event.

## Fixed
- Pling and the four trumpets are tunable (`BASE_BLOCK`) in vanilla; they were treated as mob-head
  instruments, so they played at fixed pitch and could not be played with a block above.
- `NOTE_BLOCK_PLAY` game event emitted together with the block event.
- Sound plays at the block centre (was the corner).
- Custom head instrument plays the skull's `note_block_sound`; with none set the event is not
  handled, so there is no sound (was a UI click).
- `useItemOn`: items in `noteblock_top_instruments` clicked on the top face pass through to the
  item (placing a head) instead of tuning the block.

## Open
- The game event carries no source entity (Pumpkin's `emit_game_event` has none).
- Attack handling is hardcoded in `player_action.rs` rather than a block behaviour hook.
