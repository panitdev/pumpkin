use std::sync::{Arc, Weak};

use super::{Goal, to_goal_ticks};
use crate::entity::EntityBase;
use crate::entity::ai::target_predicate::TargetPredicate;
use crate::entity::mob::Mob;
use crate::entity::mob::phantom::PhantomEntity;

/// Vanilla `Phantom.PhantomAttackPlayerTargetGoal`: scans for players in a tall box and
/// picks the highest one, every 20 then 60 reduced ticks.
pub struct PhantomAttackPlayerTargetGoal {
    phantom: Weak<PhantomEntity>,
    next_scan_tick: i32,
}

impl PhantomAttackPlayerTargetGoal {
    #[must_use]
    pub const fn new(phantom: Weak<PhantomEntity>) -> Self {
        Self {
            phantom,
            next_scan_tick: to_goal_ticks(20),
        }
    }
}

impl Goal for PhantomAttackPlayerTargetGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        if self.phantom.upgrade().is_none() {
            return false;
        }

        if self.next_scan_tick > 0 {
            self.next_scan_tick -= 1;
            return false;
        }
        self.next_scan_tick = to_goal_ticks(60);

        let entity = mob.get_entity();
        let world = entity.world.load();
        let search_box = entity.bounding_box.load().expand(16.0, 64.0, 16.0);

        let mut players = world.get_players_at_box(&search_box);
        players.sort_by(|a, b| {
            b.get_entity()
                .pos
                .load()
                .y
                .total_cmp(&a.get_entity().pos.load().y)
        });

        let predicate = TargetPredicate::create_attackable().set_base_max_distance(64.0);
        let attack_predicate = TargetPredicate::create_attackable();
        for player in players {
            if predicate.test(&world, Some(mob), player.as_ref())
                && attack_predicate.test(&world, Some(mob), player.as_ref())
            {
                mob.set_mob_target(Some(Arc::clone(&player) as Arc<dyn EntityBase>));
                return true;
            }
        }
        false
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        let Some(target) = mob.get_mob_entity().get_target() else {
            return false;
        };
        TargetPredicate::create_attackable().test(
            &mob.get_entity().world.load(),
            Some(mob),
            target.as_ref(),
        )
    }
}
