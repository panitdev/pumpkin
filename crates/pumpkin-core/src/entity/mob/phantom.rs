use std::sync::atomic::{AtomicI32, Ordering::Relaxed};
use std::sync::{Arc, Mutex};

use crossbeam::atomic::AtomicCell;
use pumpkin_data::attributes::Attributes;
use pumpkin_data::tracked_data;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;
use pumpkin_util::math::boundingbox::{BoundingBox, EntityDimensions};
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;

use crate::entity::ai::control::look_control::LookControl;
use crate::entity::ai::control::phantom_move_control::PhantomMoveControl;
use crate::entity::ai::control::{Control, LookControlTrait};
use crate::entity::ai::goal::phantom_attack_player_target::PhantomAttackPlayerTargetGoal;
use crate::entity::ai::goal::phantom_attack_strategy::PhantomAttackStrategyGoal;
use crate::entity::ai::goal::phantom_circle_around_anchor::PhantomCircleAroundAnchorGoal;
use crate::entity::ai::goal::phantom_sweep_attack::PhantomSweepAttackGoal;
use crate::entity::mob::spawn::SpawnGroupData;
use crate::entity::{
    Entity,
    mob::{Mob, MobEntity},
};
use crate::world::World;

/// Vanilla `Phantom.ID_SIZE` clamp bound.
const MAX_SIZE: i32 = 64;

/// Vanilla `Phantom.AttackPhase`.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum AttackPhase {
    #[default]
    Circle,
    Swoop,
}

pub struct PhantomEntity {
    pub mob_entity: MobEntity,
    /// Vanilla `moveTargetPoint`: the point the move control steers toward.
    pub move_target_point: AtomicCell<Vector3<f64>>,
    /// Vanilla `anchorPoint`: centre of the circling orbit.
    pub anchor_point: Mutex<Option<BlockPos>>,
    pub attack_phase: AtomicCell<AttackPhase>,
    size: AtomicI32,
}

impl PhantomEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let phantom = Self {
            mob_entity,
            move_target_point: AtomicCell::new(Vector3::new(0.0, 0.0, 0.0)),
            anchor_point: Mutex::new(None),
            attack_phase: AtomicCell::new(AttackPhase::Circle),
            size: AtomicI32::new(0),
        };
        let mob_arc = Arc::new(phantom);

        {
            let mut move_control = mob_arc
                .mob_entity
                .move_control
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *move_control = Box::new(PhantomMoveControl::new(Arc::downgrade(&mob_arc)));
        };

        {
            *mob_arc
                .mob_entity
                .look_control
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) =
                Box::new(PhantomLookControl::default());
        };

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;

            // TODO: PhantomCircleAroundAnchorGoal, PhantomSweepAttackGoal
            goal_selector.add_goal(
                1,
                Box::new(PhantomAttackStrategyGoal::new(Arc::downgrade(&mob_arc))),
            );
            goal_selector.add_goal(
                2,
                Box::new(PhantomSweepAttackGoal::new(Arc::downgrade(&mob_arc))),
            );
            goal_selector.add_goal(
                3,
                Box::new(PhantomCircleAroundAnchorGoal::new(Arc::downgrade(&mob_arc))),
            );

            let target_selector = &mob_arc.mob_entity.target_selector;
            target_selector.add_goal(
                1,
                Box::new(PhantomAttackPlayerTargetGoal::new(Arc::downgrade(&mob_arc))),
            );
        };

        mob_arc
    }

    #[must_use]
    pub fn get_phantom_size(&self) -> i32 {
        self.size.load(Relaxed)
    }

    /// Vanilla `Phantom.setPhantomSize`: clamp and sync. `SynchedEntityData.set` skips
    /// `onSyncedDataUpdated` when the value is unchanged, so dimensions and damage are
    /// only re-derived when the size actually changes; a size-0 phantom keeps the
    /// registry attack damage.
    pub fn set_phantom_size(&self, size: i32) {
        let size = size.clamp(0, MAX_SIZE);
        let changed = self.size.swap(size, Relaxed) != size;
        self.mob_entity
            .living_entity
            .entity
            .set_synced_data(tracked_data::phantom::ID_SIZE, size);
        if changed {
            self.update_phantom_size_info();
        }
    }

    /// Vanilla `Phantom.updatePhantomSizeInfo`.
    fn update_phantom_size_info(&self) {
        let size = self.get_phantom_size();
        let entity = &self.mob_entity.living_entity.entity;
        let entity_type = entity.entity_type;
        let scale = 1.0 + 0.15 * size as f32;

        let dimensions = EntityDimensions {
            width: entity_type.dimension[0] * scale,
            height: entity_type.dimension[1] * scale,
            eye_height: entity_type.eye_height * scale,
        };
        entity.entity_dimension.store(dimensions);
        let pos = entity.pos.load();
        entity
            .bounding_box
            .store(BoundingBox::new_from_pos(pos.x, pos.y, pos.z, &dimensions));

        let mut attributes = self
            .mob_entity
            .living_entity
            .attributes
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(damage) = attributes.get_mut(&Attributes::ATTACK_DAMAGE.id) {
            damage.base_value = f64::from(6 + size);
            damage.dirty.store(true, Relaxed);
        }
    }
}

impl Mob for PhantomEntity {
    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn get_mob_gravity(&self) -> f64 {
        0.0
    }

    fn get_mob_y_velocity_drag(&self) -> Option<f64> {
        // Vanilla `travelFlying` scales the whole movement vector by 0.91.
        Some(0.91)
    }

    fn mob_init_data_tracker(&self) {
        self.set_phantom_size(self.get_phantom_size());
    }

    fn finalize_spawn(
        &self,
        _world: &Arc<World>,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        let block_pos = self.mob_entity.living_entity.entity.block_pos.load();
        *self
            .anchor_point
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(block_pos.up_height(5));
        self.set_phantom_size(0);
        self.mob_entity.finalize_spawn_base();
        group_data
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        if let Some(anchor) = *self
            .anchor_point
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
        {
            nbt.put(
                "anchor_pos",
                NbtTag::IntArray(vec![anchor.0.x, anchor.0.y, anchor.0.z]),
            );
        }
        nbt.put_int("size", self.get_phantom_size());
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        *self
            .anchor_point
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            nbt.get_int_array("anchor_pos").and_then(|values| {
                let &[x, y, z] = values else {
                    return None;
                };
                Some(BlockPos::new(x, y, z))
            });
        self.set_phantom_size(nbt.get_int("size").unwrap_or(0));
    }
}

/// Vanilla `Phantom.PhantomLookControl`: records look requests but its `tick` does nothing.
#[derive(Default)]
struct PhantomLookControl(LookControl);

impl Control for PhantomLookControl {}

impl LookControlTrait for PhantomLookControl {
    fn tick(&mut self, _mob: &dyn Mob) {}

    fn is_looking_at_target(&self) -> bool {
        self.0.is_looking_at_target()
    }

    fn wanted_position(&self) -> Vector3<f64> {
        self.0.wanted_position()
    }

    fn look_at_with_range(
        &mut self,
        x: f64,
        y: f64,
        z: f64,
        max_yaw_change: f32,
        max_pitch_change: f32,
    ) {
        self.0
            .look_at_with_range(x, y, z, max_yaw_change, max_pitch_change);
    }
}
