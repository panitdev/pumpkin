# Farmland (`minecraft:farmland`, vanilla `FarmlandBlock`)

Matched: neighbor-update scheduling a 1-tick check, placement falling back to dirt, pathfinding off.

Fixed:
- `canSurvive` now also passes when the block above is in `#maintains_farmland`.
- Random tick: water check uses the water fluid (so waterlogged blocks count) instead of the
  water block; rain on the block above hydrates; moisture only rises when below 7; dry farmland
  at 0 reverts to dirt unless a maintaining block sits above. Moisture-change plugin event kept.
- Scheduled tick only reverts to dirt when the block can no longer survive (was unconditional).
- Trampling (`fallOn`): living entities landing roll `fall_distance - 0.5`, need spawn protection
  (players) or mobGriefing (others) and `w*w*h > 0.512`. Required adding `position` to
  `OnLandedUponArgs`.
- Reverting to dirt lifts entities in the lost top 1/16 slice up onto the new surface.

Open: `GameEvent.BLOCK_CHANGE` is not emitted on reversion (no equivalent hook used here).
