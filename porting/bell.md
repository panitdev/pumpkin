# Port log: Bell (vanilla 26.3)

- Tracker issue: `Pumpkin-747.28`; vanilla: `BellBlock`, `BellBlockEntity`
- Files: `crates/pumpkin-core/src/block/blocks/redstone/bell.rs`, `crates/pumpkin-core/src/block/entities/bell.rs`

## Already matched
- Placement state, survival and attachment handling, `isProperHit` side/height check on use,
  `BELL_RING` stat, plugin `BellRingEvent`.

## Fixed
- Ringing now queues block event 1 (direction data), so clients actually play the swing animation;
  the block entity mirrors `onHit` / `triggerEvent` (tick reset, nearby-entity search cache of 60
  ticks, 48 block radius).
- Sound volume and pitch were swapped (now 2.0 volume, 1.0 pitch).
- `BLOCK_CHANGE` game event emitted on ring.
- Ring direction is the clicked face (any of six) or FACING, as in vanilla.
- Resonation: raiders within 32 blocks after 5 ticks start the resonate sound (previously stubbed
  to never happen); at the end of 40 ticks raiders within 48 blocks get Glowing for 60 ticks.
- Powered edge rings before the state is updated, like `neighborChanged`.
- Explosions with block interaction ring the bell (`onExplosionHit`).

## Open
- `HEARD_BELL_TIME` brain memory is not set (nothing reads it yet; TODO in the block entity).
- Projectile hits ring with the facing direction and skip `isProperHit`: `on_projectile_hit` does
  not pass the hit face.
- Resonation particles are client side in vanilla (sent via the block event), nothing to do here.
