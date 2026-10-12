use std::sync::{
    Arc, Weak,
    atomic::{AtomicI32, Ordering},
};

use pumpkin_data::entity::EntityType;
use pumpkin_data::game_event::GameEvent;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::Sound;
use pumpkin_data::tag::{self, Taggable};
use pumpkin_data::world::WorldEvent;
use pumpkin_data::{Block, block_properties::WheatLikeProperties};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::codec::var_int::VarInt;
use pumpkin_util::math::position::BlockPos;
use pumpkin_world::world::BlockFlags;
use rand::RngExt;

use crate::block::blocks::plant::crop::MAX_AGE as CARROT_MAX_AGE;
use crate::entity::{
    Entity, EntityBase,
    ageable::{AgeableData, AgeableMob},
    ai::goal::{
        Controls, Goal, ParentHandle,
        active_target::ActiveTargetGoal,
        avoid_entity::AvoidEntityGoal,
        breed::BreedGoal,
        climb_on_top_of_powder_snow::ClimbOnTopOfPowderSnowGoal,
        escape_danger::EscapeDangerGoal,
        follow_parent::FollowParentGoal,
        look_around::RandomLookAroundGoal,
        look_at_entity::LookAtEntityGoal,
        melee_attack::MeleeAttackGoal,
        move_to_target_pos::{MoveToTargetPos, MoveToTargetPosGoal},
        swim::SwimGoal,
        tempt::TemptGoal,
        wander_around::WanderAroundGoal,
    },
    mob::{Mob, MobEntity},
    passive::animal::Animal,
    player::Player,
};
use crate::world::World;

const TEMPT_ITEMS: &[&Item] = &[&Item::CARROT, &Item::GOLDEN_CARROT, &Item::DANDELION];
/// Vanilla `CarrotBlock` max age.
/// Vanilla `Rabbit.MORE_CARROTS_DELAY`.
const MORE_CARROTS_DELAY: i32 = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(i32)]
pub enum RabbitVariant {
    #[default]
    Brown = 0,
    White = 1,
    Black = 2,
    WhiteSplotched = 3,
    Gold = 4,
    Salt = 5,
    Evil = 99,
}

impl RabbitVariant {
    #[must_use]
    pub const fn from_id(id: i32) -> Self {
        match id {
            1 => Self::White,
            2 => Self::Black,
            3 => Self::WhiteSplotched,
            4 => Self::Gold,
            5 => Self::Salt,
            99 => Self::Evil,
            _ => Self::Brown,
        }
    }

    #[must_use]
    pub const fn id(self) -> i32 {
        self as i32
    }

    #[must_use]
    pub fn random_variant() -> Self {
        let mut rng = rand::rng();
        match rng.random_range(0..6) {
            1 => Self::White,
            2 => Self::Black,
            3 => Self::WhiteSplotched,
            4 => Self::Gold,
            5 => Self::Salt,
            _ => Self::Brown,
        }
    }
}

pub struct RabbitEntity {
    pub mob_entity: MobEntity,
    pub ageable_data: AgeableData,
    pub variant: AtomicI32,
    pub more_carrot_ticks: AtomicI32,
}

impl RabbitEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let variant = RabbitVariant::random_variant();
        let rabbit = Self {
            mob_entity,
            ageable_data: AgeableData::default(),
            variant: AtomicI32::new(variant.id()),
            more_carrot_ticks: AtomicI32::new(0),
        };
        let mob_arc = Arc::new(rabbit);
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
            goal_selector.add_goal(1, EscapeDangerGoal::new(2.2));
            goal_selector.add_goal(1, Box::new(ClimbOnTopOfPowderSnowGoal));
            goal_selector.add_goal(2, BreedGoal::new(0.8));
            goal_selector.add_goal(3, Box::new(TemptGoal::new(1.0, TEMPT_ITEMS, false)));
            goal_selector.add_goal(
                4,
                Box::new(AvoidEntityGoal::new(&EntityType::PLAYER, 8.0, 2.2, 2.2)),
            );
            goal_selector.add_goal(
                4,
                Box::new(AvoidEntityGoal::new(&EntityType::WOLF, 10.0, 2.2, 2.2)),
            );
            goal_selector.add_goal(
                4,
                Box::new(AvoidEntityGoal::new(&EntityType::FOX, 10.0, 2.2, 2.2)),
            );
            goal_selector.add_goal(5, Box::new(FollowParentGoal::new(0.8)));
            goal_selector.add_goal(5, RabbitRaidGardenGoal::new());
            goal_selector.add_goal(6, Box::new(WanderAroundGoal::new(0.6)));
            goal_selector.add_goal(
                11,
                LookAtEntityGoal::with_default(mob_weak, &EntityType::PLAYER, 10.0),
            );
            goal_selector.add_goal(11, Box::new(RandomLookAroundGoal::default()));
        };

        mob_arc
    }

    #[must_use]
    pub fn get_variant(&self) -> RabbitVariant {
        RabbitVariant::from_id(self.variant.load(Ordering::Relaxed))
    }

    /// Vanilla `Rabbit.wantsMoreFood()`: the rabbit eats again once its food cooldown ran out.
    #[must_use]
    pub fn wants_more_food(&self) -> bool {
        self.more_carrot_ticks.load(Ordering::Relaxed) <= 0
    }

    pub fn set_variant(&self, variant: RabbitVariant) {
        self.variant.store(variant.id(), Ordering::Relaxed);
        let entity = self.get_entity();
        entity.set_synced_data(
            pumpkin_data::tracked_data::rabbit::DATA_TYPE_ID,
            VarInt(variant.id()),
        );

        if variant == RabbitVariant::Evil {
            let mut goal_selector = self
                .mob_entity
                .goals_selector
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            goal_selector.add_goal(4, Box::new(MeleeAttackGoal::new(1.4, true)));

            let mut target_selector = self
                .mob_entity
                .target_selector
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            target_selector.add_goal(
                1,
                ActiveTargetGoal::with_default(&self.mob_entity, &EntityType::PLAYER, true),
            );
            target_selector.add_goal(
                2,
                ActiveTargetGoal::with_default(&self.mob_entity, &EntityType::WOLF, true),
            );
            target_selector.add_goal(
                2,
                ActiveTargetGoal::with_default(&self.mob_entity, &EntityType::FOX, true),
            );
        }
    }
}

impl AgeableMob for RabbitEntity {
    fn get_ageable_data(&self) -> &AgeableData {
        &self.ageable_data
    }
}

impl Animal for RabbitEntity {
    fn is_food(&self, item_stack: &ItemStack) -> bool {
        item_stack.item.has_tag(&tag::Item::MINECRAFT_RABBIT_FOOD)
            || TEMPT_ITEMS.iter().any(|i| i.id == item_stack.item.id)
    }
}

impl Mob for RabbitEntity {
    fn as_ageable(&self) -> Option<&dyn AgeableMob> {
        Some(self)
    }

    fn as_animal(&self) -> Option<&dyn Animal> {
        Some(self)
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        self.write_ageable_nbt(nbt);
        nbt.put_int("RabbitType", self.get_variant().id());
        nbt.put_int(
            "MoreCarrotTicks",
            self.more_carrot_ticks.load(Ordering::Relaxed),
        );
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.read_ageable_nbt(nbt);
        if let Some(rabbit_type) = nbt.get_int("RabbitType") {
            self.set_variant(RabbitVariant::from_id(rabbit_type));
        }
        if let Some(ticks) = nbt.get_int("MoreCarrotTicks") {
            self.more_carrot_ticks.store(ticks, Ordering::Relaxed);
        }
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn mob_tick(&self, _caller: &dyn EntityBase) {
        self.ageable_ai_step();
    }

    fn custom_server_ai_step(&self, _caller: &dyn EntityBase) {
        // Vanilla `Rabbit.customServerAiStep` food cooldown. The rabbit jump/landing
        // timers in the same method are not modelled.
        let ticks = self.more_carrot_ticks.load(Ordering::Relaxed);
        if ticks > 0 {
            let mut rng = rand::rng();
            self.more_carrot_ticks
                .store((ticks - rng.random_range(0..3)).max(0), Ordering::Relaxed);
        }
    }

    fn mob_init_data_tracker(&self) {
        let entity = self.get_entity();
        let is_baby = entity.age.load(Ordering::Relaxed) < 0;
        if is_baby {
            entity.set_synced_data(pumpkin_data::tracked_data::rabbit::DATA_BABY_ID, true);
        }
        entity.set_synced_data(
            pumpkin_data::tracked_data::rabbit::DATA_TYPE_ID,
            VarInt(self.get_variant().id()),
        );
    }

    fn mob_interact(&self, player: &Arc<Player>, item_stack: &mut ItemStack) -> bool {
        self.animal_interact(player, item_stack, Sound::EntityRabbitAmbient)
    }
}

/// Vanilla `Rabbit.RaidGardenGoal`: walks to a mature carrot and eats one growth stage.
pub struct RabbitRaidGardenGoal {
    move_to_target_pos_goal: MoveToTargetPosGoal<Self>,
    wants_to_raid: bool,
}

impl RabbitRaidGardenGoal {
    #[must_use]
    pub fn new() -> Box<Self> {
        let mut this = Box::new(Self {
            move_to_target_pos_goal: MoveToTargetPosGoal::with_default(
                ParentHandle::none(),
                0.7,
                16,
            ),
            wants_to_raid: false,
        });

        // SAFETY: `this` is pinned in a `Box` and outlives the `ParentHandle` reference.
        this.move_to_target_pos_goal.move_to_target_pos = unsafe { ParentHandle::new(&this) };
        this
    }

    /// `block_pos` is the farmland (`#minecraft:supports_crops`); the carrot is above it.
    fn is_mature_carrot(world: &World, block_pos: &BlockPos) -> bool {
        if !world
            .get_block(block_pos)
            .has_tag(&tag::Block::MINECRAFT_SUPPORTS_CROPS)
        {
            return false;
        }

        let (block, state_id) = world.get_block_and_state_id(&block_pos.up());
        block.id == Block::CARROTS.id
            && i32::from(WheatLikeProperties::from_state_id(state_id).age) >= CARROT_MAX_AGE
    }
}

impl Goal for RabbitRaidGardenGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        // Vanilla sets both flags only on the tick that runs the search, and only checks
        // mob griefing there, so the cooldown keeps counting down while griefing is off.
        if self.move_to_target_pos_goal.cooldown <= 0 {
            let world = mob.get_entity().world.load();
            if !world.level_info.load().game_rules.mob_griefing {
                return false;
            }
            self.wants_to_raid = mob
                .cast_any()
                .downcast_ref::<RabbitEntity>()
                .is_some_and(RabbitEntity::wants_more_food);
        }

        self.move_to_target_pos_goal.can_start(mob)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        self.wants_to_raid && self.move_to_target_pos_goal.should_continue(mob)
    }

    fn start(&mut self, mob: &dyn Mob) {
        self.move_to_target_pos_goal.start(mob);
    }

    fn tick(&mut self, mob: &dyn Mob) {
        self.move_to_target_pos_goal.tick(mob);

        let target = self.move_to_target_pos_goal.target_pos;
        mob.get_mob_entity()
            .look_control
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .look_at_with_range(
                f64::from(target.0.x) + 0.5,
                f64::from(target.0.y) + 1.0,
                f64::from(target.0.z) + 0.5,
                10.0,
                mob.get_max_head_rotation(),
            );

        if !self.move_to_target_pos_goal.is_reached_target() {
            return;
        }

        let world = mob.get_entity().world.load_full();
        if Self::is_mature_carrot(&world, &target) {
            let crops_pos = target.up();
            let (block, state_id) = world.get_block_and_state_id(&crops_pos);
            let age = i32::from(WheatLikeProperties::from_state_id(state_id).age);
            if age == 0 {
                world.set_block_state(
                    &crops_pos,
                    Block::AIR.default_state.id,
                    BlockFlags::NOTIFY_LISTENERS,
                );
            } else {
                let mut props = WheatLikeProperties::from_state_id(state_id);
                props.age = (age - 1) as u8;
                world.set_block_state(
                    &crops_pos,
                    props.to_state_id(block),
                    BlockFlags::NOTIFY_LISTENERS,
                );
                world.emit_game_event(GameEvent::BlockChange.name(), crops_pos.to_centered_f64());
                world.sync_world_event(
                    WorldEvent::ParticlesDestroyBlock,
                    crops_pos,
                    i32::from(state_id.as_u16()),
                );
            }

            if let Some(rabbit) = mob.cast_any().downcast_ref::<RabbitEntity>() {
                rabbit
                    .more_carrot_ticks
                    .store(MORE_CARROTS_DELAY, Ordering::Relaxed);
            }
        }

        self.wants_to_raid = false;
        self.move_to_target_pos_goal.cooldown = 10;
    }

    fn should_run_every_tick(&self) -> bool {
        self.move_to_target_pos_goal.should_run_every_tick()
    }

    fn controls(&self) -> Controls {
        self.move_to_target_pos_goal.controls()
    }
}

impl MoveToTargetPos for RabbitRaidGardenGoal {
    fn is_target_pos(&self, world: Arc<World>, block_pos: BlockPos) -> bool {
        self.wants_to_raid && Self::is_mature_carrot(&world, &block_pos)
    }
}
