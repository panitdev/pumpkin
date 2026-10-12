use crate::entity::ai::control::{Control, MoveControlTrait};
use crate::entity::mob::Mob;
use pumpkin_data::attributes::Attributes;
use pumpkin_util::math::vector3::Vector3;
use std::sync::atomic::Ordering;

/// Speed modifier of every goal that moves a guardian. The navigator does not expose its own.
const GUARDIAN_SPEED_MODIFIER: f64 = 1.0;

/// Vanilla `Guardian.GuardianMoveControl`.
///
/// Pumpkin's navigators write the movement input themselves, so the next path node stands in for
/// the wanted position vanilla's `WaterBoundPathNavigation` hands to the move control.
#[derive(Default)]
pub struct GuardianMoveControl {
    speed: f32,
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

        let wanted = mob_entity.navigator.try_lock().ok().and_then(|navigator| {
            if navigator.is_done() {
                return None;
            }
            navigator.get_path()?.get_next_node_pos()
        });
        let state = guardian.guardian_state();

        let pos = entity.pos.load();
        let delta = wanted.map(|node| {
            Vector3::new(
                f64::from(node.0.x) + 0.5 - pos.x,
                f64::from(node.0.y) + 0.5 - pos.y,
                f64::from(node.0.z) + 0.5 - pos.z,
            )
        });
        let Some((delta, length)) = delta
            .map(|delta| (delta, delta.length()))
            .filter(|(_, length)| *length > 1.0E-7)
        else {
            self.speed = 0.0;
            living_entity
                .movement_input
                .store(Vector3::new(0.0, 0.0, 0.0));
            state.set_moving(entity, false);
            return;
        };

        let xd = delta.x / length;
        let yd = delta.y / length;
        let zd = delta.z / length;
        let y_rot_d = (delta.z.atan2(delta.x).to_degrees() as f32) - 90.0;
        let yaw = self.change_angle(entity.yaw.load(), y_rot_d, 90.0);
        entity.yaw.store(yaw);
        entity.body_yaw.store(yaw);

        let target_speed = (GUARDIAN_SPEED_MODIFIER
            * living_entity.get_attribute_value(&Attributes::MOVEMENT_SPEED))
            as f32;
        let new_speed = self.speed + (target_speed - self.speed) * 0.125;
        self.speed = new_speed;
        // Vanilla `Mob.setSpeed` also sets the forward input `travelInWater` pushes with.
        living_entity
            .movement_input
            .store(Vector3::new(0.0, 0.0, f64::from(new_speed)));

        let ticks = f64::from(entity.age.load(Ordering::Relaxed) + entity.entity_id);
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
}
