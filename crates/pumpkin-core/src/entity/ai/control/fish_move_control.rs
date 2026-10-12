use std::sync::atomic::Ordering;

use crate::entity::ai::control::{Control, MoveControlTrait};
use crate::entity::ai::pathfinder::path::Path;
use crate::entity::mob::Mob;
use pumpkin_data::attributes::Attributes;
use pumpkin_util::math::vector3::Vector3;

/// Vanilla `AbstractFish.FishMoveControl`.
///
/// Pumpkin's navigators write the movement input themselves, so the next path node stands in for
/// the wanted position vanilla's `WaterBoundPathNavigation` hands to the move control.
#[derive(Default)]
pub struct FishMoveControl {
    speed: f32,
}

impl Control for FishMoveControl {}

impl MoveControlTrait for FishMoveControl {
    fn tick(&mut self, mob: &dyn Mob) {
        let mob_entity = mob.get_mob_entity();
        let living_entity = &mob_entity.living_entity;
        let entity = &living_entity.entity;

        // Vanilla `isEyeInFluid(WATER)`: gentle buoyancy while the eye is submerged.
        if entity.touching_water.load(Ordering::Relaxed)
            && entity.water_height.load() > f64::from(entity.entity_dimension.load().eye_height)
        {
            let velocity = entity.velocity.load();
            entity
                .velocity
                .store(Vector3::new(velocity.x, velocity.y + 0.005, velocity.z));
        }

        let (wanted, speed_modifier) = {
            let navigator = mob_entity
                .navigator
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if navigator.is_done() {
                (None, 0.0)
            } else {
                (
                    navigator.get_path().and_then(Path::get_next_node_pos),
                    navigator.speed_modifier(),
                )
            }
        };

        let Some(wanted) = wanted else {
            self.speed = 0.0;
            living_entity
                .movement_input
                .store(Vector3::new(0.0, 0.0, 0.0));
            return;
        };

        let pos = entity.pos.load();
        let xd = f64::from(wanted.0.x) + 0.5 - pos.x;
        let yd = f64::from(wanted.0.y) + 0.5 - pos.y;
        let zd = f64::from(wanted.0.z) + 0.5 - pos.z;
        let dd = (xd * xd + yd * yd + zd * zd).sqrt();

        let target_speed = (speed_modifier
            * living_entity.get_attribute_value(&Attributes::MOVEMENT_SPEED))
            as f32;
        // Vanilla `Mth.lerp(0.125F, getSpeed(), targetSpeed)`.
        let speed = 0.125f32.mul_add(target_speed - self.speed, self.speed);
        self.speed = speed;
        // Vanilla `Mob.setSpeed` also sets the forward input `travelInWater` pushes with.
        living_entity
            .movement_input
            .store(Vector3::new(0.0, 0.0, f64::from(speed)));

        if yd != 0.0 && dd > 0.0 {
            let velocity = entity.velocity.load();
            entity.velocity.store(Vector3::new(
                velocity.x,
                velocity.y + f64::from(speed) * (yd / dd) * 0.1,
                velocity.z,
            ));
        }

        if xd != 0.0 || zd != 0.0 {
            let y_rot_d = (zd.atan2(xd).to_degrees() as f32) - 90.0;
            let yaw = self.change_angle(entity.yaw.load(), y_rot_d, 90.0);
            entity.yaw.store(yaw);
            entity.body_yaw.store(yaw);
        }
    }
}
