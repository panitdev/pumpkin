use std::borrow::Cow;
use std::sync::{
    Arc, Weak,
    atomic::{AtomicBool, AtomicI32, AtomicU8, Ordering},
};

use crossbeam::atomic::AtomicCell;
use pumpkin_data::data_component_impl::EnchantmentsImpl;
use pumpkin_data::data_component_impl::EquipmentSlot;
use pumpkin_data::entity::EntityType;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::vector3::Vector3;
use uuid::Uuid;

use crate::entity::{
    Entity, EntityBase,
    ageable::{AgeableData, AgeableMob},
    ai::goal::{
        Goal, escape_danger::EscapeDangerGoal, look_around::RandomLookAroundGoal,
        look_at_entity::LookAtEntityGoal, swim::SwimGoal, wander_around::WanderAroundGoal,
    },
    ai::util::RandomExt,
    lightning::LightningBoltEntity,
    mob::equipment::{RegionalDifficulty, apply_vanilla_enchantments},
    mob::spawn::finalize_spawn,
    mob::{Mob, MobEntity},
    passive::animal::Animal,
    player::Player,
    r#type::from_type,
};
use crate::world::World;

/// Vanilla `SkeletonHorse.TRAP_MAX_LIFE`.
const TRAP_MAX_LIFE: i32 = 18000;
/// Vanilla `SkeletonTrapGoal` player detection range.
const TRAP_TRIGGER_RANGE: f64 = 10.0;

pub const FLAG_TAME: u8 = 2;
pub const FLAG_SADDLE: u8 = 4;
pub const FLAG_EATING: u8 = 16;
pub const FLAG_STANDING: u8 = 32;
pub const FLAG_OPEN_MOUTH: u8 = 64;

pub struct SkeletonHorseEntity {
    pub mob_entity: MobEntity,
    pub ageable_data: AgeableData,
    pub flags: AtomicU8,
    pub temper: AtomicI32,
    pub owner: AtomicCell<Option<Uuid>>,
    is_trap: AtomicBool,
    trap_time: AtomicI32,
}

impl SkeletonHorseEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let horse = Self {
            mob_entity,
            ageable_data: AgeableData::default(),
            flags: AtomicU8::new(0),
            temper: AtomicI32::new(0),
            owner: AtomicCell::new(None),
            is_trap: AtomicBool::new(false),
            trap_time: AtomicI32::new(0),
        };
        let mob_arc = Arc::new(horse);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;

            goal_selector.add_goal(0, Box::new(SwimGoal::default()));
            goal_selector.add_goal(1, EscapeDangerGoal::new(1.2));
            goal_selector.add_goal(1, Box::new(SkeletonTrapGoal));
            goal_selector.add_goal(6, Box::new(WanderAroundGoal::new(0.7)));
            goal_selector.add_goal(
                7,
                LookAtEntityGoal::with_default(mob_weak, &EntityType::PLAYER, 6.0),
            );
            goal_selector.add_goal(8, Box::new(RandomLookAroundGoal::default()));
        };

        mob_arc
    }

    #[must_use]
    pub fn is_trap(&self) -> bool {
        self.is_trap.load(Ordering::Relaxed)
    }

    /// Vanilla `SkeletonHorse.setTrap`. The trap goal stays registered and is gated on this flag,
    /// because the selector is taken out of its lock while goals run.
    pub fn set_trap(&self, trap: bool) {
        self.is_trap.store(trap, Ordering::Relaxed);
    }

    #[must_use]
    pub fn has_flag(&self, flag: u8) -> bool {
        (self.flags.load(Ordering::Relaxed) & flag) != 0
    }

    pub fn set_flag(&self, flag: u8, val: bool) {
        let current = self.flags.load(Ordering::Relaxed);
        let new_flags = if val { current | flag } else { current & !flag };
        self.flags.store(new_flags, Ordering::Relaxed);
        let entity = self.get_entity();
        entity.set_synced_data(
            pumpkin_data::tracked_data::skeleton_horse::DATA_ID_FLAGS,
            new_flags as i8,
        );
    }

    #[must_use]
    pub fn is_tame(&self) -> bool {
        self.has_flag(FLAG_TAME)
    }

    pub fn set_tame(&self, val: bool) {
        self.set_flag(FLAG_TAME, val);
    }

    #[must_use]
    pub fn is_saddled(&self) -> bool {
        self.has_flag(FLAG_SADDLE)
    }

    pub fn set_saddled(&self, val: bool) {
        self.set_flag(FLAG_SADDLE, val);
    }
}

impl AgeableMob for SkeletonHorseEntity {
    fn get_ageable_data(&self) -> &AgeableData {
        &self.ageable_data
    }
}

impl Animal for SkeletonHorseEntity {
    fn is_food(&self, _item_stack: &ItemStack) -> bool {
        false
    }
}

impl Mob for SkeletonHorseEntity {
    fn as_ageable(&self) -> Option<&dyn AgeableMob> {
        Some(self)
    }

    fn as_animal(&self) -> Option<&dyn Animal> {
        Some(self)
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        self.write_ageable_nbt(nbt);
        nbt.put_bool("Tame", self.is_tame());
        nbt.put_int("Temper", self.temper.load(Ordering::Relaxed));
        nbt.put_bool("SkeletonTrap", self.is_trap());
        nbt.put_int("SkeletonTrapTime", self.trap_time.load(Ordering::Relaxed));
        if let Some(owner) = self.owner.load() {
            nbt.put_uuid("Owner", owner);
        }
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.read_ageable_nbt(nbt);
        if let Some(tame) = nbt.get_bool("Tame") {
            self.set_tame(tame);
        }
        if let Some(temper) = nbt.get_int("Temper") {
            self.temper.store(temper, Ordering::Relaxed);
        }
        if let Some(trap) = nbt.get_bool("SkeletonTrap") {
            self.set_trap(trap);
        }
        if let Some(trap_time) = nbt.get_int("SkeletonTrapTime") {
            self.trap_time.store(trap_time, Ordering::Relaxed);
        }
        if let Some(owner) = nbt.get_uuid("Owner") {
            self.owner.store(Some(owner));
        }
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn mob_tick(&self, _caller: &dyn EntityBase) {
        // Vanilla `SkeletonHorse.aiStep`: an untethered trap horse is discarded after 15 minutes.
        if !self.mob_entity.persistence_required.load(Ordering::Relaxed)
            && self.is_trap()
            && self.trap_time.fetch_add(1, Ordering::Relaxed) >= TRAP_MAX_LIFE
        {
            self.get_entity().remove();
        }
    }

    fn mob_init_data_tracker(&self) {
        let entity = self.get_entity();
        let is_baby = entity.age.load(Ordering::Relaxed) < 0;
        if is_baby {
            entity.set_synced_data(
                pumpkin_data::tracked_data::skeleton_horse::DATA_BABY_ID,
                true,
            );
        }
        entity.set_synced_data(
            pumpkin_data::tracked_data::skeleton_horse::DATA_ID_FLAGS,
            self.flags.load(Ordering::Relaxed) as i8,
        );
    }

    fn mob_interact(&self, player: &Arc<Player>, item_stack: &mut ItemStack) -> bool {
        let item = item_stack.get_item();

        if self.is_tame() && item == &Item::SADDLE && !self.is_saddled() && !self.is_baby() {
            self.set_saddled(true);
            item_stack.decrement_unless_creative(player.gamemode.load(), 1);
            let entity = self.get_entity();
            let world = entity.world.load();
            world.play_sound(
                Sound::EntityHorseSaddle,
                SoundCategory::Neutral,
                &entity.pos.load(),
            );
            return true;
        }

        if !self.is_baby() {
            let world = player.world();
            let ent = &self.mob_entity.living_entity.entity;
            if let Some(vehicle) = world.get_entity_by_id(ent.entity_id)
                && let Some(passenger) = world.get_player_by_id(player.entity_id())
            {
                ent.add_passenger(vehicle, passenger as Arc<dyn EntityBase>);
                return true;
            }
        }

        self.animal_interact(player, item_stack, Sound::EntitySkeletonHorseAmbient)
    }
}

/// Vanilla `SkeletonTrapGoal`: a trap skeleton horse that gets close to a player triggers a
/// visual lightning strike and disgorges four mounted skeletons.
pub struct SkeletonTrapGoal;

impl SkeletonTrapGoal {
    fn has_nearby_alive_player(world: &World, pos: Vector3<f64>) -> bool {
        world
            .get_nearest_player(pos, TRAP_TRIGGER_RANGE, |player| {
                !player.is_spectator() && player.get_entity().is_alive()
            })
            .is_some()
    }

    fn create_horse(world: &Arc<World>, pos: Vector3<f64>) -> Arc<dyn EntityBase> {
        let horse = from_type(&EntityType::SKELETON_HORSE, pos, world, Uuid::new_v4());
        finalize_spawn(&horse, world, None);
        if let Some(horse_mob) = horse.get_mob() {
            horse_mob
                .get_mob_entity()
                .persistence_required
                .store(true, Ordering::Relaxed);
        }
        if let Some(skeleton_horse) = horse.cast_any().downcast_ref::<SkeletonHorseEntity>() {
            skeleton_horse.set_tame(true);
        }
        horse.get_entity().set_age(0);
        // TODO: vanilla `SkeletonTrapGoal.createHorse` sets `setInvulnerableTime(60)`; the
        // engine has no timed-invulnerability hook.
        horse
    }

    fn create_skeleton(world: &Arc<World>, pos: Vector3<f64>) -> Arc<dyn EntityBase> {
        let skeleton = from_type(&EntityType::SKELETON, pos, world, Uuid::new_v4());
        finalize_spawn(&skeleton, world, None);
        if let Some(mob) = skeleton.get_mob() {
            mob.get_mob_entity()
                .persistence_required
                .store(true, Ordering::Relaxed);
        }
        // TODO: vanilla `SkeletonTrapGoal.createSkeleton` sets `setInvulnerableTime(60)`; the
        // engine has no timed-invulnerability hook.
        skeleton
    }

    /// Vanilla `SkeletonTrapGoal.createSkeleton`: give the rider its iron helmet if bare, then
    /// run the mob spawn equipment enchantment provider over the weapon and helmet.
    fn equip_skeleton(skeleton: &Arc<dyn EntityBase>, difficulty: &RegionalDifficulty) {
        let Some(mob) = skeleton.get_mob() else {
            return;
        };
        let mob_entity = mob.get_mob_entity();

        let head_empty = mob_entity
            .living_entity
            .entity_equipment
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&EquipmentSlot::HEAD)
            .is_empty();
        if head_empty {
            mob_entity.set_item_slot(&EquipmentSlot::HEAD, ItemStack::new(1, &Item::IRON_HELMET));
        }

        for slot in [&EquipmentSlot::MAIN_HAND, &EquipmentSlot::HEAD] {
            let stack = mob_entity
                .living_entity
                .entity_equipment
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(slot);
            if stack.is_empty() {
                continue;
            }
            let mut stack = stack;
            // Vanilla SkeletonTrapGoal.enchant drops any spawn-roll enchantments first.
            stack.set_data_component(EnchantmentsImpl {
                enchantment: Cow::Borrowed(&[]),
            });
            apply_vanilla_enchantments(&mut stack, slot, difficulty.special_multiplier);
            mob_entity.set_item_slot(slot, stack);
        }
    }
}

impl Goal for SkeletonTrapGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(horse) = mob.cast_any().downcast_ref::<SkeletonHorseEntity>() else {
            return false;
        };
        if !horse.is_trap() {
            return false;
        }
        let entity = mob.get_entity();
        Self::has_nearby_alive_player(&entity.world.load(), entity.pos.load())
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let Some(horse) = mob.cast_any().downcast_ref::<SkeletonHorseEntity>() else {
            return;
        };
        let entity = mob.get_entity();
        let world = entity.world.load_full();
        let pos = entity.pos.load();
        let difficulty = RegionalDifficulty::at(&world, pos);

        horse.set_trap(false);
        horse.set_tame(true);
        entity.set_age(0);

        let bolt = from_type(&EntityType::LIGHTNING_BOLT, pos, &world, Uuid::new_v4());
        if let Some(lightning) = bolt.cast_any().downcast_ref::<LightningBoltEntity>() {
            lightning.set_visual_only(true);
        }
        world.spawn_entity(bolt);

        // The trap horse itself gets the first rider.
        let skeleton = Self::create_skeleton(&world, pos);
        world.spawn_entity(skeleton.clone());
        Self::equip_skeleton(&skeleton, &difficulty);
        if let Some(vehicle) = world.get_entity_by_id(entity.entity_id) {
            vehicle
                .get_entity()
                .add_passenger(vehicle.clone(), skeleton);
        }

        for _ in 0..3 {
            let other_horse = Self::create_horse(&world, pos);
            let other_skeleton = Self::create_skeleton(&world, pos);
            world.spawn_entity(other_horse.clone());
            world.spawn_entity(other_skeleton.clone());
            Self::equip_skeleton(&other_skeleton, &difficulty);
            other_horse
                .get_entity()
                .add_passenger(other_horse.clone(), other_skeleton);

            let mut rng = rand::rng();
            let velocity = other_horse.get_entity().velocity.load();
            other_horse.get_entity().set_velocity(Vector3::new(
                velocity.x + rng.triangle(0.0, 1.1485),
                velocity.y,
                velocity.z + rng.triangle(0.0, 1.1485),
            ));
        }
    }
}
