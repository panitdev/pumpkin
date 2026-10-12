use std::sync::Weak;
use std::sync::atomic::Ordering::Relaxed;

use pumpkin_data::world::WorldEvent;
use pumpkin_util::math::vector3::Vector3;

use super::{Controls, Goal};
use crate::entity::mob::Mob;
use crate::entity::mob::phantom::{AttackPhase, PhantomEntity};
use crate::entity::passive::cat::CatEntity;

/// Vanilla `Phantom.PhantomSweepAttackGoal`: dives at the target, fleeing nearby cats
/// before it commits.
pub struct PhantomSweepAttackGoal {
    phantom: Weak<PhantomEntity>,
    is_scared_of_cat: bool,
    cat_search_tick: i32,
}

impl PhantomSweepAttackGoal {
    #[must_use]
    pub const fn new(phantom: Weak<PhantomEntity>) -> Self {
        Self {
            phantom,
            is_scared_of_cat: false,
            cat_search_tick: 0,
        }
    }
}

impl Goal for PhantomSweepAttackGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(phantom) = self.phantom.upgrade() else {
            return false;
        };
        mob.get_mob_entity().get_target().is_some()
            && phantom.attack_phase.load() == AttackPhase::Swoop
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        let Some(target) = mob.get_mob_entity().get_target() else {
            return false;
        };
        if !target.get_entity().is_alive() {
            return false;
        }
        if target
            .get_player()
            .is_some_and(|player| player.is_spectator() || player.is_creative())
        {
            return false;
        }
        if !self.can_start(mob) {
            return false;
        }

        let tick_count = mob.get_entity().age.load(Relaxed);
        if tick_count > self.cat_search_tick {
            self.cat_search_tick = tick_count + 20;
            let world = mob.get_entity().world.load();
            let search_box = mob
                .get_entity()
                .bounding_box
                .load()
                .expand(16.0, 16.0, 16.0);
            let mut any_cat = false;
            for entity in world.get_entities_at_box(&search_box) {
                if !entity.get_entity().is_alive() {
                    continue;
                }
                if let Some(cat) = entity.cast_any().downcast_ref::<CatEntity>() {
                    cat.hiss();
                    any_cat = true;
                }
            }
            self.is_scared_of_cat = any_cat;
        }

        !self.is_scared_of_cat
    }

    fn stop(&mut self, mob: &dyn Mob) {
        mob.set_mob_target(None);
        if let Some(phantom) = self.phantom.upgrade() {
            phantom.attack_phase.store(AttackPhase::Circle);
        }
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let Some(phantom) = self.phantom.upgrade() else {
            return;
        };
        let Some(target) = mob.get_mob_entity().get_target() else {
            return;
        };
        let target_entity = target.get_entity();
        let target_pos = target_entity.pos.load();
        phantom.move_target_point.store(Vector3::new(
            target_pos.x,
            target_pos.y + f64::from(target_entity.height()) * 0.5,
            target_pos.z,
        ));

        let attack_box = mob.get_entity().bounding_box.load().expand(0.2, 0.2, 0.2);
        if attack_box.intersects(&target_entity.bounding_box.load()) {
            if mob
                .get_mob_entity()
                .try_attack(mob.get_entity(), target.as_ref())
            {
                mob.on_attack(target.as_ref());
            }
            phantom.attack_phase.store(AttackPhase::Circle);
            if !mob.get_entity().is_silent() {
                mob.get_entity().world.load().sync_world_event(
                    WorldEvent::SoundPhantomBite,
                    mob.get_entity().block_pos.load(),
                    0,
                );
            }
        } else if mob.get_entity().horizontal_collision.load(Relaxed)
            || mob
                .get_mob_entity()
                .living_entity
                .hurt_cooldown
                .load(Relaxed)
                > 0
        {
            phantom.attack_phase.store(AttackPhase::Circle);
        }
    }

    fn controls(&self) -> Controls {
        Controls::MOVE
    }
}
