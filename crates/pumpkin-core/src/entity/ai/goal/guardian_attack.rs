use super::{Controls, Goal};
use crate::entity::mob::Mob;
use pumpkin_data::damage::DamageType;
use pumpkin_data::entity::EntityStatus;
use pumpkin_util::Difficulty;

/// Attack time at which the beam starts, counted up from [`GuardianAttackGoal::start`].
const ATTACK_START_TIME: i32 = -10;
const TOO_CLOSE_DISTANCE_SQ: f64 = 9.0;
const BASE_MAGIC_DAMAGE: f32 = 1.0;
const HARD_MAGIC_DAMAGE_BONUS: f32 = 2.0;
const ELDER_MAGIC_DAMAGE_BONUS: f32 = 2.0;
const LOOK_SPEED: f32 = 90.0;

/// Vanilla `Guardian.GuardianAttackGoal`.
#[derive(Default)]
pub struct GuardianAttackGoal {
    attack_time: i32,
}

impl Goal for GuardianAttackGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        mob.get_mob_entity()
            .get_target()
            .is_some_and(|target| target.get_entity().is_alive())
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        let Some(guardian) = mob.as_guardian() else {
            return false;
        };
        self.can_start(mob)
            && (guardian.is_elder()
                || mob.get_mob_entity().get_target().is_some_and(|target| {
                    mob.get_entity()
                        .pos
                        .load()
                        .squared_distance_to_vec(&target.get_entity().pos.load())
                        > TOO_CLOSE_DISTANCE_SQ
                }))
    }

    fn start(&mut self, mob: &dyn Mob) {
        self.attack_time = ATTACK_START_TIME;
        let mob_entity = mob.get_mob_entity();
        mob_entity
            .navigator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .stop();
        if let Some(target) = mob_entity.get_target() {
            mob_entity
                .look_control
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .look_at_entity_with_range(&target, LOOK_SPEED, LOOK_SPEED);
        }
    }

    fn stop(&mut self, mob: &dyn Mob) {
        if let Some(guardian) = mob.as_guardian() {
            let state = guardian.guardian_state();
            state.set_active_attack_target(mob.get_entity(), 0);
            state.trigger_stroll();
        }
        mob.get_mob_entity().set_target(None);
    }

    fn should_run_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let Some(guardian) = mob.as_guardian() else {
            return;
        };
        let mob_entity = mob.get_mob_entity();
        let Some(target) = mob_entity.get_target() else {
            return;
        };
        let entity = mob.get_entity();

        mob_entity
            .navigator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .stop();
        mob_entity
            .look_control
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .look_at_entity_with_range(&target, LOOK_SPEED, LOOK_SPEED);

        if !mob.has_line_of_sight(target.get_entity()) {
            mob_entity.set_target(None);
            return;
        }

        self.attack_time += 1;
        if self.attack_time == 0 {
            guardian
                .guardian_state()
                .set_active_attack_target(entity, target.get_entity().entity_id);
            if !entity.is_silent() {
                entity.world.load().broadcast_entity_event(
                    entity,
                    EntityStatus::GuardianAttackSound,
                    None,
                );
            }
        } else if self.attack_time >= guardian.attack_duration() {
            let mut magic_damage = BASE_MAGIC_DAMAGE;
            if entity.world.load().level_info.load().difficulty == Difficulty::Hard {
                magic_damage += HARD_MAGIC_DAMAGE_BONUS;
            }
            if guardian.is_elder() {
                magic_damage += ELDER_MAGIC_DAMAGE_BONUS;
            }

            target.damage_with_context(
                target.as_ref(),
                magic_damage,
                DamageType::INDIRECT_MAGIC,
                None,
                Some(mob),
                Some(mob),
            );
            mob_entity.try_attack(mob, target.as_ref());
            mob_entity.set_target(None);
        }
    }

    fn controls(&self) -> Controls {
        Controls::MOVE | Controls::LOOK
    }
}
