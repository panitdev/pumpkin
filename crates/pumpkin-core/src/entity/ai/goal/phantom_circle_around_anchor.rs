use std::sync::Weak;

use pumpkin_util::math::vector3::Vector3;
use rand::RngExt;

use super::{Controls, Goal, to_goal_ticks};
use crate::entity::mob::Mob;
use crate::entity::mob::phantom::{AttackPhase, PhantomEntity};

/// Vanilla `Phantom.PhantomCircleAroundAnchorGoal`: orbits the anchor with a wandering
/// distance and height until the strategy switches to a swoop.
pub struct PhantomCircleAroundAnchorGoal {
    phantom: Weak<PhantomEntity>,
    angle: f32,
    distance: f32,
    height: f32,
    clockwise: f32,
}

impl PhantomCircleAroundAnchorGoal {
    #[must_use]
    pub const fn new(phantom: Weak<PhantomEntity>) -> Self {
        Self {
            phantom,
            angle: 0.0,
            distance: 0.0,
            height: 0.0,
            clockwise: 0.0,
        }
    }

    /// Vanilla `selectNext`: advance the orbit angle and move the target point around the anchor.
    fn select_next(&mut self, mob: &dyn Mob, phantom: &PhantomEntity) {
        let mut anchor = phantom
            .anchor_point
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if anchor.is_none() {
            *anchor = Some(mob.get_entity().block_pos.load());
        }
        if let Some(anchor) = *anchor {
            self.angle += self.clockwise * 15.0 * (std::f32::consts::PI / 180.0);
            phantom.move_target_point.store(Vector3::new(
                f64::from(anchor.0.x) + f64::from(self.distance) * f64::from(self.angle.cos()),
                f64::from(anchor.0.y) - 4.0 + f64::from(self.height),
                f64::from(anchor.0.z) + f64::from(self.distance) * f64::from(self.angle.sin()),
            ));
        }
    }

    fn touching_target(mob: &dyn Mob, phantom: &PhantomEntity) -> bool {
        let target = phantom.move_target_point.load();
        let pos = mob.get_entity().pos.load();
        target.squared_distance_to_vec(&pos) < 4.0
    }
}

impl Goal for PhantomCircleAroundAnchorGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(phantom) = self.phantom.upgrade() else {
            return false;
        };
        mob.get_mob_entity().get_target().is_none()
            || phantom.attack_phase.load() == AttackPhase::Circle
    }

    fn start(&mut self, mob: &dyn Mob) {
        let Some(phantom) = self.phantom.upgrade() else {
            return;
        };
        let mut rng = mob.get_random();
        self.distance = 5.0 + rng.random_range(0.0..10.0);
        self.height = -4.0 + rng.random_range(0.0..9.0);
        self.clockwise = if rng.random::<bool>() { 1.0 } else { -1.0 };
        self.select_next(mob, &phantom);
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let Some(phantom) = self.phantom.upgrade() else {
            return;
        };

        {
            let mut rng = mob.get_random();
            if rng.random_range(0..to_goal_ticks(350)) == 0 {
                self.height = -4.0 + rng.random_range(0.0..9.0);
            }
            if rng.random_range(0..to_goal_ticks(250)) == 0 {
                self.distance += 1.0;
                if self.distance > 15.0 {
                    self.distance = 5.0;
                    self.clockwise = -self.clockwise;
                }
            }
            if rng.random_range(0..to_goal_ticks(450)) == 0 {
                self.angle = rng.random_range(0.0..1.0) * 2.0 * std::f32::consts::PI;
                self.select_next(mob, &phantom);
            }
        }

        if Self::touching_target(mob, &phantom) {
            self.select_next(mob, &phantom);
        }

        let my_y = mob.get_entity().pos.load().y;
        let block_pos = mob.get_entity().block_pos.load();
        let world = mob.get_entity().world.load();

        if phantom.move_target_point.load().y < my_y
            && !world.get_block_state(&block_pos.down()).is_air()
        {
            self.height = self.height.max(1.0);
            self.select_next(mob, &phantom);
        }
        if phantom.move_target_point.load().y > my_y
            && !world.get_block_state(&block_pos.up()).is_air()
        {
            self.height = self.height.min(-1.0);
            self.select_next(mob, &phantom);
        }
    }

    fn controls(&self) -> Controls {
        Controls::MOVE
    }
}
