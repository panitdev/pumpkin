# Port log: Mace smash attack (vanilla 26.3)

- Tracker issue: `Pumpkin-cx6.2`
- Vanilla reference: `MaceItem`, `Player.attack`
- Files: `crates/pumpkin-core/src/item/items/mace.rs`, `entity/player.rs` (`attack`), `entity/combat.rs`

## Fixed

1. Bonus damage follows `getAttackDamageBonus`: 4 per block up to 3, then 12 + 2 per block up to 8, then
   22 + 1 per block, plus the Density effect per fallen block times the fall distance. The old code added a
   flat 1.5 per block. The bonus is added before the critical multiplier, as in vanilla.
2. `canSmashAttack` is `fallDistance > 1.5 && !fallFlying`, shared by the damage, hurt and post-hurt hooks.
3. `hurtEnemy`: attacker's vertical velocity is set to 0.01 and synced, smash sound chosen by whether the
   victim is on the ground (heavy above 5 blocks) or in the air, played at the attacker; level event 2013 is
   sent at the victim's feet; nearby living entities within 3.5 blocks are pushed with the vanilla
   predicate (spectators, allies, pets owned by the hit entity, marker armor stands, flying creative players skipped) and
   a power scaled by distance, heavy fall and knockback resistance.
4. `postHurtEnemy` resets the fall distance.
5. The normal hit sound (crit/strong/weak) is played alongside the mace sound, and a smash can be a critical.

## Open

- `setIgnoreFallDamageFromCurrentImpulse` is not modelled, so the attacker still takes fall damage on landing.
- The landing block particles (`ServerPlayer.checkFallDamage`) are sent from `Player::spawn_extra_particles_on_landing`.
- A sprinting smash does not get the sprint knockback bonus (`AttackType` is exclusive), and the critical
  check (`Player::can_critical_attack`, also used for normal crits) skips the trapdoor-as-ladder case.
- Wind Burst keeps its previous velocity handling.
