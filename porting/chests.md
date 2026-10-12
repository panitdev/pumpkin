# Port log: Chests (vanilla 26.3)

- Tracker issue: `Pumpkin-747.4`
- Vanilla reference: `ChestBlock`, `TrappedChestBlock`, `CopperChestBlock`, `WeatheringCopperChestBlock`, `ChestBlockEntity`
- Files: `crates/pumpkin-core/src/block/blocks/chests.rs`, `crates/pumpkin-core/src/block/entities/chest_like_block_entity.rs`

## Matched already
- Double chest connection on placement (normal and sneaking via clicked face), loot unpacking, comparator output over both halves.
- Blocked by a conductive block above; trapped chest signal (viewer count, strong power downwards); viewers counted on both halves; copper weathering skips open chests and the RIGHT half.

## Fixed
1. TODO resolved: a sitting cat on top now blocks opening and zeroes the comparator (`isChestBlockedAt`).
2. Copper chests connect across weathering stages and waxing; on placement both halves take the least oxidized block (wax dropped if only one is waxed); the partner adopts the placed block.
3. `updateShape` equivalent: a connected copper half adopts its partner's block (waxing/weathering one half changes both); waterlogged chests schedule a water tick.
4. Breaking one half keeps the partner's own block when it reverts to single.
5. Copper chests play their own open/close sounds per weathering stage.

## Open
- Whether the honeycomb/axe items handle chests was not checked.
- `Containers.updateNeighboursAfterDestroy`, piglin anger on opening and locked-chest notifications are not ported.
