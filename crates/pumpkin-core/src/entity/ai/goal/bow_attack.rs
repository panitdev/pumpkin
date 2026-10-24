use pumpkin_data::data_component_impl::EquipmentSlot;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_util::Hand;
use rand::RngExt;
use std::sync::Weak;

use crate::entity::ai::goal::{Controls, Goal};
use crate::entity::ai::pathfinder::NavigatorGoal;
use crate::entity::mob::{Mob, RangedAttackMob};
use crate::item::items::bow::BowItem;

/// Ranged bow attack used by skeletons and their variants.
/// Mirrors vanilla `RangedBowAttackGoal`: the mob keeps its distance, draws the bow
/// and releases an arrow once it has been drawn long enough.
pub struct BowAttackGoal {
    mob: Weak<dyn RangedAttackMob>,
    goal_control: Controls,
    speed: f64,
    attack_interval: i32,
    squared_range: f64,
    attack_time: i32,
    see_time: i32,
    strafing_clockwise: bool,
    strafing_backwards: bool,
    strafing_time: i32,
    draw_ticks: i32,
    drawing: bool,
}

impl BowAttackGoal {
    /// Ticks the bow has to be drawn before the arrow is released.
    const DRAW_TIME: i32 = 20;

    #[must_use]
    pub fn new(
        mob: Weak<dyn RangedAttackMob>,
        speed: f64,
        attack_interval: i32,
        range: f32,
    ) -> Self {
        Self {
            mob,
            goal_control: Controls::MOVE | Controls::LOOK,
            speed,
            attack_interval,
            squared_range: f64::from(range * range),
            attack_time: -1,
            see_time: 0,
            strafing_clockwise: false,
            strafing_backwards: false,
            strafing_time: -1,
            draw_ticks: 0,
            drawing: false,
        }
    }

    fn hand_item(mob: &dyn Mob, slot: &EquipmentSlot) -> ItemStack {
        mob.get_mob_entity()
            .living_entity
            .entity_equipment
            .try_lock()
            .map_or_else(|_| ItemStack::EMPTY.clone(), |eq| eq.get(slot))
    }

    fn is_holding_bow(mob: &dyn Mob) -> bool {
        Self::hand_item(mob, &EquipmentSlot::MAIN_HAND).item.id == Item::BOW.id
            || Self::hand_item(mob, &EquipmentSlot::OFF_HAND).item.id == Item::BOW.id
    }

    /// Vanilla `ProjectileUtil.getWeaponHoldingHand` for the bow.
    fn weapon_holding_hand(mob: &dyn Mob) -> (Hand, ItemStack) {
        let main_hand = Self::hand_item(mob, &EquipmentSlot::MAIN_HAND);
        if main_hand.item.id == Item::BOW.id {
            (Hand::Right, main_hand)
        } else {
            (Hand::Left, Self::hand_item(mob, &EquipmentSlot::OFF_HAND))
        }
    }

    fn stop_drawing(&mut self, mob: &dyn Mob) {
        if self.drawing {
            mob.get_mob_entity().living_entity.clear_active_hand();
            self.drawing = false;
            self.draw_ticks = 0;
        }
    }
}

impl Goal for BowAttackGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        mob.get_mob_entity().get_target().is_some() && Self::is_holding_bow(mob)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        let navigating = !mob.is_navigator_idle();
        (self.can_start(mob) || navigating) && Self::is_holding_bow(mob)
    }

    fn start(&mut self, mob: &dyn Mob) {
        mob.get_mob_entity().set_attacking(true);
    }

    fn stop(&mut self, mob: &dyn Mob) {
        mob.get_mob_entity().set_attacking(false);
        self.see_time = 0;
        self.attack_time = -1;
        self.stop_drawing(mob);
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let target = mob.get_mob_entity().get_target().clone();
        let Some(target) = target else {
            return;
        };

        let mob_pos = mob.get_entity().pos.load();
        let target_pos = target.get_entity().pos.load();
        let distance_sq = mob_pos.squared_distance_to_vec(&target_pos);

        let has_line_of_sight = mob.has_line_of_sight(target.get_entity());
        if has_line_of_sight != (self.see_time > 0) {
            self.see_time = 0;
        }
        if has_line_of_sight {
            self.see_time += 1;
        } else {
            self.see_time -= 1;
        }

        {
            let steered = mob.get_mob_entity().steered();
            let mut navigator = steered.navigator();
            if distance_sq <= self.squared_range && self.see_time >= 20 {
                navigator.stop();
                self.strafing_time += 1;
            } else {
                navigator.set_progress(NavigatorGoal {
                    current_progress: mob_pos,
                    destination: target_pos,
                    speed: self.speed,
                });
                self.strafing_time = -1;
            }
        }

        if self.strafing_time >= 20 {
            if mob.get_random().random::<f32>() < 0.3 {
                self.strafing_clockwise = !self.strafing_clockwise;
            }
            if mob.get_random().random::<f32>() < 0.3 {
                self.strafing_backwards = !self.strafing_backwards;
            }
            self.strafing_time = 0;
        }

        if self.strafing_time > -1 {
            if distance_sq > self.squared_range * 0.75 {
                self.strafing_backwards = false;
            } else if distance_sq < self.squared_range * 0.25 {
                self.strafing_backwards = true;
            }
            mob.get_mob_entity().steered().move_control().strafe(
                if self.strafing_backwards { -0.5 } else { 0.5 },
                if self.strafing_clockwise { 0.5 } else { -0.5 },
            );
        }

        // Always through the look control, which smooths towards the same angle.
        mob.get_mob_entity()
            .look_control
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .look_at_entity_with_range(&target, 30.0, 30.0);

        if self.drawing {
            if !has_line_of_sight && self.see_time < -60 {
                self.stop_drawing(mob);
            } else if has_line_of_sight {
                self.draw_ticks += 1;
                if self.draw_ticks >= Self::DRAW_TIME {
                    let power = BowItem::get_power_for_time(self.draw_ticks);
                    self.stop_drawing(mob);
                    if let Some(ranged) = self.mob.upgrade() {
                        ranged.perform_ranged_attack(&target, power);
                    }
                    self.attack_time = self.attack_interval;
                }
            }
        } else {
            self.attack_time -= 1;
            if self.attack_time <= 0 && self.see_time >= -60 {
                let (hand, stack) = Self::weapon_holding_hand(mob);
                mob.get_mob_entity()
                    .living_entity
                    .set_active_hand(hand, stack, i32::MAX);
                self.drawing = true;
                self.draw_ticks = 0;
            }
        }
    }

    fn should_run_every_tick(&self) -> bool {
        true
    }

    fn controls(&self) -> Controls {
        self.goal_control
    }
}
