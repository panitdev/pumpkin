# Tripwire and tripwire hook (vanilla TripWireBlock / TripWireHookBlock)

## Matched already
- Wire connection logic, scan up to 41 blocks south/west from a wire to its hooks, hook-to-hook
  scan, attached/powered computation, 10-tick recheck, sounds and signal output.

## Fixed
- Wire removal now re-evaluates hooks with the removed wire's own state forced to powered
  (it used the state now at the position, i.e. the replacement block).
- Shearing a wire emits the `shear` game event; no durability is deducted (vanilla doesn't
  either: tripwire has zero hardness).
- Hook: neighbor notifications now update all neighbors of the hook and of its support block
  (previously only the hook itself was poked).
- Hook: placement picks the facing from the clicked face, then the player's looking order, and
  `can_place_at` no longer checks the wrong side of the block.
- Hook: if the start hook disappears while the far hook is updated, run the removal path and stop.
- Hook: attach/detach/activate/deactivate game events emitted.

## Open
- TODO: `Entity.isIgnoringBlockTriggers` / `canAvoidTraps` (e.g. sneaking-irrelevant, but some
  entities such as spectators/projectile-excluded ones) has no Pumpkin equivalent; any entity in
  the box presses the wire.
- Tripwire collision uses the full block box rather than the attached/unattached shape bounds.
