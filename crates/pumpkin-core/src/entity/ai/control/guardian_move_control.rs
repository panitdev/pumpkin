use crate::entity::ai::control::move_control::{MoveControl, Operation};
use crate::entity::ai::control::{Control, MoveControlTrait};
use crate::entity::mob::Mob;
use pumpkin_data::attributes::Attributes;
use pumpkin_util::math::vector3::Vector3;
use std::sync::atomic::Ordering;

/// Vanilla `Guardian.GuardianMoveControl`.
#[derive(Default)]
pub struct GuardianMoveControl {
    inner: MoveControl,
}

impl Control for GuardianMoveControl {}

impl MoveControlTrait for GuardianMoveControl {
    fn tick(&mut self, mob: &dyn Mob) {
        let Some(guardian) = mob.as_guardian() else {
            return;
        };
        let mob_entity = mob.get_mob_entity();
        let living_entity = &mob_entity.living_entity;
        let entity = &living_entity.entity;
        let state = guardian.guardian_state();

        let pos = entity.pos.load();
        let delta = Vector3::new(
            self.inner.wanted_x - pos.x,
            self.inner.wanted_y - pos.y,
            self.inner.wanted_z - pos.z,
        );
        let length = delta.length();
        if self.inner.operation != Operation::MoveTo || mob_entity.steered().navigator().is_done() {
            mob_entity.set_speed(0.0);
            state.set_moving(entity, false);
            return;
        }

        let xd = delta.x / length;
        let yd = delta.y / length;
        let zd = delta.z / length;
        let y_rot_d = (delta.z.atan2(delta.x).to_degrees() as f32) - 90.0;
        let yaw = MoveControl::rotlerp(entity.yaw.load(), y_rot_d, MoveControl::MAX_TURN);
        entity.yaw.store(yaw);
        entity.body_yaw.store(yaw);

        let target_speed = (self.inner.speed_modifier
            * living_entity.get_attribute_value(&Attributes::MOVEMENT_SPEED))
            as f32;
        let speed = living_entity.get_speed();
        let new_speed = speed + (target_speed - speed) * 0.125;
        mob_entity.set_speed(new_speed);

        let ticks = f64::from(entity.tick_count.load(Ordering::Relaxed) + entity.entity_id);
        let push = (ticks * 0.5).sin() * 0.05;
        let yaw_rad = f64::from(yaw.to_radians());
        let cos = yaw_rad.cos();
        let sin = yaw_rad.sin();
        let y_push = (ticks * 0.75).sin() * 0.05;
        let velocity = entity.velocity.load();
        entity.velocity.store(Vector3::new(
            velocity.x + push * cos,
            velocity.y + y_push * (sin + cos) * 0.25 + f64::from(new_speed) * yd * 0.1,
            velocity.z + push * sin,
        ));

        let mut look_control = mob_entity
            .look_control
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let new_look = Vector3::new(
            pos.x + xd * 2.0,
            entity.get_eye_y() + yd / length,
            pos.z + zd * 2.0,
        );
        let old_look = if look_control.is_looking_at_target() {
            look_control.wanted_position()
        } else {
            new_look
        };
        look_control.look_at_with_range(
            old_look.x + (new_look.x - old_look.x) * 0.125,
            old_look.y + (new_look.y - old_look.y) * 0.125,
            old_look.z + (new_look.z - old_look.z) * 0.125,
            10.0,
            40.0,
        );
        drop(look_control);

        state.set_moving(entity, true);
    }

    fn set_wanted_position(&mut self, x: f64, y: f64, z: f64, speed_modifier: f64) {
        self.inner.set_wanted_position(x, y, z, speed_modifier);
    }

    fn strafe(&mut self, forwards: f32, right: f32) {
        self.inner.strafe(forwards, right);
    }

    fn has_wanted(&self) -> bool {
        self.inner.has_wanted()
    }
}
