use crate::entity::ai::control::move_control::Operation;
use crate::entity::ai::control::{Control, MoveControlTrait};
use crate::entity::mob::Mob;
use pumpkin_util::math::vector3::Vector3;

/// Vanilla `Vex.VexMoveControl`.
///
/// Unlike `MoveControl`, the wanted position stays set until the mob is within its own size, and
/// movement is applied straight to the delta movement instead of the movement input.
#[derive(Default)]
pub struct VexMoveControl {
    wanted_x: f64,
    wanted_y: f64,
    wanted_z: f64,
    speed_modifier: f64,
    operation: Operation,
}

impl Control for VexMoveControl {}

impl MoveControlTrait for VexMoveControl {
    fn tick(&mut self, mob: &dyn Mob) {
        if self.operation != Operation::MoveTo {
            return;
        }

        let entity = mob.get_entity();
        let pos = entity.pos.load();
        let delta = Vector3::new(
            self.wanted_x - pos.x,
            self.wanted_y - pos.y,
            self.wanted_z - pos.z,
        );
        let delta_length = delta.length();

        if delta_length < entity.bounding_box.load().get_average_side_length() {
            self.operation = Operation::Wait;
            let velocity = entity.velocity.load();
            entity.velocity.store(velocity * 0.5);
            return;
        }

        let velocity = entity.velocity.load();
        entity
            .velocity
            .store(velocity.add(&(delta * (self.speed_modifier * 0.05 / delta_length))));

        let yaw = mob.get_mob_entity().get_target().map_or_else(
            || {
                let movement = entity.velocity.load();
                -(movement.x.atan2(movement.z) as f32).to_degrees()
            },
            |target| {
                let target_pos = target.get_entity().pos.load();
                -((target_pos.x - pos.x).atan2(target_pos.z - pos.z) as f32).to_degrees()
            },
        );
        entity.yaw.store(yaw);
        entity.body_yaw.store(yaw);
    }

    fn set_wanted_position(&mut self, x: f64, y: f64, z: f64, speed_modifier: f64) {
        self.wanted_x = x;
        self.wanted_y = y;
        self.wanted_z = z;
        self.speed_modifier = speed_modifier;
        self.operation = Operation::MoveTo;
    }

    fn has_wanted(&self) -> bool {
        self.operation == Operation::MoveTo
    }
}
