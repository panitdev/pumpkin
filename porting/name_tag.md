# Port log: Name Tag (vanilla 26.3)

- Tracker issue: `Pumpkin-cx6.4`
- Vanilla reference: `NameTagItem.interactLivingEntity`
- Files: `crates/pumpkin-core/src/item/items/name_tag.rs`

## Fixed

1. Only living entities of a serializable type that are alive are renamed (players are not serializable).
2. Mobs get `persistence_required` set, so a named mob no longer despawns.
3. One item is consumed outside creative, as before.

## Open

- Vanilla returns SUCCESS on the client even when nothing is renamed server-side; Pumpkin's interact path has no
  result type, so the arm swing is unchanged.
- The Dinnerbone, Grumm, jeb_ and Toast names are client rendering and need nothing on the server.
