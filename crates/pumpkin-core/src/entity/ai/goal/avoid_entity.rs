use std::sync::Arc;

use super::{Controls, Goal};
use crate::entity::ai::util::default_random_pos;
use crate::entity::predicate::EntityPredicate;
use crate::entity::{Entity, EntityBase, ai::pathfinder::NavigatorGoal, mob::Mob};
use pumpkin_data::entity::EntityType;
use pumpkin_util::math::vector3::Vector3;

const FAST_DISTANCE_SQ: f64 = 49.0;
const HORIZONTAL_RANGE: i32 = 16;
const VERTICAL_RANGE: i32 = 7;

/// Vanilla's `predicateOnAvoidEntity`: which entities are eligible to be avoided.
#[derive(Clone, Copy, Default)]
pub enum AvoidSelector {
    /// `EntitySelector.NO_CREATIVE_OR_SPECTATOR`, the constructor default.
    #[default]
    ExceptCreativeOrSpectator,
    /// `EntitySelector.NO_SPECTATORS`.
    ExceptSpectator,
}

impl AvoidSelector {
    fn test(self, entity: &Entity) -> bool {
        match self {
            Self::ExceptCreativeOrSpectator => {
                EntityPredicate::ExceptCreativeOrSpectator.test(entity)
            }
            Self::ExceptSpectator => EntityPredicate::ExceptSpectator.test(entity),
        }
    }
}

pub struct AvoidEntityGoal {
    goal_control: Controls,
    flee_type: &'static EntityType,
    flee_distance: f64,
    slow_speed: f64,
    fast_speed: f64,
    target: Option<Arc<dyn EntityBase>>,
    flee_pos: Option<Vector3<f64>>,
    filter: Option<EntityFilter>,
    selector: AvoidSelector,
}

type EntityFilter = Box<dyn Fn(&dyn EntityBase) -> bool + Send + Sync>;

impl AvoidEntityGoal {
    #[must_use]
    pub fn new(
        flee_type: &'static EntityType,
        flee_distance: f64,
        slow_speed: f64,
        fast_speed: f64,
    ) -> Self {
        Self {
            goal_control: Controls::MOVE,
            flee_type,
            flee_distance,
            slow_speed,
            fast_speed,
            target: None,
            flee_pos: None,
            filter: None,
            selector: AvoidSelector::default(),
        }
    }

    /// Vanilla's `avoidPredicate`: only entities passing it are avoided.
    #[must_use]
    pub fn with_filter(
        mut self,
        filter: impl Fn(&dyn EntityBase) -> bool + Send + Sync + 'static,
    ) -> Self {
        self.filter = Some(Box::new(filter));
        self
    }

    /// Overrides the selector vanilla passes as `predicateOnAvoidEntity`.
    #[must_use]
    pub const fn with_selector(mut self, selector: AvoidSelector) -> Self {
        self.selector = selector;
        self
    }

    fn passes_filter(&self, entity: &dyn EntityBase) -> bool {
        self.filter.as_ref().is_none_or(|filter| filter(entity))
    }

    fn find_threat(&self, mob: &dyn Mob) -> Option<Arc<dyn EntityBase>> {
        let entity = &mob.get_mob_entity().living_entity.entity;
        let pos = entity.pos.load();
        let world = entity.world.load();

        if self.flee_type == &EntityType::PLAYER {
            world
                .get_nearest_player(pos, self.flee_distance, |player| {
                    self.selector.test(player.get_entity()) && self.passes_filter(player.as_ref())
                })
                .map(|p| p as Arc<dyn EntityBase>)
        } else {
            world.get_nearest_entity(pos, self.flee_distance, Some(&[self.flee_type]), |entity| {
                self.selector.test(entity.get_entity()) && self.passes_filter(entity.as_ref())
            })
        }
    }
}

impl Goal for AvoidEntityGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(target) = self.find_threat(mob) else {
            return false;
        };

        let threat_pos = target.get_entity().pos.load();
        let Some(flee_pos) =
            default_random_pos::get_pos_away(mob, HORIZONTAL_RANGE, VERTICAL_RANGE, threat_pos)
        else {
            return false;
        };

        // Give up when the escape route does not gain any distance.
        let mob_pos = mob.get_entity().pos.load();
        if threat_pos.squared_distance_to_vec(&flee_pos)
            < threat_pos.squared_distance_to_vec(&mob_pos)
        {
            return false;
        }

        self.target = Some(target);
        self.flee_pos = Some(flee_pos);
        true
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        !mob.is_navigator_idle()
    }

    fn start(&mut self, mob: &dyn Mob) {
        if let Some(flee_pos) = self.flee_pos {
            let mob_pos = mob.get_mob_entity().living_entity.entity.pos.load();
            let mut navigator = mob
                .get_mob_entity()
                .navigator
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            navigator.set_progress(NavigatorGoal::new(mob_pos, flee_pos, self.slow_speed));
        }
    }

    fn tick(&mut self, mob: &dyn Mob) {
        if let Some(target) = &self.target {
            let mob_pos = mob.get_mob_entity().living_entity.entity.pos.load();
            let threat_pos = target.get_entity().pos.load();
            let dist_sq = mob_pos.squared_distance_to_vec(&threat_pos);
            let speed = if dist_sq < FAST_DISTANCE_SQ {
                self.fast_speed
            } else {
                self.slow_speed
            };
            let mut navigator = mob
                .get_mob_entity()
                .navigator
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            navigator.set_speed(speed);
        }
    }

    fn stop(&mut self, _mob: &dyn Mob) {
        self.target = None;
        self.flee_pos = None;
    }

    fn controls(&self) -> Controls {
        self.goal_control
    }
}
