# Powered / activator rails

Vanilla: `PoweredRailBlock` (shared by both), `AbstractMinecart.activateMinecart`, `NewMinecartBehavior` boost/halt.

## Matched
- Power search (`findPoweredRailSignal`/`isSameRailWithPower`): 8-rail depth limit, same-axis check,
  slope handling, below-check, already-powered rail counted as a source.
- `updateState`: neighbour update below, and above for slopes, on change.
- Minecart on powered rail: +0.06 boost when moving, 0.5x halt (zeroed below 0.03) when unpowered.
- Activator rail: TNT minecart primes, hopper minecart disabled while powered, rideable minecart
  ejects passengers and wobbles.

## Fixed
- The stale "power extends from the wrong rail" TODO was incorrect: vanilla also keeps a whole chain
  lit when a source sits mid-chain, because powered neighbours count as sources. Comment replaced.
- A stationary cart on a powered rail is now pushed away from an adjacent solid block at the rail
  end (`getRedstoneDirection`), instead of along its yaw.

## Open
- Command block minecart (`MinecartCommandBlock.activateMinecart`) does not exist in Pumpkin.
- Pumpkin additionally propagates powered state recursively in the block code; vanilla relies on
  neighbour updates alone. Result is equivalent, only redundant work.
