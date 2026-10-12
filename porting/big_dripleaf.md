# Port log: Big Dripleaf and stem (vanilla 26.3)

- Tracker issue: `Pumpkin-747.22`; vanilla: `BigDripleafBlock`, `BigDripleafStemBlock`
- Files: `crates/pumpkin-core/src/block/blocks/plant/big_dripleaf.rs`, `big_dripleaf_stem.rs`

## Already matched
- Placement state (facing from the dripleaf below or opposite the player, waterlogged), survival
  tag check, tilt delays 10/10/100, tilt sounds, converting the lower leaf to a stem on stacking.

## Fixed
- Tilting now runs from entity-inside (`entityInside`), not the step-on callback.
- `onProjectileHit` tilts fully and schedules the reset tick.
- The scheduled tick resets the tilt when the leaf is redstone powered.
- `setTilt` uses flag 2 and emits `BLOCK_CHANGE` when the new tilt causes vibration (not for
  `unstable`) and differs from the old one.
- Tilt sounds play at the block centre instead of its corner.
- Bone meal on leaf and stem (`canGrowInto`, `getTopConnectedBlock`), keeping facing and water.
- Stem `canSurvive` checks below (stem or support tag) and above (stem or leaf); an up/down
  neighbour update schedules a 1-tick check that destroys it, so breaking the top collapses the
  stem like vanilla. The old "turn the stem below into a leaf on break" workaround is removed.

## Open
- Collision/outline shapes per tilt are handled by generated shape data, not checked here.
- `placeWithRandomHeight` (world gen) is not touched.
- Game event is only the plugin-facing generic event, not a vibration source.
