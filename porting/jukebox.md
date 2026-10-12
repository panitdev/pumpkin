# Port log: Jukebox (vanilla 26.3)

- Tracker issue: `Pumpkin-747.12`; vanilla: `JukeboxBlock`, `JukeboxBlockEntity`, `JukeboxSongPlayer`, `JukeboxPlayable`
- Files: `crates/pumpkin-core/src/block/blocks/jukebox.rs`, `crates/pumpkin-core/src/block/entities/jukebox.rs`

## Already matched
- Record insert via `jukebox_playable`, `play_record` stat, redstone signal 15 while playing,
  comparator output from the song, item drop position with 0.7 horizontal random offset.

## Fixed
- The song player (song, ticks since start) now lives in the block entity, is persisted as
  `ticks_since_song_started` and restored if the song has not finished (length + 20 padding).
  Previously the length was never saved, so a reloaded jukebox stopped signalling.
- The song ends with `JUKEBOX_STOP_PLAY` and level event 1011 (no more stale redstone), plus
  `JUKEBOX_PLAY` and note particles every 20 ticks while playing.
- `setTheItem` flow: block state `has_record`, `BLOCK_CHANGE` game events, play/stop, neighbour
  update on every song change (`onSongChanged`).
- Removal of the block (any cause) pops the record out via `on_block_replaced` and sends stop
  event/game event; comparators are updated after removal.
- Hopper insertion/extraction (`canPlaceItem` needs a playable disc and an empty slot,
  `canTakeItem` needs a free hopper slot, max stack size 1).

- `setPlacedBy`: an item with `RecordItem` block entity data places a jukebox with `has_record`.
- Hopper changes apply the world side synchronously once the block entity knows its world
  (bound on place and first tick); an occupied jukebox pops its disc out instead of being overwritten.
- Note particles only reach players within 32 blocks.
- Plugin `start_playing(length)` honours the length and `stop_playing` applies immediately.

## Open
- Before the first tick after a chunk load, inventory changes are deferred to the next tick.
- Generated `JukeboxSong::length_in_seconds` is truncated to whole seconds (vanilla
  `lengthInTicks` is `ceil(seconds * 20)`), so songs with fractional length end slightly early.
