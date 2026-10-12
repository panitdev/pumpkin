use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Mutex, Weak};

use pumpkin_data::damage::DamageType;
use pumpkin_data::entity::EntityType;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_data::tracked_data;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use rand::RngExt;
use uuid::Uuid;

use crate::entity::{
    Entity, EntityBase,
    ai::{
        control::vex_move_control::VexMoveControl,
        goal::{
            Controls, Goal, active_target::ActiveTargetGoal, look_at_entity::LookAtEntityGoal,
            revenge::RevengeGoal, swim::SwimGoal, to_goal_ticks, track_target::TrackTargetGoal,
        },
        target_predicate::TargetPredicate,
    },
    mob::{Mob, MobEntity},
};

const FLAG_IS_CHARGING: u8 = 1;

pub struct VexEntity {
    pub mob_entity: MobEntity,
    owner: Mutex<Option<Uuid>>,
    bound_origin: Mutex<Option<BlockPos>>,
    has_limited_life: AtomicBool,
    limited_life_ticks: AtomicI32,
    charging: AtomicBool,
}

impl VexEntity {
    #[must_use]
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        mob_entity.living_entity.entity.set_has_no_gravity(true);

        let vex = Self {
            mob_entity,
            owner: Mutex::new(None),
            bound_origin: Mutex::new(None),
            has_limited_life: AtomicBool::new(false),
            limited_life_ticks: AtomicI32::new(0),
            charging: AtomicBool::new(false),
        };
        let mob_arc = Arc::new(vex);

        *mob_arc
            .mob_entity
            .move_control
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Box::new(VexMoveControl::default());

        let vex_weak: Weak<Self> = Arc::downgrade(&mob_arc);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        {
            let mut goal_selector = mob_arc
                .mob_entity
                .goals_selector
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            goal_selector.add_goal(0, Box::new(SwimGoal::default()));
            goal_selector.add_goal(4, Box::new(VexChargeAttackGoal::new(vex_weak.clone())));
            goal_selector.add_goal(8, Box::new(VexRandomMoveGoal::new(vex_weak)));
            goal_selector.add_goal(
                9,
                Box::new(LookAtEntityGoal::new(
                    mob_weak.clone(),
                    &EntityType::PLAYER,
                    3.0,
                    1.0,
                    false,
                )),
            );
            // Vanilla goal 10 is `LookAtPlayerGoal(this, Mob.class, 8.0)`. Pumpkin's look-at
            // goal filters by `EntityType` and has no whole-class target, so this keeps the
            // vanilla range and 2% chance but only looks at players.
            goal_selector.add_goal(
                10,
                LookAtEntityGoal::with_default(mob_weak, &EntityType::PLAYER, 8.0),
            );

            let mut target_selector = mob_arc
                .mob_entity
                .target_selector
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            target_selector.add_goal(
                1,
                Box::new(
                    RevengeGoal::new(true)
                        .ignoring(|entity_type| {
                            entity_type.has_tag(&tag::EntityType::MINECRAFT_RAIDERS)
                        })
                        .alerting_others(),
                ),
            );
            target_selector.add_goal(2, Box::new(VexCopyOwnerTargetGoal::new()));
            target_selector.add_goal(
                3,
                ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::PLAYER, true),
            );
        };

        mob_arc
    }

    pub fn set_owner(&self, owner: Option<Uuid>) {
        *self
            .owner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = owner;
    }

    #[must_use]
    pub fn get_bound_origin(&self) -> Option<BlockPos> {
        *self
            .bound_origin
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn set_bound_origin(&self, bound_origin: Option<BlockPos>) {
        *self
            .bound_origin
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = bound_origin;
    }

    pub fn set_limited_life(&self, life_ticks: i32) {
        self.has_limited_life.store(true, Ordering::Relaxed);
        self.limited_life_ticks.store(life_ticks, Ordering::Relaxed);
    }

    #[must_use]
    pub fn is_charging(&self) -> bool {
        self.charging.load(Ordering::Relaxed)
    }

    pub fn set_charging(&self, charging: bool) {
        self.charging.store(charging, Ordering::Relaxed);
        let flags: u8 = if charging { FLAG_IS_CHARGING } else { 0 };
        self.mob_entity
            .living_entity
            .entity
            .set_synced_data(tracked_data::vex::DATA_FLAGS_ID, flags);
    }
}

impl Mob for VexEntity {
    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn get_owner_uuid(&self) -> Option<Uuid> {
        *self
            .owner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Vanilla `Vex` floats: no gravity, and `noPhysics` around its own tick.
    fn get_mob_gravity(&self) -> f64 {
        0.0
    }

    /// Vanilla `Vex.isAffectedByBlocks`: powder snow, `stepOn` and `entityInside` still apply
    /// while the tick's `no_physics` only makes movement ignore collisions.
    fn mob_is_affected_by_blocks(&self) -> bool {
        !self.get_entity().is_removed()
    }

    fn mob_init_data_tracker(&self) {
        self.get_entity()
            .set_synced_data(tracked_data::vex::DATA_FLAGS_ID, 0u8);
    }

    fn mob_tick(&self, caller: &dyn EntityBase) {
        let entity = &self.mob_entity.living_entity.entity;
        entity.no_physics.store(true, Ordering::Relaxed);

        if self.has_limited_life.load(Ordering::Relaxed)
            && self.limited_life_ticks.fetch_sub(1, Ordering::Relaxed) - 1 <= 0
        {
            self.limited_life_ticks.store(20, Ordering::Relaxed);
            self.mob_entity
                .living_entity
                .damage(caller, 1.0, DamageType::STARVE);
        }
    }

    fn post_tick(&self) {
        self.mob_entity
            .living_entity
            .entity
            .no_physics
            .store(false, Ordering::Relaxed);
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        if let Some(pos) = self.get_bound_origin() {
            nbt.put(
                "bound_pos",
                NbtTag::IntArray(vec![pos.0.x, pos.0.y, pos.0.z]),
            );
        }
        if self.has_limited_life.load(Ordering::Relaxed) {
            nbt.put_int(
                "life_ticks",
                self.limited_life_ticks.load(Ordering::Relaxed),
            );
        }
        if let Some(owner) = self.get_owner_uuid() {
            nbt.put_uuid("owner", owner);
        }
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.set_bound_origin(
            nbt.get_int_array("bound_pos")
                .and_then(|pos| (pos.len() == 3).then(|| BlockPos::new(pos[0], pos[1], pos[2]))),
        );

        if let Some(life_ticks) = nbt.get_int("life_ticks") {
            self.set_limited_life(life_ticks);
        } else {
            self.has_limited_life.store(false, Ordering::Relaxed);
        }

        self.set_owner(nbt.get_uuid("owner"));
    }
}

/// Vanilla `Vex.VexChargeAttackGoal`.
struct VexChargeAttackGoal {
    vex: Weak<VexEntity>,
}

impl VexChargeAttackGoal {
    #[must_use]
    const fn new(vex: Weak<VexEntity>) -> Self {
        Self { vex }
    }

    fn set_wanted_to_target(mob: &dyn Mob, target: &Arc<dyn EntityBase>) {
        let eye = target.get_entity().get_eye_pos();
        mob.get_mob_entity()
            .move_control
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .set_wanted_position(eye.x, eye.y, eye.z, 1.0);
    }
}

impl Goal for VexChargeAttackGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(target) = mob.get_mob_entity().get_target() else {
            return false;
        };
        if !target.get_entity().is_alive() {
            return false;
        }
        if mob
            .get_mob_entity()
            .move_control
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .has_wanted()
        {
            return false;
        }
        if mob.get_random().random_range(0..to_goal_ticks(7)) != 0 {
            return false;
        }
        let mob_pos = mob.get_entity().pos.load();
        let target_pos = target.get_entity().pos.load();
        mob_pos.squared_distance_to_vec(&target_pos) > 4.0
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        let has_wanted = mob
            .get_mob_entity()
            .move_control
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .has_wanted();
        let charging = self.vex.upgrade().is_some_and(|vex| vex.is_charging());
        let alive_target = mob
            .get_mob_entity()
            .get_target()
            .is_some_and(|target| target.get_entity().is_alive());
        has_wanted && charging && alive_target
    }

    fn start(&mut self, mob: &dyn Mob) {
        if let Some(target) = mob.get_mob_entity().get_target() {
            Self::set_wanted_to_target(mob, &target);
        }
        if let Some(vex) = self.vex.upgrade() {
            vex.set_charging(true);
        }
        let entity = mob.get_entity();
        if !entity.silent.load(Ordering::Relaxed) {
            entity.world.load().play_sound(
                Sound::EntityVexCharge,
                SoundCategory::Hostile,
                &entity.pos.load(),
            );
        }
    }

    fn stop(&mut self, _mob: &dyn Mob) {
        if let Some(vex) = self.vex.upgrade() {
            vex.set_charging(false);
        }
    }

    fn should_run_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let Some(target) = mob.get_mob_entity().get_target() else {
            return;
        };

        if mob
            .get_entity()
            .bounding_box
            .load()
            .intersects(&target.get_entity().bounding_box.load())
        {
            if mob
                .get_mob_entity()
                .try_attack(mob.get_entity(), target.as_ref())
            {
                mob.on_attack(target.as_ref());
            }
            if let Some(vex) = self.vex.upgrade() {
                vex.set_charging(false);
            }
        } else {
            let mob_pos = mob.get_entity().pos.load();
            let target_pos = target.get_entity().pos.load();
            if mob_pos.squared_distance_to_vec(&target_pos) < 9.0 {
                Self::set_wanted_to_target(mob, &target);
            }
        }
    }

    fn controls(&self) -> Controls {
        Controls::MOVE
    }
}

/// Vanilla `Vex.VexRandomMoveGoal`.
struct VexRandomMoveGoal {
    vex: Weak<VexEntity>,
}

impl VexRandomMoveGoal {
    #[must_use]
    const fn new(vex: Weak<VexEntity>) -> Self {
        Self { vex }
    }
}

impl Goal for VexRandomMoveGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        if mob
            .get_mob_entity()
            .move_control
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .has_wanted()
        {
            return false;
        }
        mob.get_random().random_range(0..to_goal_ticks(7)) == 0
    }

    fn should_continue(&mut self, _mob: &dyn Mob) -> bool {
        false
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let entity = mob.get_entity();
        let origin = self
            .vex
            .upgrade()
            .and_then(|vex| vex.get_bound_origin())
            .unwrap_or_else(|| entity.block_pos.load());
        let world = entity.world.load();

        for _ in 0..3 {
            let test_pos = origin.offset(Vector3::new(
                mob.get_random().random_range(-7..8),
                mob.get_random().random_range(-5..6),
                mob.get_random().random_range(-7..8),
            ));
            if world.get_block_state(&test_pos).is_air() {
                let x = f64::from(test_pos.0.x) + 0.5;
                let y = f64::from(test_pos.0.y) + 0.5;
                let z = f64::from(test_pos.0.z) + 0.5;
                mob.get_mob_entity()
                    .move_control
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .set_wanted_position(x, y, z, 0.25);
                if mob.get_mob_entity().get_target().is_none() {
                    mob.get_mob_entity()
                        .look_control
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .look_at_with_range(x, y, z, 180.0, 20.0);
                }
                break;
            }
        }
    }

    fn controls(&self) -> Controls {
        Controls::MOVE
    }
}

/// Vanilla `Vex.VexCopyOwnerTargetGoal`.
struct VexCopyOwnerTargetGoal {
    track_target: TrackTargetGoal,
    target: Option<Arc<dyn EntityBase>>,
    copy_owner_targeting: TargetPredicate,
}

impl VexCopyOwnerTargetGoal {
    fn new() -> Self {
        Self {
            track_target: TrackTargetGoal::with_default(false),
            target: None,
            copy_owner_targeting: TargetPredicate::create_non_attackable()
                .ignore_visibility()
                .ignore_distance_scaling_factor(),
        }
    }
}

impl Goal for VexCopyOwnerTargetGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(owner_uuid) = mob.get_owner_uuid() else {
            return false;
        };
        let world = mob.get_entity().world.load();
        let Some(owner) = world.get_entity_by_uuid(owner_uuid) else {
            return false;
        };
        let Some(owner_target) = owner
            .get_mob()
            .and_then(|owner_mob| owner_mob.get_mob_entity().get_target())
        else {
            return false;
        };
        if !self
            .copy_owner_targeting
            .test(&world, Some(mob), owner_target.as_ref())
        {
            return false;
        }
        self.target = Some(owner_target);
        true
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        self.track_target.should_continue(mob)
    }

    fn start(&mut self, mob: &dyn Mob) {
        mob.set_mob_target(self.target.clone());
        self.track_target.start(mob);
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.target = None;
        self.track_target.stop(mob);
    }

    fn controls(&self) -> Controls {
        self.track_target.controls()
    }
}
