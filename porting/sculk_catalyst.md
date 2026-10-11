# Sculk catalyst (Pumpkin-747.18) - resume note

Status: WIP, `cargo check -p pumpkin` passes. Clippy, fmt and tests not yet run.

## Done
- Block entity (`block/entities/sculk_catalyst.rs`) now owns the pumpkin-world `SculkSpreader` (level preset).
  It ticks cursors, persists them as `cursors` NBT (pos, charge, decay_delay, update_delay, facings) and
  finds the nearest catalyst within 8 blocks of a death (`find_near`).
- `on_entity_die`: adds cursors at death pos + 0.5 up with the mob's xp reward, then blooms
  (bloom=true, tick in 8, sculk_soul particles, bloom sound).
- Block (`blocks/sculk/sculk_catalyst.rs`): scheduled tick clears bloom.
- `blocks/sculk/world_level.rs`: `SculkLevel` adapter over the live World (unloaded = obstacle),
  plays spread/place sounds on placement.
- Hook: `LivingEntity::on_death` (entity/living.rs) - nearest catalyst consumes the xp, so no orb drops.
- pumpkin-world spreader: made `add_cursor` and `cursors()` public.

## Left / next step
1. Run `cargo clippy -p pumpkin-core`, `rustfmt --edition 2024` on touched files, and `cargo test -p pumpkin-core sculk`.
2. Test in game: kill a mob next to a catalyst.
3. Players do not feed catalysts (TODO in living.rs; their xp drop is elsewhere).
4. Missing: KILL_MOB_NEAR_SCULK_CATALYST advancement, xp drop when the catalyst is broken (5 xp),
   level event 3006 particles, pushEntitiesUp on sculk placement.
5. `assets/datapack` in this worktree is a local symlink to the main checkout's datapack, only for building. Not committed.
