use std::sync::Weak;

use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_util::math::position::BlockPos;
use pumpkin_world::chunk::ChunkHeightmapType;
use rand::RngExt;

use super::{Goal, to_goal_ticks};
use crate::entity::ai::target_predicate::TargetPredicate;
use crate::entity::mob::Mob;
use crate::entity::mob::phantom::{AttackPhase, PhantomEntity};

/// Vanilla `Phantom.PhantomAttackStrategyGoal`: circles the anchor, then swoops.
pub struct PhantomAttackStrategyGoal {
    phantom: Weak<PhantomEntity>,
    next_sweep_tick: i32,
}

impl PhantomAttackStrategyGoal {
    #[must_use]
    pub const fn new(phantom: Weak<PhantomEntity>) -> Self {
        Self {
            phantom,
            next_sweep_tick: 0,
        }
    }

    /// Vanilla `setAnchorAboveTarget`: anchor 20-39 blocks above the target, never below sea level.
    fn set_anchor_above_target(&self, mob: &dyn Mob) {
        let Some(phantom) = self.phantom.upgrade() else {
            return;
        };
        let Some(target) = mob.get_mob_entity().get_target() else {
            return;
        };

        let mut anchor = phantom
            .anchor_point
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if anchor.is_none() {
            return;
        }

        let target_pos = target.get_entity().block_pos.load();
        let world = mob.get_entity().world.load();
        let mut y = target_pos.0.y + 20 + mob.get_random().random_range(0..20);
        if y < world.sea_level {
            y = world.sea_level + 1;
        }
        *anchor = Some(BlockPos::new(target_pos.0.x, y, target_pos.0.z));
    }
}

impl Goal for PhantomAttackStrategyGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        if self.phantom.upgrade().is_none() {
            return false;
        }
        let Some(target) = mob.get_mob_entity().get_target() else {
            return false;
        };
        TargetPredicate::create_attackable().test(
            &mob.get_entity().world.load(),
            Some(mob),
            target.as_ref(),
        )
    }

    fn start(&mut self, mob: &dyn Mob) {
        self.next_sweep_tick = to_goal_ticks(10);
        if let Some(phantom) = self.phantom.upgrade() {
            phantom.attack_phase.store(AttackPhase::Circle);
        }
        self.set_anchor_above_target(mob);
    }

    fn stop(&mut self, mob: &dyn Mob) {
        let Some(phantom) = self.phantom.upgrade() else {
            return;
        };
        let world = mob.get_entity().world.load();
        let mut anchor = phantom
            .anchor_point
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(current) = *anchor {
            // `getHeightmapPos(...).getY()` is the first available block: heightmap height + 1.
            let height = world.get_heightmap_height(
                ChunkHeightmapType::MotionBlocking,
                current.0.x,
                current.0.z,
            ) + 1;
            let y = height + 10 + mob.get_random().random_range(0..20);
            *anchor = Some(BlockPos::new(current.0.x, y, current.0.z));
        }
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let Some(phantom) = self.phantom.upgrade() else {
            return;
        };
        if phantom.attack_phase.load() != AttackPhase::Circle {
            return;
        }

        self.next_sweep_tick -= 1;
        if self.next_sweep_tick > 0 {
            return;
        }

        phantom.attack_phase.store(AttackPhase::Swoop);
        self.set_anchor_above_target(mob);
        self.next_sweep_tick = to_goal_ticks((8 + mob.get_random().random_range(0..4)) * 20);

        let entity = mob.get_entity();
        if !entity.is_silent() {
            entity.world.load().play_sound_fine(
                Sound::EntityPhantomSwoop,
                SoundCategory::Hostile,
                &entity.pos.load(),
                10.0,
                0.95 + mob.get_random().random::<f32>() * 0.1,
            );
        }
    }
}
