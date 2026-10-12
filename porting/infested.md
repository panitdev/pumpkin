# Port log: Infested Blocks (vanilla 26.3)

- Tracker issue: `Pumpkin-747.11`
- Vanilla reference: `InfestedBlock`, `BlockStateBase.spawnAfterBreak`, `BlockBehaviour.onExplosionHit`
- Files: `crates/pumpkin-core/src/block/blocks/infested.rs`, `crates/pumpkin-core/src/block/mod.rs`,
  `crates/pumpkin-core/src/world/explosion.rs`

## Fixed

1. Removed the TODO and the gamemode-only shortcut. The player path now mirrors `spawnAfterBreak` reached
   through `playerDestroy`: it skips creative (`preventsBlockDrops`), skips when the held tool cannot
   harvest the block (`hasCorrectToolForDrops`), skips when `BLOCK_DROPS` is off, and skips when the tool
   has an enchantment in `EnchantmentTags.PREVENTS_INFESTED_SPAWNS` (silk touch).
2. Silverfish now spawn at the block centre (`x + 0.5`, `y`, `z + 0.5`) like `spawnInfestation`.
3. Explosion handling: vanilla `onExplosionHit` calls `spawnAfterBreak` with an empty tool before the
   drops. Added a `BlockBehaviour::spawn_after_break` hook, called from the explosion destroy path next
   to `drop_loot` (gated the same way as `dropFromExplosion`), and implemented it for infested blocks.

## Open

- `Silverfish.spawnAnim()` particles after spawning are not sent; spawn reason `TRIGGERED` is not modelled.
- `spawnAfterBreak` on the generic `Block.dropResources` callers (other than explosion and player break)
  is not wired up; only the two 26.3 paths that matter for this block are.
- Not tested in game.
