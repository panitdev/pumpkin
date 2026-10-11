# Chiseled bookshelf (vanilla ChiseledBookShelfBlock / ChiseledBookShelfBlockEntity)

## Matched already
- Facing, slot_N_occupied states, comparator output (last interacted slot + 1), hit-face check,
  enchanted-book sound variant, full-inventory fallback drop, contents scattered on break
  (default block entity replacement path), NBT of items and last_interacted_slot.

## Fixed
- Inserting a book played the pickup sound; now plays the insert / insert_enchanted sounds.
- Slot columns used old 0.375/0.6875 cut-offs; now equal thirds (rows halves) like vanilla.
- Removing a book emits the `block_change` game event.
- The "used stat" TODO: the item-used statistic is already awarded for every item use on a block
  in `use_item_on.rs`, so nothing more is needed here.

## Open
- Hopper insertion/extraction: no vanilla-specific slot rules; relies on the generic Inventory impl.
