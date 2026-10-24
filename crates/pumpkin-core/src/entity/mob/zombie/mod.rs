use super::conversion::{ConversionParams, convert_to};
use super::{Mob, MobEntity, MobMetadata};
use crate::entity::EntityBase;
use crate::entity::ai::goal::break_door::BreakDoorGoal;
use crate::entity::ai::goal::destroy_egg::DestroyEggGoal;
use crate::entity::ai::goal::look_around::RandomLookAroundGoal;
use crate::entity::ai::goal::revenge::RevengeGoal;
use crate::entity::ai::goal::spear_use::SpearUseGoal;
use crate::entity::ai::goal::swim::SwimGoal;
use crate::entity::ai::goal::wander_around::WanderAroundGoal;
use crate::entity::ai::goal::zombie_attack::ZombieAttackGoal;
use crate::entity::mob::equipment::RegionalDifficulty;
use crate::entity::{
    Entity,
    ai::goal::{active_target::ActiveTargetGoal, look_at_entity::LookAtEntityGoal},
};
use crate::world::World;
use pumpkin_data::data_component_impl::EquipmentSlot;
use pumpkin_data::effect::StatusEffect;
use pumpkin_data::entity::EntityType;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::potion::Effect;
use pumpkin_data::tracked_data;
use pumpkin_data::world::WorldEvent;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::Difficulty;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Weak};

/// The `Mob` methods every `ZombieEntityBase` wrapper forwards, `$field` being the base.
macro_rules! zombie_mob_forwards {
    ($field:ident) => {
        fn get_mob_entity(&self) -> &$crate::entity::mob::MobEntity {
            &self.$field.mob_entity
        }

        fn as_zombie(&self) -> Option<&$crate::entity::mob::zombie::ZombieEntityBase> {
            Some(&self.$field)
        }

        fn spawn_as_baby(&self) -> bool {
            self.$field.spawn_as_baby()
        }

        fn populate_default_equipment_enchantments(
            &self,
            difficulty: &$crate::entity::mob::equipment::RegionalDifficulty,
        ) {
            self.$field
                .populate_default_equipment_enchantments(difficulty);
        }

        fn on_attack(&self, target: &dyn $crate::entity::EntityBase) {
            self.$field.on_attack(target);
        }
    };
}

/// Forwards the default equipment roll, for types that keep the zombie one.
macro_rules! zombie_mob_slots {
    ($field:ident) => {
        fn populate_default_equipment_slots(
            &self,
            world: &std::sync::Arc<$crate::world::World>,
            difficulty: &$crate::entity::mob::equipment::RegionalDifficulty,
        ) {
            self.$field
                .populate_default_equipment_slots(world, difficulty);
        }
    };
}

/// `Mob` for a zombie-family type with nothing of its own on top of `ZombieEntityBase`.
macro_rules! zombie_mob {
    ($ty:ident, $field:ident) => {
        zombie_mob!($ty, $field, {
            zombie_mob_slots!($field);
        });
    };
    ($ty:ident, $field:ident, { $($extra:tt)* }) => {
        impl $crate::entity::mob::Mob for $ty {
            zombie_mob_forwards!($field);
            $($extra)*

            fn mob_tick(&self, caller: &dyn $crate::entity::EntityBase) {
                self.$field.mob_tick(caller);
            }

            fn mob_write_nbt(&self, nbt: &mut pumpkin_nbt::compound::NbtCompound) {
                self.$field.mob_write_nbt(nbt);
            }

            fn mob_read_nbt(&self, nbt: &pumpkin_nbt::compound::NbtCompound) {
                self.$field.mob_read_nbt(nbt);
            }
        }
    };
}

pub mod drowned;
pub mod husk;
#[allow(clippy::module_inception)]
pub mod zombie;
pub mod zombie_villager;

/// The `Zombie` subclasses that share `ZombieEntityBase`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ZombieKind {
    Zombie,
    Husk,
    Drowned,
    ZombieVillager,
}

impl ZombieKind {
    /// Vanilla `Zombie.convertsInWater`.
    const fn converts_in_water(self) -> bool {
        matches!(self, Self::Zombie | Self::Husk)
    }

    /// Vanilla `Zombie.convertsToWhenDrowning`.
    const fn converts_to_when_drowning(self) -> &'static EntityType {
        match self {
            Self::Husk => &EntityType::ZOMBIE,
            _ => &EntityType::DROWNED,
        }
    }

    /// Vanilla `Zombie.getConversionSound`.
    const fn conversion_sound(self) -> WorldEvent {
        match self {
            Self::Husk => WorldEvent::SoundHuskToZombie,
            _ => WorldEvent::SoundZombieToDrowned,
        }
    }
}

/// Ticks submerged before the drowning conversion starts (vanilla `ConversionTracker` afflictionTime).
const IN_WATER_TIME_TO_CONVERT: i32 = 600;
/// Ticks the conversion runs before the zombie turns.
const DROWNED_CONVERSION_TIME: i32 = 300;
const NOT_CONVERTING: i32 = -1;
/// Vanilla `Husk.doHurtTarget`: hunger ticks per point of effective difficulty.
const HUSK_HUNGER_TICKS_PER_DIFFICULTY: i32 = 140;
/// Vanilla `Zombie.doHurtTarget`: chance per point of effective difficulty to ignite the target.
const IGNITE_TARGET_CHANCE_PER_DIFFICULTY: f32 = 0.3;

pub struct ZombieEntityBase {
    pub mob_entity: MobEntity,
    kind: ZombieKind,
    pub can_break_doors: AtomicBool,
    pub is_baby: AtomicBool,
    /// Vanilla `drowningTracker` afflictionTime: ticks spent with the eyes in water.
    in_water_time: AtomicI32,
    drowned_conversion_time: AtomicI32,
    converting_to_drowned: AtomicBool,
    /// Vanilla `Drowned.searchingForLand`.
    searching_for_land: AtomicBool,
}

impl ZombieEntityBase {
    pub fn new(entity: Entity, kind: ZombieKind) -> Arc<Self> {
        Self::with_can_break_doors(entity, kind, false)
    }

    pub fn with_can_break_doors(
        entity: Entity,
        kind: ZombieKind,
        can_break_doors: bool,
    ) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let zombie = Self {
            mob_entity,
            kind,
            can_break_doors: AtomicBool::new(can_break_doors),
            is_baby: AtomicBool::new(false),
            in_water_time: AtomicI32::new(0),
            drowned_conversion_time: AtomicI32::new(NOT_CONVERTING),
            converting_to_drowned: AtomicBool::new(false),
            searching_for_land: AtomicBool::new(false),
        };
        let mob_arc = Arc::new(zombie);

        // `Drowned.addBehaviourGoals` replaces the whole Zombie behaviour goal set, and the
        // trident goal needs the Drowned's own `RangedAttackMob` arc, so DrownedEntity
        // registers its goals itself once it exists.
        if kind != ZombieKind::Drowned {
            let mob_weak: Weak<dyn Mob> = {
                let mob_arc: Arc<dyn Mob> = mob_arc.clone();
                Arc::downgrade(&mob_arc)
            };

            let goal_selector = &mob_arc.mob_entity.goals_selector;
            let target_selector = &mob_arc.mob_entity.target_selector;

            goal_selector.add_goal(0, Box::new(SwimGoal::default()));
            if can_break_doors {
                goal_selector.add_goal(1, Box::new(BreakDoorGoal::default()));
            }
            goal_selector.add_goal(2, SpearUseGoal::new(1.0, 1.0, 10.0, 2.0));
            goal_selector.add_goal(3, ZombieAttackGoal::new(1.0, false));
            goal_selector.add_goal(4, DestroyEggGoal::new(1.0, 3));
            // TODO: MoveThroughVillageGoal (priority 6) needs village POIs, which Pumpkin lacks.
            goal_selector.add_goal(7, Box::new(WanderAroundGoal::new(1.0)));
            goal_selector.add_goal(
                8,
                LookAtEntityGoal::with_default(mob_weak, &EntityType::PLAYER, 8.0),
            );
            goal_selector.add_goal(8, Box::new(RandomLookAroundGoal::default()));

            target_selector.add_goal(
                1,
                Box::new(
                    RevengeGoal::new(true).alerting_others_except(|entity_type| {
                        entity_type == &EntityType::ZOMBIFIED_PIGLIN
                    }),
                ),
            );
            target_selector.add_goal(
                2,
                ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::PLAYER, true),
            );
            // AbstractVillager, which needs no line of sight.
            target_selector.add_goal(
                3,
                ActiveTargetGoal::predicated(&mob_arc.mob_entity, 10, false, |target, _world| {
                    target.entity.entity_type == &EntityType::VILLAGER
                        || target.entity.entity_type == &EntityType::WANDERING_TRADER
                }),
            );
            target_selector.add_goal(
                3,
                ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::IRON_GOLEM, true),
            );
            target_selector.add_goal(
                5,
                ActiveTargetGoal::baby_turtle_on_land(&mob_arc.mob_entity),
            );
        }

        mob_arc
    }

    #[must_use]
    pub fn can_break_doors(&self) -> bool {
        self.can_break_doors.load(Ordering::Relaxed)
    }

    pub fn set_can_break_doors(&self, can_break_doors: bool, mob: &dyn Mob) {
        if self
            .can_break_doors
            .swap(can_break_doors, Ordering::Relaxed)
            != can_break_doors
        {
            if can_break_doors {
                self.mob_entity
                    .goals_selector
                    .add_goal(1, Box::new(BreakDoorGoal::default()));
            } else {
                self.mob_entity
                    .goals_selector
                    .remove_goal::<BreakDoorGoal>(mob);
            }
        }
    }
}

impl ZombieEntityBase {
    /// Vanilla `Zombie.setBaby`: the baby state is only the synced flag, it never ages up.
    // TODO: vanilla baby zombies get the +50% SPEED_MODIFIER_BABY and 2.5x XP
    // (`Zombie.getBaseExperienceReward`), unlike passive babies which drop none.
    pub fn set_baby(&self, baby: bool) {
        self.mob_entity.set_baby_flag(
            &self.is_baby,
            pumpkin_data::tracked_data::zombie::BABY,
            baby,
        );
    }

    #[must_use]
    pub fn is_baby(&self) -> bool {
        self.is_baby.load(Ordering::Relaxed)
    }
}

impl ZombieEntityBase {
    /// Vanilla `Drowned.isSearchingForLand`.
    #[must_use]
    pub fn is_searching_for_land(&self) -> bool {
        self.searching_for_land.load(Ordering::Relaxed)
    }

    pub fn set_searching_for_land(&self, searching: bool) {
        self.searching_for_land.store(searching, Ordering::Relaxed);
    }

    /// Vanilla `Drowned.wantsToSwim`.
    #[must_use]
    pub fn wants_to_swim(&self) -> bool {
        self.is_searching_for_land()
            || self
                .mob_entity
                .get_target()
                .is_some_and(|target| target.get_entity().is_in_water())
    }

    /// Vanilla `Drowned.updateSwimming`.
    fn update_drowned_swimming(&self) {
        let entity = &self.mob_entity.living_entity.entity;
        entity.set_swimming(
            !self.mob_entity.is_no_ai() && entity.is_under_water() && self.wants_to_swim(),
        );
    }
}

impl ZombieEntityBase {
    /// Vanilla `Zombie.isUnderWaterConverting`.
    #[must_use]
    pub fn is_under_water_converting(&self) -> bool {
        self.converting_to_drowned.load(Ordering::Relaxed)
    }

    fn set_converting_to_drowned(&self, converting: bool) {
        self.converting_to_drowned
            .store(converting, Ordering::Relaxed);
        if !converting {
            self.drowned_conversion_time
                .store(NOT_CONVERTING, Ordering::Relaxed);
        }
        self.mob_entity
            .living_entity
            .entity
            .set_synced_data(tracked_data::zombie::DATA_DROWNED_CONVERSION_ID, converting);
    }

    fn start_drowned_conversion(&self, time: i32) {
        self.drowned_conversion_time.store(time, Ordering::Relaxed);
        self.set_converting_to_drowned(true);
    }

    /// Vanilla `ConversionTracker.tick` for the water conversion (`Zombie.drowningTracker`).
    fn tick_drowning_conversion(&self) {
        let entity = &self.mob_entity.living_entity.entity;
        if !entity.is_alive() || self.mob_entity.is_no_ai() {
            return;
        }

        if entity.is_submerged_in_water() {
            if self.is_under_water_converting() {
                if self.drowned_conversion_time.fetch_sub(1, Ordering::Relaxed) - 1 < 0 {
                    self.do_underwater_conversion();
                }
            } else if self.in_water_time.fetch_add(1, Ordering::Relaxed) + 1
                >= IN_WATER_TIME_TO_CONVERT
            {
                self.start_drowned_conversion(DROWNED_CONVERSION_TIME);
            }
        } else {
            self.in_water_time.store(NOT_CONVERTING, Ordering::Relaxed);
            self.set_converting_to_drowned(false);
        }
    }

    /// Vanilla `ConversionTracker.doConversion`: `Mob.convertTo` with
    /// `ConversionParams.single(mob, true, true)`.
    fn do_underwater_conversion(&self) {
        let entity = &self.mob_entity.living_entity.entity;
        let world = entity.world.load_full();
        let silent = entity.is_silent();
        let converted = convert_to(
            self,
            self.is_baby(),
            self.kind.converts_to_when_drowning(),
            ConversionParams::single(true, true),
            |converted| {
                if self.can_break_doors()
                    && let Some(zombie) = converted.get_mob().and_then(Mob::as_zombie)
                {
                    zombie.set_can_break_doors(true, zombie);
                }
            },
        );
        if let Some(converted) = converted
            && !silent
        {
            world.sync_world_event(
                self.kind.conversion_sound(),
                converted.get_entity().block_pos.load(),
                0,
            );
        }
    }
}

impl Mob for ZombieEntityBase {
    fn mob_metadata(&self) -> Option<MobMetadata> {
        self.is_baby().then(|| MobMetadata::new().baby(true))
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn as_zombie(&self) -> Option<&ZombieEntityBase> {
        Some(self)
    }

    fn spawn_as_baby(&self) -> bool {
        self.set_baby(true);
        true
    }

    fn mob_tick(&self, _caller: &dyn EntityBase) {
        if self.kind.converts_in_water() {
            self.tick_drowning_conversion();
        }
        if self.kind == ZombieKind::Drowned {
            self.update_drowned_swimming();
        }
    }

    /// Vanilla `Zombie.doHurtTarget` and `Husk.doHurtTarget`.
    fn on_attack(&self, target: &dyn EntityBase) {
        let entity = &self.mob_entity.living_entity.entity;
        let main_hand_empty = self
            .mob_entity
            .living_entity
            .entity_equipment
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&EquipmentSlot::MAIN_HAND)
            .is_empty();
        let difficulty = RegionalDifficulty::at(&entity.world.load_full(), entity.pos.load())
            .effective_difficulty;

        if main_hand_empty
            && entity.is_on_fire()
            && rand::random::<f32>() < difficulty * IGNITE_TARGET_CHANCE_PER_DIFFICULTY
        {
            target.set_on_fire_for(2.0 * difficulty.trunc());
        }

        if self.kind == ZombieKind::Husk
            && main_hand_empty
            && let Some(living) = target.get_living_entity()
        {
            living.add_effect(Effect {
                effect_type: &StatusEffect::HUNGER,
                duration: HUSK_HUNGER_TICKS_PER_DIFFICULTY * difficulty as i32,
                amplifier: 0,
                ambient: false,
                show_particles: true,
                show_icon: true,
                blend: false,
            });
        }
    }

    fn populate_default_equipment_slots(
        &self,
        _world: &Arc<World>,
        difficulty: &RegionalDifficulty,
    ) {
        // Default armor slots (super.populateDefaultEquipmentSlots)
        if rand::random::<f32>()
            < MobEntity::MAX_WEARING_ARMOR_CHANCE * difficulty.special_multiplier
        {
            let mut armor_type = rand::random_range(0..3);
            for _ in 1..=3 {
                if rand::random::<f32>() < MobEntity::WEARING_ARMOR_UPGRADE_MATERIAL_CHANCE {
                    armor_type += 1;
                }
            }

            let partial_chance = if difficulty.base_difficulty == Difficulty::Hard {
                0.1f32
            } else {
                0.25f32
            };

            let living = &self.mob_entity.living_entity;
            let mut equipment = living
                .entity_equipment
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut first = true;

            for slot in &MobEntity::EQUIPMENT_POPULATION_ORDER {
                let current = equipment.get(slot);
                if !first && rand::random::<f32>() < partial_chance {
                    break;
                }
                first = false;
                if current.is_empty()
                    && let Some(item) = MobEntity::get_equipment_for_slot(slot, armor_type)
                {
                    equipment.put(slot, ItemStack::new(1, item));
                }
            }
        }

        let weapon_chance = if difficulty.base_difficulty == Difficulty::Hard {
            0.05f32
        } else {
            0.01f32
        };
        if rand::random::<f32>() < weapon_chance {
            let r = rand::random_range(0..6);
            let weapon_item = match r {
                0 => &Item::IRON_SWORD,
                1 => &Item::IRON_SPEAR,
                _ => &Item::IRON_SHOVEL,
            };
            let living = &self.mob_entity.living_entity;
            let mut equipment = living
                .entity_equipment
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            equipment.put(&EquipmentSlot::MAIN_HAND, ItemStack::new(1, weapon_item));
        }
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        // Vanilla Zombie.addAdditionalSaveData; shared by every zombie-family mob.
        nbt.put_bool("IsBaby", self.is_baby());
        if self.can_break_doors() {
            nbt.put_bool("CanBreakDoors", true);
        }
        nbt.put_int(
            "DrownedConversionTime",
            if self.is_under_water_converting() {
                self.drowned_conversion_time.load(Ordering::Relaxed)
            } else {
                NOT_CONVERTING
            },
        );
        nbt.put_int(
            "InWaterTime",
            if self.mob_entity.living_entity.entity.is_submerged_in_water() {
                self.in_water_time.load(Ordering::Relaxed)
            } else {
                NOT_CONVERTING
            },
        );
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.set_baby(nbt.get_bool("IsBaby").unwrap_or(false));
        if let Some(can_break_doors) = nbt.get_bool("CanBreakDoors") {
            self.set_can_break_doors(can_break_doors, self);
        }
        self.in_water_time
            .store(nbt.get_int("InWaterTime").unwrap_or(0), Ordering::Relaxed);
        let conversion_time = nbt
            .get_int("DrownedConversionTime")
            .unwrap_or(NOT_CONVERTING);
        if conversion_time == NOT_CONVERTING {
            self.set_converting_to_drowned(false);
        } else {
            self.start_drowned_conversion(conversion_time);
        }
    }
}
