# Port log: Lodestone Compass binding (vanilla 26.3)

- Tracker issue: `Pumpkin-747.13`
- Vanilla reference: `CompassItem`
- Files: `crates/pumpkin-core/src/item/items/compass.rs`

## Fixed

1. `useOn` now follows `CompassItem.useOn`: clicking a lodestone plays `LODESTONE_COMPASS_LOCK`, then
   splits into vanilla's two cases.
   - A single compass in survival is bound in place (`itemStack.set(LODESTONE_TRACKER, target)`).
   - Otherwise (creative, or a stack larger than one) a fresh compass is created, one item is consumed,
     the tracker is set on the copy, and it is added to the inventory or dropped.
2. `CompassItem::ids` now registers only `minecraft:compass`, matching vanilla (`RECOVERY_COMPASS` is a
   plain item in 26.3, not a `CompassItem`). The old `item.id == COMPASS` guard is gone with it.
3. The tracker component is written with the current dimension and clicked position, `tracked = true`.

## Open

- `CompassItem.inventoryTick` (ticking the tracker when the lodestone is broken/moved) is not ported;
  `ItemBehaviour` has no per-tick inventory hook.
- `CompassItem.isFoil` and the lodestone-compass display name are not ported; there is no item foil or
  name override hook.
- Not tested in game.
