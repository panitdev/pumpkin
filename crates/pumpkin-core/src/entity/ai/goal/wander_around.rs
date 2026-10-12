use super::{Controls, Goal, to_goal_ticks};
use crate::entity::ai::pathfinder::node::PathComputationType;
use crate::entity::ai::util::default_random_pos;
use crate::entity::{ai::pathfinder::NavigatorGoal, mob::Mob};
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use rand::RngExt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub const DEFAULT_INTERVAL: i32 = 120;
/// Vanilla `BehaviorUtils.getRandomSwimmablePos`: how often a candidate is re-rolled.
const SWIM_POS_ATTEMPTS: i32 = 10;

pub struct WanderAroundGoal {
    goal_control: Controls,
    speed: f64,
    target: Option<Vector3<f64>>,
    interval: i32,
    force_trigger: Arc<AtomicBool>,
    swim: bool,
}

impl WanderAroundGoal {
    #[must_use]
    pub fn new(speed: f64) -> Self {
        Self::with_interval(speed, DEFAULT_INTERVAL)
    }

    #[must_use]
    pub fn with_interval(speed: f64, interval: i32) -> Self {
        Self {
            goal_control: Controls::MOVE,
            speed,
            target: None,
            interval,
            force_trigger: Arc::new(AtomicBool::new(false)),
            swim: false,
        }
    }

    /// Vanilla `RandomSwimmingGoal`: only picks positions the mob can swim through.
    #[must_use]
    pub fn swimming(speed: f64, interval: i32) -> Self {
        Self {
            swim: true,
            ..Self::with_interval(speed, interval)
        }
    }

    /// Replaces the controls, e.g. `MOVE | LOOK` for the guardian's `RandomStrollGoal`.
    #[must_use]
    pub const fn with_controls(mut self, controls: Controls) -> Self {
        self.goal_control = controls;
        self
    }

    /// Vanilla `RandomStrollGoal.trigger`: run once on the next check, ignoring the interval.
    pub fn trigger(&self) {
        self.force_trigger.store(true, Ordering::Relaxed);
    }

    /// Shares the trigger flag, so the owning mob can call `trigger` after the goal moved into the selector.
    #[must_use]
    pub fn with_trigger_flag(mut self, flag: Arc<AtomicBool>) -> Self {
        self.force_trigger = flag;
        self
    }

    pub const fn set_interval(&mut self, interval: i32) {
        self.interval = interval;
    }

    fn get_position(&self, mob: &dyn Mob) -> Option<Vector3<f64>> {
        if self.swim {
            Self::get_random_swimmable_pos(mob, 10, 7)
        } else {
            default_random_pos::get_pos(mob, 10, 7)
        }
    }

    /// Vanilla `BehaviorUtils.getRandomSwimmablePos`.
    fn get_random_swimmable_pos(
        mob: &dyn Mob,
        horizontal_dist: i32,
        vertical_dist: i32,
    ) -> Option<Vector3<f64>> {
        let mut target = default_random_pos::get_pos(mob, horizontal_dist, vertical_dist);
        let mut attempts = 0;
        while let Some(pos) = target {
            if Self::is_swimmable(mob, pos) || attempts >= SWIM_POS_ATTEMPTS {
                break;
            }
            attempts += 1;
            target = default_random_pos::get_pos(mob, horizontal_dist, vertical_dist);
        }
        target
    }

    fn is_swimmable(mob: &dyn Mob, pos: Vector3<f64>) -> bool {
        let block_pos = BlockPos::new(
            pos.x.floor() as i32,
            pos.y.floor() as i32,
            pos.z.floor() as i32,
        );
        let world = mob.get_entity().world.load();
        let (block, state) = world.get_block_and_state(&block_pos);
        world
            .block_registry
            .is_pathfindable(block, state, PathComputationType::Water)
    }
}

impl Goal for WanderAroundGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        // Every mob that carries a passenger is controlled by it.
        if mob.get_entity().has_passengers() {
            return false;
        }

        if !self.force_trigger.load(Ordering::Relaxed) {
            // TODO: also bail out on a long no-action time, which needs the despawn bookkeeping.
            if mob
                .get_random()
                .random_range(0..to_goal_ticks(self.interval))
                != 0
            {
                return false;
            }
        }

        self.target = self.get_position(mob);
        if self.target.is_none() {
            return false;
        }
        self.force_trigger.store(false, Ordering::Relaxed);
        true
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        let idle = mob.is_navigator_idle();
        !idle && !mob.get_entity().has_passengers()
    }

    fn start(&mut self, mob: &dyn Mob) {
        if let Some(target) = self.target {
            let pos = mob.get_mob_entity().living_entity.entity.pos.load();
            mob.get_mob_entity()
                .navigator
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .set_progress(NavigatorGoal::new(pos, target, self.speed));
        }
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.target = None;
        mob.get_mob_entity()
            .navigator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .stop();
    }

    fn controls(&self) -> Controls {
        self.goal_control
    }
}
