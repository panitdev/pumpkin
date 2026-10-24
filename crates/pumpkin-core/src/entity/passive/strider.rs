use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};

use pumpkin_data::attributes::Attributes;
use pumpkin_data::entity::EntityType;
use pumpkin_data::fluid::Fluid;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_nbt::compound::NbtCompound;

use crate::block::PathComputationType;
use crate::entity::ai::goal::{
    Controls, Goal, ParentHandle,
    breed::BreedGoal,
    escape_danger::EscapeDangerGoal,
    follow_parent::FollowParentGoal,
    look_around::RandomLookAroundGoal,
    look_at_entity::LookAtEntityGoal,
    move_to_target_pos::{MoveToTargetPos, MoveToTargetPosGoal},
    tempt::TemptGoal,
    wander_around::WanderAroundGoal,
};
use crate::entity::ai::pathfinder::node::PathType;
use crate::entity::attributes::{Modifier, ModifierOperation, send_attribute_updates_for_living};
use crate::entity::{
    Entity, EntityBase,
    ageable::{AgeableData, AgeableMob},
    item_steerable::{ItemBasedSteering, ItemSteerable},
    mob::{Mob, MobEntity},
    passive::animal::Animal,
    player::Player,
};
use crate::world::World;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use rand::RngExt;

const TEMPT_ITEMS: &[&Item] = &[&Item::WARPED_FUNGUS, &Item::WARPED_FUNGUS_ON_A_STICK];

/// Vanilla `Strider.SUFFOCATING_MODIFIER`: `ADD_MULTIPLIED_BASE -0.34`.
const SUFFOCATING_MODIFIER_ID: &str = "minecraft:suffocating";
const SUFFOCATING_MODIFIER_AMOUNT: f64 = -0.34;

pub struct StriderEntity {
    pub mob_entity: MobEntity,
    pub ageable_data: AgeableData,
    pub steering: ItemBasedSteering,
    pub saddled: AtomicBool,
    pub suffocating: AtomicBool,
}

impl StriderEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let strider = Self {
            mob_entity,
            ageable_data: AgeableData::default(),
            steering: ItemBasedSteering::default(),
            saddled: AtomicBool::new(false),
            suffocating: AtomicBool::new(false),
        };
        let mob_arc = Arc::new(strider);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        // Striders never path through water, but treat lava and fire as walkable.
        let mut navigator = mob_arc
            .mob_entity
            .navigator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        navigator.set_pathfinding_malus(PathType::Water, -1.0);
        navigator.set_pathfinding_malus(PathType::Lava, 0.0);
        navigator.set_pathfinding_malus(PathType::DangerFire, 0.0);
        navigator.set_pathfinding_malus(PathType::DamageFire, 0.0);
        drop(navigator);

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;

            goal_selector.add_goal(1, EscapeDangerGoal::new(1.65));
            goal_selector.add_goal(2, BreedGoal::new(1.0));
            goal_selector.add_goal(3, Box::new(TemptGoal::new(1.4, TEMPT_ITEMS, false)));
            goal_selector.add_goal(4, StriderGoToLavaGoal::new(1.0));
            goal_selector.add_goal(5, Box::new(FollowParentGoal::new(1.0)));
            goal_selector.add_goal(7, Box::new(WanderAroundGoal::with_interval(1.0, 60)));
            goal_selector.add_goal(
                8,
                LookAtEntityGoal::with_default(mob_weak.clone(), &EntityType::PLAYER, 8.0),
            );
            goal_selector.add_goal(8, Box::new(RandomLookAroundGoal::default()));
            goal_selector.add_goal(
                9,
                LookAtEntityGoal::with_default(mob_weak, &EntityType::STRIDER, 8.0),
            );
        };

        mob_arc
    }

    #[must_use]
    pub fn is_suffocating(&self) -> bool {
        self.suffocating.load(Ordering::Relaxed)
    }

    pub fn set_suffocating(&self, suffocating: bool) {
        let was_suffocating = self.suffocating.swap(suffocating, Ordering::Relaxed);
        let entity = self.get_entity();
        entity.set_synced_data(
            pumpkin_data::tracked_data::strider::DATA_SUFFOCATING,
            suffocating,
        );

        if was_suffocating == suffocating {
            return;
        }

        // Vanilla `Strider.setSuffocating` multiplies the movement speed while out of lava.
        let living = &self.mob_entity.living_entity;
        if suffocating {
            living.update_attribute(&Attributes::MOVEMENT_SPEED, |inst| {
                inst.add_or_replace_modifier(Modifier {
                    id: SUFFOCATING_MODIFIER_ID.to_string(),
                    amount: SUFFOCATING_MODIFIER_AMOUNT,
                    operation: ModifierOperation::MultiplyBase,
                });
            });
        } else {
            living.update_attribute(&Attributes::MOVEMENT_SPEED, |inst| {
                inst.remove_modifier(SUFFOCATING_MODIFIER_ID);
            });
        }
        send_attribute_updates_for_living(living, vec![Attributes::MOVEMENT_SPEED]);
    }

    fn is_being_tempted(&self) -> bool {
        self.mob_entity
            .goals_selector
            .is_goal_running::<TemptGoal>()
    }

    fn is_panicking(&self) -> bool {
        self.mob_entity
            .goals_selector
            .is_goal_running::<EscapeDangerGoal>()
    }

    /// Vanilla `Strider.floatStrider`: bobbing up while in lava, or standing on its surface.
    ///
    /// TODO: vanilla `canStandOnFluid` (lava) and `getLiquidCollisionShape` are approximated by
    /// `LivingEntity::tick_movement`, which skips `travel_in_fluid` for `EntityType::STRIDER`.
    /// `StriderPathNavigation.isStableDestination` (lava is a stable target) has no equivalent.
    fn float_strider(&self) {
        let entity = self.get_entity();
        if !entity.touching_lava.load(Ordering::SeqCst) {
            return;
        }
        let world = entity.world.load();
        let block_pos = entity.block_pos.load();
        // Vanilla `getLiquidCollisionShape` is a 16x16x8 column, so its top is at y + 0.5.
        let above_collision_shape = entity.pos.load().y > f64::from(block_pos.0.y) + 0.5 - 1.0e-5;
        let fluid_above = world.get_fluid(&block_pos.up());

        if above_collision_shape && !fluid_above.matches_type(&Fluid::LAVA) {
            entity.on_ground.store(true, Ordering::SeqCst);
        } else {
            let velocity = entity.velocity.load() * 0.5 + Vector3::new(0.0, 0.05, 0.0);
            entity.velocity.store(velocity);
        }
    }
}

impl AgeableMob for StriderEntity {
    fn get_ageable_data(&self) -> &AgeableData {
        &self.ageable_data
    }
}

impl Animal for StriderEntity {
    fn is_food(&self, item_stack: &ItemStack) -> bool {
        item_stack.item.has_tag(&tag::Item::MINECRAFT_STRIDER_FOOD)
    }
}

impl Mob for StriderEntity {
    fn as_ageable(&self) -> Option<&dyn AgeableMob> {
        Some(self)
    }

    fn as_animal(&self) -> Option<&dyn Animal> {
        Some(self)
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        self.write_ageable_nbt(nbt);
        nbt.put_bool("Saddle", self.is_saddled());
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.read_ageable_nbt(nbt);
        if let Some(saddle) = nbt.get_bool("Saddle") {
            self.set_saddled(saddle);
        } else if let Some(saddle_byte) = nbt.get_byte("Saddle") {
            self.set_saddled(saddle_byte == 1);
        }
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn get_item_steerable(&self) -> Option<&dyn ItemSteerable> {
        Some(self)
    }

    fn is_saddled(&self) -> bool {
        self.saddled.load(Ordering::Relaxed)
    }

    fn can_be_saddled(&self) -> bool {
        self.mob_entity.living_entity.entity.is_alive() && !self.is_baby()
    }

    fn set_saddled(&self, saddled: bool) {
        self.saddled.store(saddled, Ordering::Relaxed);
    }

    fn mob_tick(&self, _caller: &dyn EntityBase) {
        // Vanilla `Strider.tick` sounds.
        if self.is_being_tempted() && self.get_random().random_range(0..140) == 0 {
            self.get_entity().play_sound(Sound::EntityStriderHappy);
        } else if self.is_panicking() && self.get_random().random_range(0..60) == 0 {
            self.get_entity().play_sound(Sound::EntityStriderRetreat);
        }

        if self.mob_entity.is_no_ai() {
            return;
        }

        // Vanilla `Strider.tick` suffocation check: warm while in, on or riding warm blocks.
        let entity = self.get_entity();
        let world = entity.world.load();
        let block_pos = entity.block_pos.load();
        // Vanilla `getBlockStateOnLegacy` is `getOnPos(0.2)`.
        let (inside_block, _) = world.get_block_and_state(&block_pos);
        let on_block = entity.get_block_with_y_offset(0.2).1;
        let in_warm_blocks = inside_block.has_tag(&tag::Block::MINECRAFT_STRIDER_WARM_BLOCKS)
            || on_block.has_tag(&tag::Block::MINECRAFT_STRIDER_WARM_BLOCKS)
            || entity.lava_height.load() > 0.0;
        let on_warm_strider = entity.get_vehicle().is_some_and(|vehicle| {
            vehicle.get_entity().entity_type == &EntityType::STRIDER
                && vehicle
                    .cast_any()
                    .downcast_ref::<Self>()
                    .is_some_and(|strider| !strider.is_suffocating())
        });
        self.set_suffocating(!in_warm_blocks && !on_warm_strider);
    }

    fn post_tick(&self) {
        self.float_strider();
    }

    fn mob_init_data_tracker(&self) {
        let entity = self.get_entity();
        let is_baby = entity.age.load(Ordering::Relaxed) < 0;
        if is_baby {
            entity.set_synced_data(pumpkin_data::tracked_data::strider::DATA_BABY_ID, true);
        }
        entity.set_synced_data(
            pumpkin_data::tracked_data::strider::DATA_SUFFOCATING,
            self.is_suffocating(),
        );
    }

    fn get_walk_target_value(&self, pos: &BlockPos) -> f32 {
        let entity = self.get_entity();
        let world = entity.world.load();
        if world.get_fluid(pos).matches_type(&Fluid::LAVA) {
            return 10.0;
        }
        if entity.touching_lava.load(Ordering::SeqCst) {
            f32::NEG_INFINITY
        } else {
            0.0
        }
    }

    fn mob_interact(&self, player: &Arc<Player>, item_stack: &mut ItemStack) -> bool {
        let item = item_stack.get_item();

        if item == &Item::SADDLE && self.can_be_saddled() && !self.is_saddled() {
            self.set_saddled(true);
            item_stack.decrement_unless_creative(player.gamemode.load(), 1);
            let entity = self.get_entity();
            let world = entity.world.load();
            let pos = entity.pos.load();
            world.play_sound(Sound::EntityStriderSaddle, SoundCategory::Neutral, &pos);
            return true;
        }

        if self.is_saddled() && !self.is_food(item_stack) {
            let world = player.world();
            let ent = &self.mob_entity.living_entity.entity;
            if let Some(vehicle) = world.get_entity_by_id(ent.entity_id)
                && let Some(passenger) = world.get_player_by_id(player.entity_id())
            {
                ent.add_passenger(vehicle, passenger as Arc<dyn EntityBase>);
                return true;
            }
        }

        self.animal_interact(player, item_stack, Sound::EntityStriderAmbient)
    }
}

impl ItemSteerable for StriderEntity {
    fn boost(&self) -> bool {
        self.steering.boost()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Vanilla `Strider.StriderGoToLavaGoal`: strolls toward a lava pool when not already in lava.
pub struct StriderGoToLavaGoal {
    move_to_target_pos_goal: MoveToTargetPosGoal<Self>,
}

impl StriderGoToLavaGoal {
    #[must_use]
    pub fn new(speed: f64) -> Box<Self> {
        let mut this = Box::new(Self {
            move_to_target_pos_goal: MoveToTargetPosGoal::new(ParentHandle::none(), speed, 8, 2),
        });
        // SAFETY: `this` heap allocation address is pinned in Box and outlives `ParentHandle` references.
        this.move_to_target_pos_goal.move_to_target_pos = unsafe { ParentHandle::new(&this) };
        this
    }

    fn is_in_lava(mob: &dyn Mob) -> bool {
        mob.get_entity().touching_lava.load(Ordering::SeqCst)
    }
}

impl MoveToTargetPos for StriderGoToLavaGoal {
    fn is_target_pos(&self, world: Arc<World>, block_pos: BlockPos) -> bool {
        if world.get_block(&block_pos).id != pumpkin_data::Block::LAVA.id {
            return false;
        }
        let above = block_pos.up();
        let (above_block, above_state) = world.get_block_and_state(&above);
        world
            .block_registry
            .is_pathfindable(above_block, above_state, PathComputationType::Land)
    }

    fn get_move_to_target(&self, block_pos: BlockPos) -> BlockPos {
        block_pos
    }

    fn should_recalculate_path(&self, try_ticks: i32) -> bool {
        try_ticks % 20 == 0
    }
}

impl Goal for StriderGoToLavaGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        !Self::is_in_lava(mob) && self.move_to_target_pos_goal.can_start(mob)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        if Self::is_in_lava(mob) {
            return false;
        }
        // Vanilla `canContinueToUse` checks only the lava state and target validity, not the
        // `MoveToBlockGoal` try-ticks bounds.
        let target_pos = self.move_to_target_pos_goal.target_pos;
        let world = mob.get_entity().world.load_full();
        self.is_target_pos(world, target_pos)
    }

    fn start(&mut self, mob: &dyn Mob) {
        self.move_to_target_pos_goal.start(mob);
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.move_to_target_pos_goal.stop(mob);
    }

    fn tick(&mut self, mob: &dyn Mob) {
        self.move_to_target_pos_goal.tick(mob);
    }

    fn should_run_every_tick(&self) -> bool {
        self.move_to_target_pos_goal.should_run_every_tick()
    }

    fn controls(&self) -> Controls {
        self.move_to_target_pos_goal.controls()
    }
}
