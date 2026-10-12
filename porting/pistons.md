# Port log: Pistons (vanilla 26.3)

- Tracker issue: `Pumpkin-747.21` (Blocks / Pistons)
- Pumpkin base: `74a02363e`, `CURRENT_MC_VERSION = 26.3`
- Vanilla reference: 26.3 server jar, package `world.level.block.piston`: `PistonBaseBlock`,
  `PistonStructureResolver`, `PistonHeadBlock`, `MovingPistonBlock`, `PistonMovingBlockEntity`
- Setup: see [README.md](README.md). Only the `piston/` package needs decompiling (~1.2k lines).

## Files touched

| File | Change |
|---|---|
| `crates/pumpkin-core/src/block/blocks/piston/piston.rs` | world-bound push checks, retract/pull logic, short-pulse block dropping, `onPlace` check, game events |
| `crates/pumpkin-core/src/block/blocks/piston/mod.rs` | structure resolver passes world + position to the pushability check |
| `crates/pumpkin-core/src/block/blocks/piston/piston_head.rs` | survival check, update forwarding to the base, base drops when the head is mined |
| `crates/pumpkin-core/src/block/entities/piston.rs` | `finalTick`/`tick` placement, `blockState` NBT, `lastTicked`, slime bounce, honey carrying |
| `crates/pumpkin-core/src/world/mod.rs` | `handling_tick` flag (vanilla `ServerLevel.isHandlingTick`) around scheduled block/fluid ticks |

## What already matched

- Structure resolver (`PistonStructureResolver`): 12-block limit, sticky-line walk, collision
  reorder, branching for slime/honey, honey/slime not sticking to each other, popped blocks.
  Pumpkin's extra "head position" guard is harmless (the head is air at that point).
- Pushability rules other than world bounds: extended pistons immovable, hardness −1, push
  reaction (`Block`/`Destroy`/`PushOnly`), block entities immovable.
- Extension: moving-piston placement order, head block entity, `deleteAfterMove` cleanup and the
  neighbour/shape update order. Sound pitches match.
- Power check (`getNeighborSignal`) including quasi-connectivity from the block above.

## Gaps found and fixed

1. **Sticky pistons facing north never pulled.** Retraction checked `data == 2` (the facing,
   where 2 = north) instead of the event type. Pulling now follows the event type: only a normal
   retract (type 1) pulls.
2. **Short-pulse block dropping was missing.** When a sticky piston retracts while the block it
   pushed is still moving, vanilla snaps that block into its final spot and leaves it there
   ("block spitting"). Pumpkin ignored it, so the block was pulled back or vanished. Added.
3. **Type-2 (drop) detection incomplete.** Vanilla also sends type 2 if the moving block was
   ticked this game tick or the retract happens during scheduled ticks; and it reads the
   previous-tick progress, not the current one. Added `last_ticked` on the block entity and a
   world `handling_tick` flag. A non-piston block entity in front no longer aborts the retract.
4. **Blocks could be pushed out of the world.** Vanilla refuses positions outside the build
   height or world border, and pushing down from the bottom row or up from the top row.
5. **Pull check used the wrong direction.** It OR'd two pushability checks; now uses vanilla's
   single check (pull direction, piston facing as connection).
6. **Head removed too early on retract.** Pumpkin removed the head with a full neighbour update
   before the moving base existed. Now follows vanilla: the base becomes a moving piston first,
   the head is removed by the move or by the "nothing to pull" branch.
7. **Placed check.** Vanilla `onPlace` re-checks power only when the *block* changed and no
   block entity is left; Pumpkin re-checked on every state change.
8. **Landing blocks that can't stay.** A pushed block whose shape update turns it into air (e.g.
   a torch losing its wall) is now placed and then broken so it drops, instead of being placed
   as-is. Waterlogged blocks lose their water when they land, like vanilla.
9. **Landing blocks didn't get their own neighbour update.** Vanilla calls `neighborChanged` on
   the landed block itself (e.g. a pushed piston re-checks power). Pumpkin only updated the
   neighbours. Same for `finalTick`.
10. **Moving blocks lost on save/reload.** `blockState` wasn't written or read, so a chunk saved
    mid-move turned the block into air. Now saved/loaded (and sent in chunk data).
11. **Piston head.** Replaced the special case (facing up + redstone block above) with vanilla:
    the head forwards neighbour updates to its base, and turns to air when its base is no longer
    a matching extended piston or moving piston.
12. **Mining the head dropped nothing.** Breaking the head broke the base with drops skipped;
    now the base drops in survival and only when it is the matching extended piston.
13. **Slime and honey with entities.** Slime blocks moved by a piston now launch entities along
    the push axis; honey moving sideways carries entities standing on it.
14. **Game events.** `block_activate` / `block_deactivate` on extend/retract (sculk sensors).
15. **Moving blocks were invisible on the client.** The client animates pistons itself from the
    block event. Pumpkin then sent a block entity data packet for each moving block on the next
    block-update flush (vanilla `PistonMovingBlockEntity` has no update packet). That packet
    overwrote the client's moving block entity, and since it had no `blockState` the moving
    block rendered as air until it landed. Added `BlockEntity::sends_update_packet` (vanilla
    `getUpdatePacket() != null`); pistons opt out. Verified in a 26.3 client against upstream
    HEAD at 4 TPS: before, the moved blocks vanish mid-move; after, they slide.

16. **Moving piston block.** Removing it now deletes the extended base without effects (vanilla
    `destroy`, no player-based break); using a moving piston with no block entity clears it
    (`useWithoutItem`); breaking one drops the block it carries (`getDrops`), via a special case in
    `World::break_block`.

17. **Piston head pick-block** (`getCloneItemStack`) gives the matching piston or sticky piston.
    The rest of `PistonHeadBlock` (survival, update forwarding, `playerWillDestroy`, shapes) was
    already matched in item 11/12.

18. **Entity pushing uses real shapes.** `moveCollidedEntities` now uses the carried state's
    collision boxes (a retracting source piston uses its head, short after 25% progress) and each
    box's swept slab (`PistonMath.getMovementArea`), instead of a full cube / 4px plate.
19. **`getNeighborSignal`**: dropped the bogus "block below" check. Vanilla's
    `hasSignal(pos, DOWN)` is evaluated on the piston itself, which is never a conductor or a
    signal source, so it never fires.
20. **Breaking the base** no longer removes the head itself. The old `PistonBlock::broken` did that
    with `break_block(SKIP_DROPS)`, which has no `NOTIFY_LISTENERS`, so clients weren't told: a
    client still animating the extension placed the head itself and kept it until rejoining
    (Pumpkin-747.36). Like vanilla, the head now goes through its `updateShape` survival check, and a
    moving piston that finishes without its base removes the head right away.

## Still open

- **Head removed without a player** (explosion, `/setblock`): vanilla `affectNeighborsAfterRemoval`
  also breaks the base. Not ported: Pumpkin's `on_state_replaced` can't tell creative from
  survival, so hooking it would double-drop the piston on player breaks.

- **Players aren't pushed** by moving blocks (existing choice: client-authoritative movement
  would rubber-band). Slime bounce and honey carrying skip players for the same reason.
- **`preRemoveSideEffects`**: removing a moving piston block (e.g. `/setblock`) doesn't finish the
  move first. `World::set_block_state` runs the block-entity replaced callback after the chunk
  write, so the block is no longer a moving piston when it would call `finalTick`; the only lost
  effect is the moved block briefly landing (and its neighbour update) before being overwritten.
- **Moving piston collision**: `MovingPistonBlock.getCollisionShape` (solid while moving) has no
  block-entity-driven shape hook; entities are only moved, not blocked by it.

- No world-level test harness exists, so behaviour was verified by reading against vanilla. Unit
  tests cover the `blockState` NBT round trip.
