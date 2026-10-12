use super::{Controls, Goal, to_goal_ticks};
use crate::entity::ai::pathfinder::NavigatorGoal;
use crate::entity::ai::util::land_random_pos;
use crate::entity::mob::Mob;
use crate::item::items::spear::SpearItem;
use pumpkin_data::data_component_impl::{EquipmentSlot, KineticWeaponImpl};
use pumpkin_util::Hand;
use pumpkin_util::math::vector3::Vector3;

const MIN_REPOSITION_DISTANCE: f64 = 6.0;
const MAX_REPOSITION_DISTANCE: f64 = 7.0;
const MIN_COOLDOWN_DISTANCE: f64 = 9.0;
const MAX_COOLDOWN_DISTANCE: f64 = 11.0;
const AWAY_VERTICAL_DISTANCE: i32 = 7;
/// Vanilla `MAX_FLEEING_TIME`: `reducedTickDelay(100)`.
const MAX_FLEEING_TIME: i32 = to_goal_ticks(100);

/// Vanilla `SpearUseGoal`: charge at the target with a kinetic weapon held out, then back off.
// TODO: vanilla scales the speed by the root vehicle's `chargeSpeedModifier` (camel husk); mounts are not ported.
pub struct SpearUseGoal {
    state: Option<SpearUseState>,
    speed_modifier_when_charging: f64,
    speed_modifier_when_repositioning: f64,
    approach_distance_sq: f64,
    target_in_range_radius_sq: f64,
    navigating: bool,
}

#[derive(Default)]
struct SpearUseState {
    engage_time: Option<i32>,
    fleeing_time: Option<i32>,
    away_pos: Option<Vector3<f64>>,
    done: bool,
}

impl SpearUseState {
    const fn not_engaged_yet(&self) -> bool {
        self.engage_time.is_none()
    }

    const fn tick_and_check_engagement(&mut self) -> bool {
        if let Some(engage_time) = self.engage_time.as_mut()
            && *engage_time > 0
        {
            *engage_time -= 1;
            return *engage_time == 0;
        }
        false
    }

    const fn tick_and_check_fleeing(&mut self) -> bool {
        if let Some(fleeing_time) = self.fleeing_time.as_mut()
            && *fleeing_time > 0
        {
            *fleeing_time += 1;
            if *fleeing_time > MAX_FLEEING_TIME {
                self.done = true;
                return true;
            }
        }
        false
    }
}

impl SpearUseGoal {
    #[must_use]
    pub fn new(
        speed_modifier_when_charging: f64,
        speed_modifier_when_repositioning: f64,
        approach_distance: f32,
        target_in_range_radius: f32,
    ) -> Box<Self> {
        Box::new(Self {
            state: None,
            speed_modifier_when_charging,
            speed_modifier_when_repositioning,
            approach_distance_sq: f64::from(approach_distance * approach_distance),
            target_in_range_radius_sq: f64::from(target_in_range_radius * target_in_range_radius),
            navigating: false,
        })
    }

    fn kinetic_weapon(mob: &dyn Mob) -> Option<KineticWeaponImpl> {
        mob.get_mob_entity()
            .living_entity
            .entity_equipment
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&EquipmentSlot::MAIN_HAND)
            .get_data_component::<KineticWeaponImpl>()
            .cloned()
    }

    fn able_to_attack(mob: &dyn Mob) -> bool {
        mob.get_mob_entity().get_target().is_some() && Self::kinetic_weapon(mob).is_some()
    }

    fn is_using_item(mob: &dyn Mob) -> bool {
        mob.get_mob_entity()
            .living_entity
            .item_in_use
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some()
    }

    /// Vanilla `KineticWeapon.computeDamageUseDuration` as a goal tick count.
    fn kinetic_weapon_use_duration(mob: &dyn Mob) -> i32 {
        let duration = Self::kinetic_weapon(mob).map_or(0, |weapon| {
            weapon.delay_ticks
                + weapon
                    .damage_conditions
                    .map_or(0, |condition| condition.max_duration_ticks)
        });
        to_goal_ticks(duration)
    }

    /// Vanilla `PathNavigation.moveTo` followed by `isDone`. Paths are built lazily, so "done" is the
    /// navigator's state from the previous request: it has stopped at, or given up on, the destination.
    fn move_to(&mut self, mob: &dyn Mob, destination: Vector3<f64>, speed: f64) -> bool {
        let mut navigator = mob
            .get_mob_entity()
            .navigator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let done = self.navigating && navigator.is_idle();
        navigator.set_progress(NavigatorGoal {
            current_progress: mob.get_entity().pos.load(),
            destination,
            speed,
        });
        self.navigating = true;
        done
    }

    fn tick_state(&mut self, mob: &dyn Mob, state: &mut SpearUseState, target_pos: Vector3<f64>) {
        let target_dist_sqr = mob
            .get_entity()
            .pos
            .load()
            .squared_distance_to_vec(&target_pos);
        let mount_distance = if mob.get_entity().has_vehicle() {
            2.0
        } else {
            0.0
        };

        if state.not_engaged_yet() {
            if target_dist_sqr > self.approach_distance_sq {
                self.move_to(mob, target_pos, self.speed_modifier_when_repositioning);
                return;
            }

            state.engage_time = Some(Self::kinetic_weapon_use_duration(mob));
            let stack = mob
                .get_mob_entity()
                .living_entity
                .entity_equipment
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(&EquipmentSlot::MAIN_HAND);
            mob.get_mob_entity().living_entity.set_active_hand(
                Hand::Right,
                stack,
                SpearItem::USE_DURATION,
            );
        }

        if state.tick_and_check_engagement() {
            Self::stop_using_item(mob);
            let distance = target_dist_sqr.sqrt();
            state.away_pos = land_random_pos::get_pos_away(
                mob,
                (MIN_COOLDOWN_DISTANCE + mount_distance - distance).max(0.0),
                (MAX_COOLDOWN_DISTANCE + mount_distance - distance).max(1.0),
                AWAY_VERTICAL_DISTANCE,
                target_pos,
            );
            state.fleeing_time = Some(1);
        }

        if state.tick_and_check_fleeing() {
            return;
        }

        if let Some(away_pos) = state.away_pos {
            if self.move_to(mob, away_pos, self.speed_modifier_when_repositioning) {
                if state.fleeing_time.is_some_and(|time| time > 0) {
                    state.done = true;
                    return;
                }
                state.away_pos = None;
            }
        } else {
            let navigator_done = self.move_to(mob, target_pos, self.speed_modifier_when_charging);
            if target_dist_sqr < self.target_in_range_radius_sq || navigator_done {
                let distance = target_dist_sqr.sqrt();
                state.away_pos = land_random_pos::get_pos_away(
                    mob,
                    MIN_REPOSITION_DISTANCE + mount_distance - distance,
                    MAX_REPOSITION_DISTANCE + mount_distance - distance,
                    AWAY_VERTICAL_DISTANCE,
                    target_pos,
                );
            }
        }
    }

    fn stop_using_item(mob: &dyn Mob) {
        mob.get_mob_entity().living_entity.clear_active_hand();
    }
}

impl Goal for SpearUseGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        Self::able_to_attack(mob) && !Self::is_using_item(mob)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        self.state.as_ref().is_some_and(|state| !state.done) && Self::able_to_attack(mob)
    }

    fn start(&mut self, mob: &dyn Mob) {
        mob.get_mob_entity().set_attacking(true);
        self.state = Some(SpearUseState::default());
        self.navigating = false;
    }

    fn stop(&mut self, mob: &dyn Mob) {
        mob.get_mob_entity()
            .navigator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .stop();
        mob.get_mob_entity().set_attacking(false);
        self.state = None;
        Self::stop_using_item(mob);
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let Some(target) = mob.get_mob_entity().get_target() else {
            return;
        };
        mob.get_mob_entity()
            .look_control
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .look_at_entity_with_range(&target, 30.0, 30.0);

        let Some(mut state) = self.state.take() else {
            return;
        };
        self.tick_state(mob, &mut state, target.get_entity().pos.load());
        self.state = Some(state);
    }

    fn controls(&self) -> Controls {
        Controls::MOVE | Controls::LOOK
    }
}
