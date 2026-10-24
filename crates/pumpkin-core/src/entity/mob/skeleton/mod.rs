use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU8, Ordering};
use std::sync::{Arc, Weak};

use pumpkin_data::data_component_impl::DataComponentImpl;
use pumpkin_data::data_component_impl::{EquipmentSlot, PotionContentsImpl, StatusEffectInstance};
use pumpkin_data::entity::EntityType;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::potion::Effect;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag;
use pumpkin_data::world::WorldEvent;
use pumpkin_data::{attributes::Attributes, effect::StatusEffect, tracked_data};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::position::BlockPos;

use crate::entity::{
    Entity, EntityBase,
    ai::{
        goal::{
            active_target::ActiveTargetGoal, avoid_entity::AvoidEntityGoal,
            bow_attack::BowAttackGoal, flee_sun::FleeSunGoal, look_around::RandomLookAroundGoal,
            look_at_entity::LookAtEntityGoal, melee_attack::MeleeAttackGoal,
            restrict_sun::RestrictSunGoal, revenge::RevengeGoal, wander_around::WanderAroundGoal,
        },
        pathfinder::node::PathType,
    },
    item::ItemEntity,
    mob::{
        Mob, MobEntity, RangedAttackMob,
        conversion::{ConversionParams, convert_to},
    },
    player::Player,
    projectile::arrow::{ArrowEntity, ArrowPickup},
};
use crate::world::World;

/// Forwards a skeleton family type to its `SkeletonEntityBase`, which holds all the behaviour.
macro_rules! skeleton_mob {
    ($ty:ident) => {
        impl $crate::entity::mob::Mob for $ty {
            fn get_mob_entity(&self) -> &$crate::entity::mob::MobEntity {
                &self.entity.mob_entity
            }

            fn mob_tick(&self, caller: &dyn $crate::entity::EntityBase) {
                self.entity.mob_tick(caller);
            }

            fn finalize_spawn(
                &self,
                world: &std::sync::Arc<$crate::world::World>,
                group_data: Option<$crate::entity::mob::spawn::SpawnGroupData>,
            ) -> Option<$crate::entity::mob::spawn::SpawnGroupData> {
                self.entity.finalize_spawn(world, group_data)
            }

            fn mob_write_nbt(&self, nbt: &mut pumpkin_nbt::compound::NbtCompound) {
                self.entity.mob_write_nbt(nbt);
            }

            fn mob_read_nbt(&self, nbt: &pumpkin_nbt::compound::NbtCompound) {
                self.entity.mob_read_nbt(nbt);
            }

            fn on_attack(&self, target: &dyn $crate::entity::EntityBase) {
                self.entity.on_attack(target);
            }

            fn mob_interact(
                &self,
                player: &std::sync::Arc<$crate::entity::player::Player>,
                item_stack: &mut pumpkin_data::item_stack::ItemStack,
            ) -> bool {
                self.entity.mob_interact(player, item_stack)
            }

            fn get_walk_target_value(&self, pos: &pumpkin_util::math::position::BlockPos) -> f32 {
                self.entity.get_walk_target_value(pos)
            }

            fn get_preferred_weapon_type(&self) -> Option<&'static pumpkin_data::tag::Tag> {
                self.entity.get_preferred_weapon_type()
            }
        }
    };
}

pub mod bogged;
pub mod parched;
#[allow(clippy::module_inception)]
pub mod skeleton;
pub mod stray;
pub mod wither;

/// The `AbstractSkeleton` subclasses that share `SkeletonEntityBase`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SkeletonKind {
    Skeleton,
    Stray,
    Bogged,
    WitherSkeleton,
    Parched,
}

const HARD_ATTACK_INTERVAL: i32 = 20;
const NORMAL_ATTACK_INTERVAL: i32 = 40;
const INCREASED_HARD_ATTACK_INTERVAL: i32 = 50;
const INCREASED_NORMAL_ATTACK_INTERVAL: i32 = 70;

const STRAY_FREEZING_TIME: i32 = 140;
const STRAY_CONVERSION_TIME: i32 = 300;
const NOT_CONVERTING: i32 = -1;

const WITHER_SKELETON_ATTACK_DAMAGE: f64 = 4.0;
const WITHER_EFFECT_DURATION: i32 = 200;
const STRAY_SLOWNESS_DURATION: i32 = 600;
const BOGGED_POISON_DURATION: i32 = 100;
const ARROW_SPEED: f64 = 1.6;

const WEAPON_UNASSESSED: u8 = 0;
const WEAPON_MELEE: u8 = 1;
const WEAPON_BOW: u8 = 2;

pub struct SkeletonEntityBase {
    pub mob_entity: MobEntity,
    kind: SkeletonKind,
    self_ref: Weak<Self>,
    /// Which attack goal `reassess_weapon_goal` last registered.
    weapon_goal: AtomicU8,
    /// `Skeleton` freeze conversion (vanilla `ConversionTracker`).
    freezing_time: AtomicI32,
    stray_conversion_time: AtomicI32,
    sheared: AtomicBool,
}

impl SkeletonEntityBase {
    pub fn new(entity: Entity, kind: SkeletonKind) -> Arc<Self> {
        let mob_arc = Arc::new_cyclic(|self_ref| Self {
            mob_entity: MobEntity::new(entity),
            kind,
            self_ref: self_ref.clone(),
            weapon_goal: AtomicU8::new(WEAPON_UNASSESSED),
            freezing_time: AtomicI32::new(0),
            stray_conversion_time: AtomicI32::new(NOT_CONVERTING),
            sheared: AtomicBool::new(false),
        });
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        if kind == SkeletonKind::WitherSkeleton {
            mob_arc
                .mob_entity
                .navigator
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .set_pathfinding_malus(PathType::Lava, 8.0);
        }

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;
            let target_selector = &mob_arc.mob_entity.target_selector;

            if kind == SkeletonKind::WitherSkeleton {
                // AbstractPiglin targets
                target_selector.add_goal(
                    3,
                    ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::PIGLIN, true),
                );
                target_selector.add_goal(
                    3,
                    ActiveTargetGoal::with_default(
                        &mob_arc.mob_entity,
                        &EntityType::PIGLIN_BRUTE,
                        true,
                    ),
                );
            }

            goal_selector.add_goal(2, Box::new(RestrictSunGoal::new()));
            goal_selector.add_goal(3, Box::new(FleeSunGoal::new(1.0)));
            goal_selector.add_goal(
                3,
                Box::new(AvoidEntityGoal::new(&EntityType::WOLF, 6.0, 1.0, 1.2)),
            );
            goal_selector.add_goal(5, Box::new(WanderAroundGoal::new(1.0)));
            goal_selector.add_goal(
                6,
                LookAtEntityGoal::with_default(mob_weak, &EntityType::PLAYER, 8.0),
            );
            goal_selector.add_goal(6, Box::new(RandomLookAroundGoal::default()));

            target_selector.add_goal(1, Box::new(RevengeGoal::new(true)));
            target_selector.add_goal(
                2,
                ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::PLAYER, true),
            );
            target_selector.add_goal(
                3,
                ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::IRON_GOLEM, true),
            );
            target_selector.add_goal(
                3,
                ActiveTargetGoal::baby_turtle_on_land(&mob_arc.mob_entity),
            );
        };

        mob_arc
    }

    const fn entity(&self) -> &Entity {
        &self.mob_entity.living_entity.entity
    }

    fn hard_attack_interval(&self) -> i32 {
        if self.kind == SkeletonKind::Bogged {
            INCREASED_HARD_ATTACK_INTERVAL
        } else {
            HARD_ATTACK_INTERVAL
        }
    }

    fn attack_interval(&self) -> i32 {
        if self.kind == SkeletonKind::Bogged {
            INCREASED_NORMAL_ATTACK_INTERVAL
        } else {
            NORMAL_ATTACK_INTERVAL
        }
    }

    fn held_item(&self, slot: &EquipmentSlot) -> ItemStack {
        self.mob_entity
            .living_entity
            .entity_equipment
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(slot)
    }

    /// Vanilla `ProjectileUtil.getWeaponHoldingHand` for the bow, as the held stack.
    fn bow_stack(&self) -> ItemStack {
        let main_hand = self.held_item(&EquipmentSlot::MAIN_HAND);
        if main_hand.item.id == Item::BOW.id {
            main_hand
        } else {
            self.held_item(&EquipmentSlot::OFF_HAND)
        }
    }

    /// Vanilla `AbstractSkeleton.reassessWeaponGoal`: a bow gets the bow goal, anything else melee.
    fn reassess_weapon_goal(&self) {
        let uses_bow = self.bow_stack().item.id == Item::BOW.id;
        let state = if uses_bow { WEAPON_BOW } else { WEAPON_MELEE };
        if self.weapon_goal.swap(state, Ordering::Relaxed) == state {
            return;
        }

        let goal_selector = &self.mob_entity.goals_selector;
        goal_selector.remove_goal::<MeleeAttackGoal>(self);
        goal_selector.remove_goal::<BowAttackGoal>(self);
        if uses_bow {
            let world = self.entity().world.load();
            let min_attack_interval =
                if world.level_info.load().difficulty == pumpkin_util::Difficulty::Hard {
                    self.hard_attack_interval()
                } else {
                    self.attack_interval()
                };
            let ranged: Weak<dyn RangedAttackMob> = self.self_ref.clone();
            goal_selector.add_goal(
                4,
                Box::new(BowAttackGoal::new(ranged, 1.0, min_attack_interval, 15.0)),
            );
        } else {
            goal_selector.add_goal(4, Box::new(MeleeAttackGoal::new(1.2, false)));
        }
    }

    /// The arrow item a shot carries; the effect rides along as potion contents like vanilla's
    /// `Arrow.addEffect`.
    fn projectile(&self) -> ItemStack {
        let mut projectile = ItemStack::new(1, &Item::ARROW);
        let effect = match self.kind {
            SkeletonKind::Stray => Some(("minecraft:slowness", STRAY_SLOWNESS_DURATION)),
            SkeletonKind::Bogged => Some(("minecraft:poison", BOGGED_POISON_DURATION)),
            _ => None,
        };
        if let Some((effect_id, duration)) = effect {
            projectile.patch.push((
                pumpkin_data::data_component::DataComponent::PotionContents,
                Some(
                    PotionContentsImpl {
                        potion_id: None,
                        custom_color: None,
                        custom_effects: vec![StatusEffectInstance {
                            effect_id: effect_id.into(),
                            amplifier: 0,
                            duration,
                            ambient: false,
                            show_particles: true,
                            show_icon: true,
                        }],
                        custom_name: None,
                    }
                    .to_dyn(),
                ),
            ));
        }
        projectile
    }

    /// Vanilla `AbstractSkeleton.getArrow` and the subclass overrides.
    fn get_arrow(
        &self,
        projectile: &ItemStack,
        power: f32,
        bow_item: &ItemStack,
        difficulty: i32,
    ) -> ArrowEntity {
        let entity = self.entity();
        let arrow_entity = Entity::new(
            entity.world.load_full(),
            entity.pos.load(),
            &EntityType::ARROW,
        );
        let arrow = ArrowEntity::new_shot_with_weapon(
            arrow_entity,
            entity,
            projectile,
            bow_item,
            ArrowPickup::Disallowed,
        );
        arrow.set_base_damage_from_mob(f64::from(power), difficulty);
        if self.kind == SkeletonKind::WitherSkeleton {
            arrow.set_flame(true);
        }
        arrow
    }

    fn tick_freeze_conversion(&self) {
        let entity = self.entity();
        if !entity.is_alive() || self.mob_entity.is_no_ai() {
            return;
        }

        if entity.is_in_powder_snow() {
            if self.is_converting_to_stray() {
                if self.stray_conversion_time.fetch_sub(1, Ordering::Relaxed) - 1 < 0 {
                    self.convert_to_stray();
                }
            } else if self.freezing_time.fetch_add(1, Ordering::Relaxed) + 1 >= STRAY_FREEZING_TIME
            {
                self.start_stray_conversion(STRAY_CONVERSION_TIME);
            }
        } else {
            self.freezing_time.store(-1, Ordering::Relaxed);
            self.set_converting_to_stray(false);
        }
    }

    fn is_converting_to_stray(&self) -> bool {
        self.stray_conversion_time.load(Ordering::Relaxed) != NOT_CONVERTING
    }

    fn set_converting_to_stray(&self, converting: bool) {
        if !converting {
            self.stray_conversion_time
                .store(NOT_CONVERTING, Ordering::Relaxed);
        }
        self.entity()
            .set_synced_data(tracked_data::skeleton::DATA_STRAY_CONVERSION_ID, converting);
    }

    fn start_stray_conversion(&self, time: i32) {
        self.stray_conversion_time.store(time, Ordering::Relaxed);
        self.set_converting_to_stray(true);
    }

    /// Vanilla `Mob.convertTo` with `ConversionParams.single(mob, true, true)`.
    fn convert_to_stray(&self) {
        let silent = self.entity().is_silent();
        let block_pos = self.entity().block_pos.load();
        let world = self.entity().world.load_full();
        if convert_to(
            self,
            false,
            &EntityType::STRAY,
            ConversionParams::single(true, true),
            |_| {},
        )
        .is_some()
            && !silent
        {
            world.sync_world_event(WorldEvent::SoundSkeletonToStray, block_pos, 0);
        }
    }

    /// Vanilla `Bogged.shear`.
    fn shear(&self, player: &Arc<Player>) -> bool {
        let entity = self.entity();
        let world = entity.world.load_full();
        let pos = entity.pos.load();
        world.play_sound(Sound::EntityBoggedShear, SoundCategory::Players, &pos);

        if let Some(table) = world.get_loot_table("minecraft:shearing/bogged") {
            let mut drop_pos = pos;
            drop_pos.y += f64::from(entity.height());
            for drop in table.generate_loot(rand::random()) {
                world.spawn_entity(Arc::new(ItemEntity::new(
                    Entity::new(world.clone(), drop_pos, &EntityType::ITEM),
                    drop,
                )));
            }
        }
        self.set_sheared(true);
        player.damage_held_item(1);
        true
    }

    fn set_sheared(&self, sheared: bool) {
        self.sheared.store(sheared, Ordering::Relaxed);
        self.entity()
            .set_synced_data(tracked_data::bogged::DATA_SHEARED, sheared);
    }
}

impl RangedAttackMob for SkeletonEntityBase {
    /// Vanilla `AbstractSkeleton.performRangedAttack`.
    fn perform_ranged_attack(&self, target: &Arc<dyn EntityBase>, power: f32) {
        let entity = self.entity();
        let world = entity.world.load();
        let world_full = entity.world.load_full();

        let bow_item = self.bow_stack();
        let projectile = self.projectile();
        let difficulty = world.level_info.load().difficulty as i32;
        let arrow = self.get_arrow(&projectile, power, &bow_item, difficulty);

        arrow.set_base_damage(crate::enchantment::EnchantmentHelper::modify_damage(
            &bow_item,
            arrow.get_base_damage(),
        ));
        arrow.punch_level.store(
            crate::enchantment::EnchantmentHelper::modify_knockback(&bow_item, 0.0) as u8,
            Ordering::Relaxed,
        );
        arrow.apply_on_projectile_spawned(&projectile);

        let mob_pos = entity.pos.load();
        let target_entity = target.get_entity();
        let target_pos = target_entity.pos.load();

        let dx = target_pos.x - mob_pos.x;
        let dy = (target_pos.y + f64::from(target_entity.entity_dimension.load().height) / 3.0)
            - arrow.entity.pos.load().y;
        let dz = target_pos.z - mob_pos.z;
        let horizontal_distance = dx.hypot(dz);

        // RangedAttackMob.rangedAttackUncertainty
        let divergence = f64::from(14 - difficulty * 4);

        arrow.set_velocity(
            dx,
            horizontal_distance.mul_add(0.2, dy),
            dz,
            ARROW_SPEED,
            divergence,
        );

        world.play_sound_fine(
            Sound::EntitySkeletonShoot,
            SoundCategory::Hostile,
            &mob_pos,
            1.0,
            1.0 / (rand::random::<f32>() * 0.4 + 0.8),
        );

        let arrow: Arc<dyn EntityBase> = Arc::new(arrow);
        if let Some(server) = world_full.server.upgrade() {
            let mut event =
                crate::plugin::api::events::entity::entity_shoot_bow::EntityShootBowEvent::new(
                    entity.entity_id,
                    "minecraft:bow".to_string(),
                    1.0,
                );
            server.plugin_manager.fire_blocking(&server, &mut event);
            if event.cancelled {
                return;
            }
        }
        world_full.spawn_entity(arrow);
    }
}

impl Mob for SkeletonEntityBase {
    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn mob_tick(&self, _caller: &dyn EntityBase) {
        // No equipment hook exists, so the held item is compared every tick.
        self.reassess_weapon_goal();
        if self.kind == SkeletonKind::Skeleton {
            self.tick_freeze_conversion();
        }
    }

    fn finalize_spawn(
        &self,
        _world: &Arc<World>,
        group_data: Option<crate::entity::mob::spawn::SpawnGroupData>,
    ) -> Option<crate::entity::mob::spawn::SpawnGroupData> {
        self.mob_entity.finalize_spawn_base();
        if self.kind == SkeletonKind::WitherSkeleton {
            self.mob_entity
                .living_entity
                .set_attribute_base(&Attributes::ATTACK_DAMAGE, WITHER_SKELETON_ATTACK_DAMAGE);
        }
        group_data
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        match self.kind {
            SkeletonKind::Skeleton => {
                nbt.put_int(
                    "StrayConversionTime",
                    if self.is_converting_to_stray() {
                        self.stray_conversion_time.load(Ordering::Relaxed)
                    } else {
                        NOT_CONVERTING
                    },
                );
                nbt.put_int(
                    "FreezingTime",
                    if self.entity().is_in_powder_snow() {
                        self.freezing_time.load(Ordering::Relaxed)
                    } else {
                        NOT_CONVERTING
                    },
                );
            }
            SkeletonKind::Bogged => nbt.put_bool("sheared", self.sheared.load(Ordering::Relaxed)),
            _ => {}
        }
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        match self.kind {
            SkeletonKind::Skeleton => {
                self.freezing_time
                    .store(nbt.get_int("FreezingTime").unwrap_or(0), Ordering::Relaxed);
                let conversion_time = nbt.get_int("StrayConversionTime").unwrap_or(NOT_CONVERTING);
                if conversion_time == NOT_CONVERTING {
                    self.set_converting_to_stray(false);
                } else {
                    self.start_stray_conversion(conversion_time);
                }
            }
            SkeletonKind::Bogged => {
                self.set_sheared(nbt.get_bool("sheared").unwrap_or(false));
            }
            _ => {}
        }
    }

    /// Vanilla `WitherSkeleton.doHurtTarget`.
    fn on_attack(&self, target: &dyn EntityBase) {
        if self.kind != SkeletonKind::WitherSkeleton {
            return;
        }
        if let Some(living) = target.get_living_entity() {
            living.add_effect(Effect {
                effect_type: &StatusEffect::WITHER,
                duration: WITHER_EFFECT_DURATION,
                amplifier: 0,
                ambient: false,
                show_particles: true,
                show_icon: true,
                blend: false,
            });
        }
    }

    /// Vanilla `Bogged.mobInteract`.
    fn mob_interact(&self, player: &Arc<Player>, item_stack: &mut ItemStack) -> bool {
        if self.kind == SkeletonKind::Bogged
            && item_stack.get_item() == &Item::SHEARS
            && !self.sheared.load(Ordering::Relaxed)
        {
            return self.shear(player);
        }
        self.mob_entity
            .mob_interact(player, item_stack, self.can_be_leashed())
    }

    /// Vanilla `Monster.getWalkTargetValue`: monsters prefer dark places.
    fn get_walk_target_value(&self, pos: &BlockPos) -> f32 {
        -(self
            .entity()
            .world
            .load()
            .get_light_level_dependent_magic_value(pos)
            - 0.5)
    }

    fn get_preferred_weapon_type(&self) -> Option<&'static tag::Tag> {
        (self.kind != SkeletonKind::WitherSkeleton)
            .then_some(&tag::Item::MINECRAFT_SKELETON_PREFERRED_WEAPONS)
    }
}
