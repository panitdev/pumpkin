use super::{Controls, Goal};
use crate::entity::mob::Mob;
use std::sync::atomic::Ordering;

/// A hurt owner closer than this makes the pet stand up to help.
const OWNER_HURT_RANGE_SQ: f64 = 144.0;

#[derive(Default)]
pub struct SitWhenOrderedToGoal {
    goal_control: Controls,
}

impl SitWhenOrderedToGoal {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            goal_control: Controls::MOVE.union(Controls::JUMP),
        }
    }
}

impl Goal for SitWhenOrderedToGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let ordered_to_sit = mob.is_sit_ordered();
        if !ordered_to_sit && !mob.is_tamed() {
            return false;
        }

        let entity = &mob.get_mob_entity().living_entity.entity;
        if entity.touching_water.load(Ordering::Relaxed) {
            return false;
        }

        if !entity.on_ground.load(Ordering::Relaxed) {
            return false;
        }

        // An owner in another dimension is not in this world, same as having none.
        let world = entity.world.load();
        let Some(owner) = mob
            .get_owner_uuid()
            .and_then(|uuid| world.get_player_by_uuid(uuid))
        else {
            return true;
        };

        let dist_sq = entity
            .pos
            .load()
            .squared_distance_to_vec(&owner.living_entity.entity.pos.load());
        if dist_sq < OWNER_HURT_RANGE_SQ && owner.living_entity.get_last_hurt_by_mob().is_some() {
            return false;
        }

        ordered_to_sit
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        mob.is_sit_ordered()
    }

    fn start(&mut self, mob: &dyn Mob) {
        mob.get_mob_entity()
            .navigator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .stop();
        if let Some(tamable) = mob.as_tamable() {
            tamable.set_in_sitting_pose(true);
        }
    }

    fn stop(&mut self, mob: &dyn Mob) {
        if let Some(tamable) = mob.as_tamable() {
            tamable.set_in_sitting_pose(false);
        }
    }

    fn controls(&self) -> Controls {
        self.goal_control
    }
}
