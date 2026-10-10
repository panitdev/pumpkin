use std::sync::atomic::{AtomicBool, Ordering};

use pumpkin_data::damage::DamageType;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_world::world::BlockFlags;

use crate::{
    block::blocks::fire::{FireBlockBase, fire::FireBlock},
    entity::{
        Entity, EntityBase,
        projectile::{ProjectileHit, ThrownItemEntity, fireball::INITIAL_ACCELERATION_POWER},
    },
    server::Server,
};

const GRAVITY: f64 = 0.0;

pub struct SmallFireballEntity {
    pub thrown: ThrownItemEntity,
}

impl SmallFireballEntity {
    #[must_use]
    pub const fn new(entity: Entity) -> Self {
        let thrown = ThrownItemEntity {
            entity,
            owner_id: None,
            collides_with_projectiles: false,
            has_hit: AtomicBool::new(false),
            gravity: GRAVITY,
        };

        Self { thrown }
    }

    #[must_use]
    pub fn new_shot(entity: Entity, shooter: &Entity, direction: Vector3<f64>) -> Self {
        let thrown = ThrownItemEntity::new(entity, shooter, GRAVITY);
        let accel = INITIAL_ACCELERATION_POWER;
        thrown
            .entity
            .velocity
            .store(direction.normalize().multiply(accel, accel, accel));
        Self { thrown }
    }
}

impl EntityBase for SmallFireballEntity {
    fn get_owner_id(&self) -> Option<i32> {
        self.thrown.owner_id
    }

    fn tick(&self, caller: &dyn EntityBase, _server: &Server) {
        self.thrown.process_tick(caller);
    }

    fn get_entity(&self) -> &Entity {
        self.thrown.get_entity()
    }

    fn get_living_entity(&self) -> Option<&crate::entity::living::LivingEntity> {
        None
    }
    fn cast_any(&self) -> &dyn std::any::Any {
        self
    }

    fn on_hit(&self, hit: ProjectileHit) {
        let world = self.get_entity().world.load();
        let owner = self
            .thrown
            .owner_id
            .and_then(|id| world.get_entity_by_id(id));

        match hit {
            ProjectileHit::Entity {
                ref entity,
                hit_pos,
                ..
            } => {
                // A hit that deals no damage (fire immunity, invulnerability) leaves the
                // target's burn time as it was.
                let target = entity.get_entity();
                let fire_ticks_before = target.fire_ticks.load(Ordering::Relaxed);
                target.set_on_fire_for(5.0);
                let damaged = entity.damage_with_context(
                    entity.as_ref(),
                    5.0,
                    DamageType::FIREBALL,
                    Some(hit_pos),
                    Some(self),
                    owner.as_deref(),
                );
                if !damaged {
                    target
                        .fire_ticks
                        .store(fire_ticks_before, Ordering::Relaxed);
                }
            }
            ProjectileHit::Block { pos, face, .. } => {
                // Mob-shot fireballs only start fires when mob griefing is on.
                let shot_by_mob = owner.as_ref().is_some_and(|o| o.get_mob().is_some());
                if shot_by_mob && !world.level_info.load().game_rules.mob_griefing {
                    return;
                }

                let fire_pos = pos.offset(face.to_offset());
                if !world.get_block_state(&fire_pos).is_air() {
                    return;
                }
                let fire_block = FireBlockBase::get_fire_type(&world, &fire_pos);
                let fire_state = FireBlock.get_state_for_position(&world, &fire_block, &fire_pos);
                world.set_block_state(&fire_pos, fire_state, BlockFlags::NOTIFY_ALL);
            }
        }
    }
}
