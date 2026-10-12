use std::sync::Weak;
use std::sync::atomic::Ordering::{Relaxed, SeqCst};

use pumpkin_util::math::vector3::Vector3;
use pumpkin_util::math::{approach, approach_degrees, degrees_difference_abs, wrap_degrees};

use crate::entity::ai::control::{Control, MoveControlTrait};
use crate::entity::mob::Mob;
use crate::entity::mob::phantom::PhantomEntity;

/// Vanilla `Phantom.PhantomMoveControl`. Steers the phantom toward its `move_target_point`
/// by writing its velocity and rotation directly; `travel` then moves it.
pub struct PhantomMoveControl {
    phantom: Weak<PhantomEntity>,
    speed: f32,
}

impl PhantomMoveControl {
    #[must_use]
    pub const fn new(phantom: Weak<PhantomEntity>) -> Self {
        Self {
            phantom,
            speed: 0.1,
        }
    }
}

impl Control for PhantomMoveControl {}

impl MoveControlTrait for PhantomMoveControl {
    fn tick(&mut self, _mob: &dyn Mob) {
        let Some(phantom) = self.phantom.upgrade() else {
            return;
        };
        let entity = &phantom.mob_entity.living_entity.entity;

        if entity.horizontal_collision.load(Relaxed) {
            entity.yaw.store(entity.yaw.load() + 180.0);
            self.speed = 0.1;
        }

        let target = phantom.move_target_point.load();
        let pos = entity.pos.load();
        let mut tdx = target.x - pos.x;
        let tdy = target.y - pos.y;
        let mut tdz = target.z - pos.z;
        let mut sd = tdx.hypot(tdz);
        if sd.abs() <= 1.0E-5 {
            return;
        }

        let y_relative_scale = 1.0 - (tdy * 0.7).abs() / sd;
        tdx *= y_relative_scale;
        tdz *= y_relative_scale;
        sd = tdx.hypot(tdz);
        let sd2 = (tdx * tdx + tdz * tdz + tdy * tdy).sqrt();

        let prev = entity.yaw.load();
        let angle = (tdz.atan2(tdx) as f32).to_degrees();
        let a = wrap_degrees(entity.yaw.load() + 90.0);
        let b = wrap_degrees(angle);
        entity.yaw.store(approach_degrees(a, b, 4.0) - 90.0);
        entity.body_yaw.store(entity.yaw.load());
        entity.head_yaw.store(entity.yaw.load());

        if degrees_difference_abs(prev, entity.yaw.load()) < 3.0 {
            self.speed = approach(self.speed, 1.8, 0.005 * (1.8 / self.speed));
        } else {
            self.speed = approach(self.speed, 0.2, 0.025);
        }

        let x_rot = -((-tdy).atan2(sd).to_degrees() as f32);
        entity.set_pitch(x_rot);

        let move_angle = entity.yaw.load() + 90.0;
        let cos = (move_angle * (std::f32::consts::PI / 180.0)).cos();
        let sin = (move_angle * (std::f32::consts::PI / 180.0)).sin();
        let txd = f64::from(self.speed * cos) * (tdx / sd2).abs();
        let tzd = f64::from(self.speed * sin) * (tdz / sd2).abs();
        let tyd = f64::from(self.speed * (x_rot * (std::f32::consts::PI / 180.0)).sin())
            * (tdy / sd2).abs();

        let movement = entity.velocity.load();
        let delta = Vector3::new(txd, tyd, tzd) - movement;
        entity.velocity.store(movement + delta * 0.2);
        entity.velocity_dirty.store(true, SeqCst);
    }
}
