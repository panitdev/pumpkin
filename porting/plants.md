# Port log: plants & crops sweep (vanilla 26.3)

Issues in this sweep: Dirt Path (`Pumpkin-747.7`), Lily Pad (`Pumpkin-747.23`),
Pumpkin/Melon Stems (`Pumpkin-747.24`), Sea Pickles (`Pumpkin-747.25`), Sugar Cane
(`Pumpkin-747.26`). Vanilla classes: `PathBlock`, `LilyPadBlock`, `StemBlock` +
`AttachedStemBlock`, `SeaPickleBlock`, `SugarCaneBlock` (the issue titles use older
names; 26.3 renamed `DirtPathBlock` -> `PathBlock` and `WaterlilyBlock` -> `LilyPadBlock`).

## Files touched

| File | Change |
|---|---|
| `crates/pumpkin-core/src/block/mod.rs` | added shared `push_entities_up` (vanilla `Block.pushEntitiesUp`) |
| `crates/pumpkin-core/src/block/blocks/dirt_path.rs` | entity push on tick, fence-gate survive |
| `crates/pumpkin-core/src/block/blocks/farmland.rs` | reuse the shared `push_entities_up` |
| `crates/pumpkin-core/src/block/blocks/plant/lily_pad.rs` | fluid-based survival, boat downcast break |
| `crates/pumpkin-core/src/block/blocks/plant/crop/gourds/stem.rs` | random-tick light gate, per-stem fruit-support tag, block flags |
| `crates/pumpkin-core/src/block/blocks/plant/sea_pickles.rs` | bonemeal via `BonemealableBlock`, fixed spread |
| `crates/pumpkin-core/src/block/blocks/plant/sugar_cane.rs` | fluid for adjacent support |

## Dirt path (`PathBlock`)

- Tick no longer just swaps to dirt: it lifts entities standing in the gained top 1/16 slice
  (`Block.pushEntitiesUp`), then sets dirt. `push_entities_up` was extracted from the identical
  farmland code; both are 15/16 -> full-block replacements.
- `canSurvive`: a block above survives if it is not solid **or** is a fence gate.
- Open: `getStateForPlacement` pushes entities in vanilla; `OnPlaceArgs.world` is only `&World`
  here, and teleport needs an `Arc<World>`, so placement does not push.
- Open: vanilla emits `GameEvent.BLOCK_CHANGE` on the swap; Pumpkin has no equivalent hook
  (same gap already noted in `farmland.md`).

## Lily pad (`LilyPadBlock`)

- Survival now checks the fluid, not the block: fluid at the support has
  `#supports_lily_pad`, or the support block does, and the lily pad's own fluid is empty
  (`Fluids.EMPTY`). The old code tested block-vs-fluid-tag and used `is_air`.
- Boat break downcasts the touching entity to `BoatEntity` (vanilla `entity instanceof
  AbstractBoat`). The `c:boats` entity tag was wrong: it misses `poplar_chest_boat`, and no
  vanilla entity tag covers every boat, chest boat and raft.
- Open: `useShapeForLightOcclusion` has no Pumpkin override.

## Pumpkin & melon stems (`StemBlock`, `AttachedStemBlock`)

- Random tick now requires raw brightness >= 9 before rolling growth, matching
  `CropBlock.getGrowthSpeed` gating. Age growth uses `NOTIFY_LISTENERS` (vanilla flag 2); the
  fruit and attached stem use `NOTIFY_ALL` (vanilla `setBlockAndUpdate`), so clients see the
  update. The fruit now sits on the per-stem `supports_pumpkin_stem_fruit` /
  `supports_melon_stem_fruit` tag instead of a hardcoded farmland/dirt check.
- `AttachedStemBlock` needed no change; checked against vanilla.

## Sea pickles (`SeaPickleBlock`)

- Removed the bone-meal logic from `use_with_item` and ported the real `BonemealableBlock`
  path: `isValidBonemealTarget` (alive + coral below) and `performBonemeal`. Bone meal is now
  decremented / particles emitted by the generic item path, as for other crops.
- Fixed the new-pickle spread: it walked a single z-column. Now it mirrors vanilla's diamond
  (`zSpan` 1 -> 3 -> 5 -> 3 -> 1, `zOffset` shifting) and uses `rand::rng()`.
- Open: `updateShape` schedules a water fluid tick when waterlogged; Pumpkin's generic
  waterlogged handling is relied on instead (no explicit schedule added).

## Sugar cane (`SugarCaneBlock`)

- Adjacent support check now tests the fluid at the neighbour for
  `#supports_sugar_cane_adjacently` OR the neighbour block tag (vanilla is an OR over fluid
  state and block state). It previously tested the block against a fluid tag with an AND, so
  it only passed when the block itself was water.
- Open: nothing else; height/age growth and the scheduled-tick break already matched.
