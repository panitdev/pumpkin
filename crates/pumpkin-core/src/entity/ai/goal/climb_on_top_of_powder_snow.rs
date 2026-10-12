use std::sync::atomic::Ordering::Relaxed;

use pumpkin_data::{Block, tag, tag::Taggable};

use super::{Controls, Goal};
use crate::entity::mob::Mob;

/// Keeps powder snow walkable mobs jumping while they are inside powder snow so they stay on top.
pub struct ClimbOnTopOfPowderSnowGoal;

impl Goal for ClimbOnTopOfPowderSnowGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let entity = mob.get_entity();
        let in_powder_snow =
            entity.was_in_powder_snow.load(Relaxed) || entity.is_in_powder_snow.load(Relaxed);
        if !in_powder_snow
            || !entity
                .entity_type
                .has_tag(&tag::EntityType::MINECRAFT_POWDER_SNOW_WALKABLE_MOBS)
        {
            return false;
        }

        let above = entity.block_pos.load().up();
        let world = entity.world.load();
        let state = world.get_block_state(&above);
        world.get_block(&above) == &Block::POWDER_SNOW
            || state.get_block_collision_shapes().next().is_none()
    }

    fn tick(&mut self, mob: &dyn Mob) {
        mob.get_mob_entity()
            .living_entity
            .jumping
            .store(true, Relaxed);
    }

    fn should_run_every_tick(&self) -> bool {
        true
    }

    fn controls(&self) -> Controls {
        Controls::JUMP
    }
}
