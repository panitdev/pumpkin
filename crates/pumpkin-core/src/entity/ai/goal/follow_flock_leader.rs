use std::sync::{Arc, Weak};

use super::{Controls, Goal, to_goal_ticks};
use crate::entity::EntityBase;
use crate::entity::mob::Mob;
use crate::entity::passive::fish::AbstractSchoolingFish;
use rand::RngExt;

const INTERVAL_TICKS: i32 = 200;
const SEARCH_RADIUS: f64 = 8.0;
const PATH_RECALC_TICKS: i32 = 10;

/// Vanilla `FollowFlockLeaderGoal`: lets fish find a leader and swim after it.
pub struct FollowFlockLeaderGoal {
    mob: Weak<dyn EntityBase>,
    time_to_recalc_path: i32,
    next_start_tick: i32,
}

impl FollowFlockLeaderGoal {
    #[must_use]
    pub fn new(mob: Weak<dyn EntityBase>) -> Self {
        let jitter = rand::rng().random_range(0..200) % 20;
        Self {
            mob,
            time_to_recalc_path: 0,
            next_start_tick: to_goal_ticks(INTERVAL_TICKS + jitter),
        }
    }

    fn next_start_tick(mob: &dyn Mob) -> i32 {
        let jitter = mob.get_random().random_range(0..200) % 20;
        to_goal_ticks(INTERVAL_TICKS + jitter)
    }

    /// Vanilla `AbstractSchoolingFish.addFollowers`.
    fn add_followers(leader: &Arc<dyn EntityBase>, candidates: &[Arc<dyn EntityBase>]) {
        let Some(leader_fish) = leader.get_mob().and_then(Mob::as_schooling_fish) else {
            return;
        };
        let remaining = leader_fish.max_school_size() - leader_fish.school_size();
        if remaining <= 0 {
            return;
        }

        // Vanilla's caller already drops followers before `addFollowers`, and the
        // `limit` there is applied before the `f != this` (leader) filter.
        for candidate in candidates
            .iter()
            .filter(|candidate| {
                candidate
                    .get_mob()
                    .and_then(Mob::as_schooling_fish)
                    .is_some_and(|fish| !fish.is_follower())
            })
            .take(remaining as usize)
        {
            if Arc::ptr_eq(candidate, leader) {
                continue;
            }
            let Some(fish) = candidate.get_mob().and_then(Mob::as_schooling_fish) else {
                continue;
            };
            fish.start_following(Arc::downgrade(leader));
        }
    }
}

impl Goal for FollowFlockLeaderGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(fish) = mob.as_schooling_fish() else {
            return false;
        };
        // A fish with its own followers stays the leader; a follower keeps following.
        if fish.has_followers() {
            return false;
        }
        if fish.is_follower() {
            return true;
        }
        if self.next_start_tick > 0 {
            self.next_start_tick -= 1;
            return false;
        }
        self.next_start_tick = Self::next_start_tick(mob);

        let entity = mob.get_entity();
        let world = entity.world.load();
        let search_box =
            entity
                .bounding_box
                .load()
                .expand(SEARCH_RADIUS, SEARCH_RADIUS, SEARCH_RADIUS);
        let candidates: Vec<Arc<dyn EntityBase>> = world
            .get_all_at_box(&search_box)
            .into_iter()
            .filter(|candidate| candidate.get_entity().entity_type == entity.entity_type)
            .filter(|candidate| {
                candidate
                    .get_mob()
                    .and_then(Mob::as_schooling_fish)
                    .is_some_and(|other| other.can_be_followed() || !other.is_follower())
            })
            .collect();

        let leader = candidates
            .iter()
            .find(|candidate| {
                candidate
                    .get_mob()
                    .and_then(Mob::as_schooling_fish)
                    .is_some_and(AbstractSchoolingFish::can_be_followed)
            })
            .cloned();

        // Vanilla falls back to the mob itself as the leader of a new school.
        let Some(leader) = leader.or_else(|| self.mob.upgrade()) else {
            return false;
        };
        Self::add_followers(&leader, &candidates);
        fish.is_follower()
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        mob.as_schooling_fish()
            .is_some_and(|fish| fish.is_follower() && fish.in_range_of_leader())
    }

    fn start(&mut self, _mob: &dyn Mob) {
        self.time_to_recalc_path = 0;
    }

    fn stop(&mut self, mob: &dyn Mob) {
        if let Some(fish) = mob.as_schooling_fish() {
            fish.stop_following();
        }
    }

    fn tick(&mut self, mob: &dyn Mob) {
        self.time_to_recalc_path -= 1;
        if self.time_to_recalc_path > 0 {
            return;
        }
        self.time_to_recalc_path = self.get_tick_count(PATH_RECALC_TICKS);
        if let Some(fish) = mob.as_schooling_fish() {
            fish.path_to_leader();
        }
    }

    fn controls(&self) -> Controls {
        Controls::empty()
    }
}
