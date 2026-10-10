use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Weak};

use crossbeam::atomic::AtomicCell;
use pumpkin_data::damage::DamageType;
use pumpkin_data::entity::EntityType;
use pumpkin_util::math::vector3::Vector3;

use crate::entity::{
    Entity, EntityBase,
    ai::{
        goal::{
            active_target::ActiveTargetGoal, look_around::RandomLookAroundGoal,
            look_at_entity::LookAtEntityGoal, move_towards_restriction::MoveTowardsRestrictionGoal,
            revenge::RevengeGoal, wander_around::WanderAroundGoal,
        },
        pathfinder::node::PathType,
        util::RandomExt,
    },
    mob::{Mob, MobEntity},
};

/// Ticks between rerolls of how far above the blaze a target may be before it climbs.
const HEIGHT_OFFSET_REROLL_TICKS: i32 = 100;
/// Mean and spread of the allowed height offset (a triangular roll).
const HEIGHT_OFFSET_MEAN: f64 = 0.5;
const HEIGHT_OFFSET_SPREAD: f64 = 6.891;
/// Climbing eases vertical speed toward this value, closing this fraction of the gap per tick.
const CLIMB_TARGET_SPEED: f64 = 0.3;
const CLIMB_EASING: f64 = 0.3;
/// Falling speed is scaled by this each tick while airborne, which gives the slow drift down.
const FALL_DAMPING: f64 = 0.6;

pub struct BlazeEntity {
    pub entity: Arc<MobEntity>,
    pub is_charged: AtomicBool,
    /// How far the target's eyes may sit above the blaze's eyes before it climbs.
    allowed_height_offset: AtomicCell<f32>,
    next_height_offset_change_tick: AtomicI32,
}

impl BlazeEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let entity = Arc::new(MobEntity::new(entity));
        let blaze = Self {
            entity,
            is_charged: AtomicBool::new(false),
            allowed_height_offset: AtomicCell::new(HEIGHT_OFFSET_MEAN as f32),
            next_height_offset_change_tick: AtomicI32::new(0),
        };
        let mob_arc = Arc::new(blaze);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        // Blazes never path through water, take lava as a last resort and ignore fire.
        let mut navigator = mob_arc
            .entity
            .navigator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        navigator.set_pathfinding_malus(PathType::Water, -1.0);
        navigator.set_pathfinding_malus(PathType::Lava, 8.0);
        navigator.set_pathfinding_malus(PathType::DangerFire, 0.0);
        navigator.set_pathfinding_malus(PathType::DamageFire, 0.0);
        drop(navigator);

        {
            let mut goal_selector = mob_arc
                .entity
                .goals_selector
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut target_selector = mob_arc
                .entity
                .target_selector
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            // No swim goal: a blaze does not float, water only hurts it.
            goal_selector.add_goal(
                4,
                Box::new(
                    crate::entity::ai::goal::blaze_attack::BlazeShootFireballGoal::new(
                        Arc::downgrade(&mob_arc),
                    ),
                ),
            );
            goal_selector.add_goal(5, Box::new(MoveTowardsRestrictionGoal::new(1.0)));
            // TODO: vanilla strolls with a water-avoiding goal; the generic wander is the closest
            // goal Pumpkin has. The water malus above already keeps its targets out of water.
            goal_selector.add_goal(7, Box::new(WanderAroundGoal::new(1.0)));
            goal_selector.add_goal(
                8,
                LookAtEntityGoal::with_default(mob_weak, &EntityType::PLAYER, 8.0),
            );
            goal_selector.add_goal(8, Box::new(RandomLookAroundGoal::default()));

            target_selector.add_goal(1, Box::new(RevengeGoal::new(true).alerting_others()));
            target_selector.add_goal(
                2,
                ActiveTargetGoal::with_default(&mob_arc.entity, &EntityType::PLAYER, true),
            );
        };

        mob_arc
    }

    pub fn is_charged(&self) -> bool {
        self.is_charged.load(Ordering::Relaxed)
    }

    pub fn set_charged(&self, charged: bool) {
        self.is_charged.store(charged, Ordering::Relaxed);
        let flags = i8::from(charged);
        self.entity
            .living_entity
            .entity
            .set_synced_data(pumpkin_data::tracked_data::blaze::DATA_FLAGS_ID, flags);
    }

    /// Rain counts when it reaches the blaze's feet or the top of its hitbox.
    fn is_in_water_or_rain(entity: &Entity) -> bool {
        if entity.touching_water.load(Ordering::Relaxed) {
            return true;
        }
        let world = entity.world.load();
        world.is_raining_at(&entity.block_pos.load())
            || world.is_raining_at(&entity.bounding_box.load().max_block_pos())
    }

    /// Rerolls the allowed height offset on its timer, then climbs toward a target that sits
    /// too far above.
    fn tick_climb(&self) {
        // The counter starts at 0, so the first tick already rolls an offset.
        let previous = self
            .next_height_offset_change_tick
            .fetch_sub(1, Ordering::Relaxed);
        if previous <= 1 {
            self.next_height_offset_change_tick
                .store(HEIGHT_OFFSET_REROLL_TICKS, Ordering::Relaxed);
            let offset = rand::rng().triangle(HEIGHT_OFFSET_MEAN, HEIGHT_OFFSET_SPREAD);
            self.allowed_height_offset.store(offset as f32);
        }

        let Some(target) = self.entity.get_target() else {
            return;
        };
        let entity = &self.entity.living_entity.entity;
        let threshold = entity.get_eye_y() + f64::from(self.allowed_height_offset.load());
        if target.get_entity().get_eye_y() <= threshold || !self.can_attack(target.as_ref()) {
            return;
        }

        let vel = entity.velocity.load();
        let climb = (CLIMB_TARGET_SPEED - vel.y) * CLIMB_EASING;
        entity
            .velocity
            .store(Vector3::new(vel.x, vel.y + climb, vel.z));
        entity.velocity_dirty.store(true, Ordering::SeqCst);
    }
}

impl Mob for BlazeEntity {
    fn get_mob_entity(&self) -> &MobEntity {
        &self.entity
    }

    fn mob_tick(&self, caller: &dyn EntityBase) {
        let base_entity = &self.entity.living_entity.entity;
        if !base_entity.is_alive() {
            return;
        }

        let on_ground = base_entity.on_ground.load(Ordering::Relaxed);
        let vel = base_entity.velocity.load();
        if !on_ground && vel.y < 0.0 {
            base_entity
                .velocity
                .store(Vector3::new(vel.x, vel.y * FALL_DAMPING, vel.z));
        }

        // Water-sensitive mobs take drowning damage in water and in rain.
        if Self::is_in_water_or_rain(base_entity) {
            caller.damage(caller, 1.0, DamageType::DROWN);
        }
    }

    fn custom_server_ai_step(&self, _caller: &dyn EntityBase) {
        self.tick_climb();
    }
}
