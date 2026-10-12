use super::{Controls, Goal};
use crate::entity::ai::goal::wander_around::WanderAroundGoal;
use crate::entity::mob::Mob;

/// Vanilla `RandomSwimmingGoal`, including the `canRandomSwim` gate that
/// `AbstractFish.FishSwimGoal` adds on top of it.
pub struct RandomSwimmingGoal {
    inner: WanderAroundGoal,
}

impl RandomSwimmingGoal {
    #[must_use]
    pub fn new(speed: f64, interval: i32) -> Self {
        Self {
            inner: WanderAroundGoal::swimming(speed, interval),
        }
    }
}

impl Goal for RandomSwimmingGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        // Vanilla `FishSwimGoal.canUse`: a schooling fish that follows a leader does not swim off
        // on its own.
        mob.as_schooling_fish()
            .is_none_or(|fish| !fish.is_follower())
            && self.inner.can_start(mob)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        self.inner.should_continue(mob)
    }

    fn start(&mut self, mob: &dyn Mob) {
        self.inner.start(mob);
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.inner.stop(mob);
    }

    fn tick(&mut self, mob: &dyn Mob) {
        self.inner.tick(mob);
    }

    fn controls(&self) -> Controls {
        self.inner.controls()
    }
}
