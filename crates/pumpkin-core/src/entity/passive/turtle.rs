use std::sync::{
    Arc, Mutex, Weak,
    atomic::{AtomicBool, AtomicI32, Ordering},
};

use pumpkin_data::attributes::Attributes;
use pumpkin_data::block_properties::TurtleEggLikeProperties;
use pumpkin_data::entity::EntityType;
use pumpkin_data::fluid::Fluid;
use pumpkin_data::game_event::GameEvent;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_data::world::WorldEvent;
use pumpkin_data::{Block, BlockStateId};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector2::Vector2;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_world::world::BlockFlags;
use rand::RngExt;

use crate::block::blocks::turtle_egg::TurtleEggBlock;
use crate::entity::{
    Entity, EntityBase,
    ageable::{AgeableData, AgeableMob},
    ai::control::{
        Control, MoveControlTrait,
        move_control::{MoveControl, Operation},
    },
    ai::goal::{
        Controls, Goal, ParentHandle,
        breed::BreedGoal,
        escape_danger::EscapeDangerGoal,
        look_at_entity::LookAtEntityGoal,
        move_to_target_pos::{MoveToTargetPos, MoveToTargetPosGoal},
        tempt::TemptGoal,
        to_goal_ticks,
        wander_around::WanderAroundGoal,
    },
    ai::pathfinder::{Navigator, NavigatorGoal},
    ai::util::default_random_pos,
    experience_orb::ExperienceOrbEntity,
    mob::{Mob, MobEntity, spawn::SpawnGroupData},
    passive::animal::Animal,
    player::Player,
};
use crate::world::World;

const TEMPT_ITEMS: &[&Item] = &[&Item::SEAGRASS];
/// Vanilla `Turtle.TurtleGoHomeGoal.GIVE_UP_TICKS`.
const GO_HOME_GIVE_UP_TICKS: i32 = 600;
/// Vanilla `Turtle.TurtleTravelGoal` horizontal search distance (`xzDist`).
const TRAVEL_XZ_DIST: i32 = 512;
/// Vanilla `Turtle.TurtleTravelGoal` vertical search distance (`yDist`).
const TRAVEL_Y_DIST: i32 = 4;
/// Vanilla `Turtle.TurtleTravelGoal` chunk radius the target area must cover.
const TRAVEL_CHUNK_RADIUS: i32 = 34;

pub struct TurtleEntity {
    pub mob_entity: MobEntity,
    pub ageable_data: AgeableData,
    pub has_egg: AtomicBool,
    pub laying_egg: AtomicBool,
    pub lay_egg_counter: AtomicI32,
    pub home_pos: Mutex<BlockPos>,
    pub going_home: AtomicBool,
    pub travel_pos: Mutex<Option<BlockPos>>,
}

impl TurtleEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        // Vanilla evaluates `isBaby()` when `registerGoals` runs, before the age is restored.
        let is_baby = entity.age.load(Ordering::Relaxed) < 0;
        let mob_entity = MobEntity::new(entity);
        let turtle = Self {
            mob_entity,
            ageable_data: AgeableData::default(),
            has_egg: AtomicBool::new(false),
            laying_egg: AtomicBool::new(false),
            lay_egg_counter: AtomicI32::new(0),
            home_pos: Mutex::new(BlockPos::ZERO),
            going_home: AtomicBool::new(false),
            travel_pos: Mutex::new(None),
        };
        let mob_arc = Arc::new(turtle);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        *mob_arc
            .mob_entity
            .navigator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Navigator::amphibious(false);

        *mob_arc
            .mob_entity
            .move_control
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Box::new(TurtleMoveControl::default());

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;

            goal_selector.add_goal(0, TurtlePanicGoal::new(1.2));
            goal_selector.add_goal(1, TurtleBreedGoal::new(1.0));
            goal_selector.add_goal(1, TurtleLayEggGoal::new(1.0));
            goal_selector.add_goal(2, Box::new(TemptGoal::new(1.1, TEMPT_ITEMS, false)));
            goal_selector.add_goal(3, TurtleGoToWaterGoal::new(1.0, is_baby));
            goal_selector.add_goal(4, TurtleGoHomeGoal::new(1.0));
            goal_selector.add_goal(7, TurtleTravelGoal::new(1.0));
            goal_selector.add_goal(
                8,
                LookAtEntityGoal::with_default(mob_weak, &EntityType::PLAYER, 8.0),
            );
            goal_selector.add_goal(9, TurtleRandomStrollGoal::new(1.0, 100));
        };

        mob_arc
    }

    #[must_use]
    pub fn has_egg(&self) -> bool {
        self.has_egg.load(Ordering::Relaxed)
    }

    pub fn set_has_egg(&self, has_egg: bool) {
        self.has_egg.store(has_egg, Ordering::Relaxed);
        let entity = self.get_entity();
        entity.set_synced_data(pumpkin_data::tracked_data::turtle::HAS_EGG, has_egg);
    }

    #[must_use]
    pub fn is_laying_egg(&self) -> bool {
        self.laying_egg.load(Ordering::Relaxed)
    }

    fn lay_egg_counter(&self) -> i32 {
        self.lay_egg_counter.load(Ordering::Relaxed)
    }

    fn increment_lay_egg_counter(&self) {
        self.lay_egg_counter.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_laying_egg(&self, laying_egg: bool) {
        self.lay_egg_counter
            .store(i32::from(laying_egg), Ordering::Relaxed);
        self.laying_egg.store(laying_egg, Ordering::Relaxed);
        let entity = self.get_entity();
        entity.set_synced_data(pumpkin_data::tracked_data::turtle::LAYING_EGG, laying_egg);
    }

    #[must_use]
    pub fn home_pos(&self) -> BlockPos {
        *self
            .home_pos
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn set_home_pos(&self, pos: BlockPos) {
        *self
            .home_pos
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = pos;
    }

    #[must_use]
    pub fn is_going_home(&self) -> bool {
        self.going_home.load(Ordering::Relaxed)
    }

    pub fn set_going_home(&self, going_home: bool) {
        self.going_home.store(going_home, Ordering::Relaxed);
    }

    #[must_use]
    pub fn travel_pos(&self) -> Option<BlockPos> {
        *self
            .travel_pos
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn set_travel_pos(&self, pos: Option<BlockPos>) {
        *self
            .travel_pos
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = pos;
    }
}

/// Vanilla `Vec3i.closerToCenterThan`: the block centre is within `distance` of `pos`.
fn closer_to_center_than(block_pos: BlockPos, pos: &Vector3<f64>, distance: f64) -> bool {
    block_pos.to_centered_f64().squared_distance_to_vec(pos) < distance * distance
}

fn as_turtle(mob: &dyn Mob) -> Option<&TurtleEntity> {
    mob.cast_any().downcast_ref::<TurtleEntity>()
}

/// Tracks whether every chunk covering the block range is loaded (`LevelReader.hasChunksAt`).
fn has_chunks_at(world: &World, x0: i32, z0: i32, x1: i32, z1: i32) -> bool {
    ((x0 >> 4)..=(x1 >> 4)).all(|chunk_x| {
        ((z0 >> 4)..=(z1 >> 4))
            .all(|chunk_z| world.level.is_chunk_loaded(&Vector2::new(chunk_x, chunk_z)))
    })
}

impl AgeableMob for TurtleEntity {
    fn get_ageable_data(&self) -> &AgeableData {
        &self.ageable_data
    }
}

impl Animal for TurtleEntity {
    fn is_food(&self, item_stack: &ItemStack) -> bool {
        item_stack.item.has_tag(&tag::Item::MINECRAFT_TURTLE_FOOD)
            || item_stack.item == &Item::SEAGRASS
    }

    /// Vanilla `Turtle.canFallInLove`: no loving while carrying an egg.
    fn can_fall_in_love(&self) -> bool {
        !self.is_in_love() && !self.has_egg()
    }
}

impl Mob for TurtleEntity {
    fn as_ageable(&self) -> Option<&dyn AgeableMob> {
        Some(self)
    }

    fn as_animal(&self) -> Option<&dyn Animal> {
        Some(self)
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        self.write_ageable_nbt(nbt);
        nbt.put_bool("has_egg", self.has_egg());
        let home = self.home_pos();
        nbt.put(
            "home_pos",
            NbtTag::IntArray(vec![home.0.x, home.0.y, home.0.z]),
        );
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        if let Some(&[x, y, z]) = nbt.get_int_array("home_pos") {
            self.set_home_pos(BlockPos::new(x, y, z));
        } else {
            self.set_home_pos(self.get_entity().block_pos.load());
        }
        self.read_ageable_nbt(nbt);
        // `HasEgg` is the pre-port key; read it too so existing worlds keep their egg state.
        if let Some(has_egg) = nbt.get_bool("has_egg").or_else(|| nbt.get_bool("HasEgg")) {
            self.set_has_egg(has_egg);
        }
    }

    fn finalize_spawn(
        &self,
        _world: &Arc<World>,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        self.set_home_pos(self.get_entity().block_pos.load());
        self.mob_entity.finalize_spawn_base();
        group_data
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    /// Vanilla `Turtle.isPushedByFluid`.
    fn mob_is_pushed_by_fluids(&self) -> bool {
        false
    }

    /// Vanilla `Turtle.travelInWater`: faster than the generic water travel, drifts toward home.
    fn travel_in_water(&self, caller: &dyn EntityBase, _gravity: f64, _falling: bool) {
        let movement_input = self.get_mob_entity().living_entity.movement_input.load();
        let entity = self.get_entity();
        entity.update_velocity_from_input(movement_input, 0.1);
        entity.move_entity(caller, entity.velocity.load());

        let mut velocity = entity.velocity.load() * 0.9;
        if self.get_mob_entity().get_target().is_none()
            && (!self.is_going_home()
                || !closer_to_center_than(self.home_pos(), &entity.pos.load(), 20.0))
        {
            velocity.y -= 0.005;
        }
        entity.velocity.store(velocity);
    }

    fn get_walk_target_value(&self, pos: &BlockPos) -> f32 {
        let world = self.get_entity().world.load();
        if !self.is_going_home() && world.get_fluid(pos).matches_type(&Fluid::WATER) {
            return 10.0;
        }
        if TurtleEggBlock::on_sand(world.as_ref(), pos) {
            10.0
        } else {
            world.get_light_level_dependent_magic_value(pos)
        }
    }

    /// Vanilla `Turtle.TurtlePathNavigation.isStableDestination`.
    fn is_stable_destination(&self, world: &World, pos: &BlockPos) -> bool {
        if self.travel_pos().is_some() {
            world.get_block(pos).id == Block::WATER.id
        } else {
            !world.get_block_state(&pos.down()).is_air()
        }
    }

    fn mob_tick(&self, _caller: &dyn EntityBase) {
        // Vanilla `Turtle.aiStep`: sand dust while the lay-egg counter ticks.
        let entity = self.get_entity();
        if entity.is_alive()
            && self.is_laying_egg()
            && self.lay_egg_counter() >= 1
            && self.lay_egg_counter() % 5 == 0
        {
            let pos = entity.block_pos.load();
            let world = entity.world.load();
            if TurtleEggBlock::on_sand(world.as_ref(), &pos) {
                world.sync_world_event(
                    WorldEvent::ParticlesAndSoundDestroyBlock,
                    pos,
                    i32::from(world.get_block_state_id(&pos.down()).as_u16()),
                );
                world.emit_game_event(GameEvent::EntityAction.name(), pos.to_centered_f64());
            }
        }
    }

    fn mob_init_data_tracker(&self) {
        let entity = self.get_entity();
        let is_baby = entity.age.load(Ordering::Relaxed) < 0;
        if is_baby {
            entity.set_synced_data(pumpkin_data::tracked_data::turtle::DATA_BABY_ID, true);
        }
        entity.set_synced_data(pumpkin_data::tracked_data::turtle::HAS_EGG, self.has_egg());
        entity.set_synced_data(
            pumpkin_data::tracked_data::turtle::LAYING_EGG,
            self.is_laying_egg(),
        );
    }

    fn mob_interact(&self, player: &Arc<Player>, item_stack: &mut ItemStack) -> bool {
        self.animal_interact(player, item_stack, Sound::EntityTurtleAmbientLand)
    }
}

/// Vanilla `Turtle.TurtleBreedGoal`: breeds only while the turtle carries no egg, and sets the
/// egg flag on the mother instead of spawning a child.
pub struct TurtleBreedGoal {
    inner: Box<BreedGoal>,
}

impl TurtleBreedGoal {
    #[must_use]
    pub fn new(speed: f64) -> Box<Self> {
        Box::new(Self {
            inner: BreedGoal::with_breed_action(speed, breed_turtle),
        })
    }
}

impl Goal for TurtleBreedGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        as_turtle(mob).is_some_and(|turtle| !turtle.has_egg()) && self.inner.can_start(mob)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        self.inner.should_continue(mob)
    }

    fn start(&mut self, mob: &dyn Mob) {
        self.inner.start(mob);
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.inner.stop(mob);
    }

    fn tick(&mut self, mob: &dyn Mob) {
        self.inner.tick(mob);
    }

    fn controls(&self) -> Controls {
        self.inner.controls()
    }
}

/// Vanilla `Turtle.TurtleBreedGoal.breed`: award breeding progress and set `has_egg`.
fn breed_turtle(mob: &dyn Mob, partner: &dyn EntityBase) {
    let mob_entity = mob.get_mob_entity();
    let entity = mob.get_entity();
    let world = entity.world.load();

    let love_cause = mob_entity.breeder.load().or_else(|| {
        partner
            .get_mob()
            .and_then(|partner| partner.get_mob_entity().breeder.load())
    });
    if let Some(uuid) = love_cause
        && let Some(player) = world.get_player_by_uuid(uuid)
    {
        player.increment_stat(
            pumpkin_data::statistic::StatisticCategory::Custom,
            pumpkin_data::statistic::CustomStatistic::AnimalsBred as i32,
            1,
        );
        player.trigger_advancement_criterion(
            pumpkin_data::advancement::Advancement::HUSBANDRY_BREED_AN_ANIMAL,
            "bred",
        );
        player.trigger_advancement_criterion(
            pumpkin_data::advancement::Advancement::HUSBANDRY_BRED_ALL_ANIMALS,
            &format!("minecraft:{}", entity.entity_type.resource_name),
        );
    }

    let Some(turtle) = as_turtle(mob) else {
        return;
    };
    turtle.set_has_egg(true);
    mob.set_breeding_cooldown(6000);
    partner.set_breeding_cooldown(6000);
    mob_entity.reset_love_ticks();
    partner.reset_love();

    if world.level_info.load().game_rules.mob_drops {
        ExperienceOrbEntity::spawn(
            &entity.world.load_full(),
            entity.pos.load(),
            mob.get_random().random_range(1..8),
        );
    }
}

/// Vanilla `Turtle.TurtleLayEggGoal`: walk onto sand near home, then lay the egg.
pub struct TurtleLayEggGoal {
    move_to_target_pos_goal: MoveToTargetPosGoal<Self>,
}

impl TurtleLayEggGoal {
    #[must_use]
    pub fn new(speed: f64) -> Box<Self> {
        let mut this = Box::new(Self {
            move_to_target_pos_goal: MoveToTargetPosGoal::new(ParentHandle::none(), speed, 16, 1),
        });
        // SAFETY: `this` heap allocation address is pinned in Box and outlives `ParentHandle` references.
        this.move_to_target_pos_goal.move_to_target_pos = unsafe { ParentHandle::new(&this) };
        this
    }
}

impl MoveToTargetPos for TurtleLayEggGoal {
    fn is_target_pos(&self, world: Arc<World>, block_pos: BlockPos) -> bool {
        world.get_block_state(&block_pos.up()).is_air()
            && TurtleEggBlock::is_sand(world.as_ref(), &block_pos)
    }
}

impl Goal for TurtleLayEggGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(turtle) = as_turtle(mob) else {
            return false;
        };
        turtle.has_egg()
            && closer_to_center_than(turtle.home_pos(), &mob.get_entity().pos.load(), 9.0)
            && self.move_to_target_pos_goal.can_start(mob)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        let Some(turtle) = as_turtle(mob) else {
            return false;
        };
        self.move_to_target_pos_goal.should_continue(mob)
            && turtle.has_egg()
            && closer_to_center_than(turtle.home_pos(), &mob.get_entity().pos.load(), 9.0)
    }

    fn start(&mut self, mob: &dyn Mob) {
        self.move_to_target_pos_goal.start(mob);
    }

    fn tick(&mut self, mob: &dyn Mob) {
        self.move_to_target_pos_goal.tick(mob);

        let Some(turtle) = as_turtle(mob) else {
            return;
        };
        if mob.get_entity().is_in_water() || !self.move_to_target_pos_goal.is_reached_target() {
            return;
        }

        if turtle.lay_egg_counter() < 1 {
            turtle.set_laying_egg(true);
        } else if turtle.lay_egg_counter() > self.get_tick_count(200) {
            let world = mob.get_entity().world.load();
            let pos = mob.get_entity().block_pos.load();
            let pitch = 0.9 + mob.get_random().random::<f32>() * 0.2;
            world.play_sound_raw(
                Sound::EntityTurtleLayEgg as u16,
                SoundCategory::Blocks,
                &pos.to_centered_f64(),
                0.3,
                pitch,
            );

            let egg_pos = self.move_to_target_pos_goal.target_pos.up();
            let state_id = Self::egg_block_state(mob.get_random().random_range(0..4) + 1);
            world.set_block_state(&egg_pos, state_id, BlockFlags::NOTIFY_ALL);
            world.emit_game_event(GameEvent::BlockPlace.name(), egg_pos.to_centered_f64());

            turtle.set_has_egg(false);
            turtle.set_laying_egg(false);
            let breeder = mob.get_mob_entity().breeder.load();
            mob.get_mob_entity().set_love_ticks(600, breeder);
        }

        if turtle.is_laying_egg() {
            turtle.increment_lay_egg_counter();
        }
    }

    fn should_run_every_tick(&self) -> bool {
        self.move_to_target_pos_goal.should_run_every_tick()
    }

    fn controls(&self) -> Controls {
        self.move_to_target_pos_goal.controls()
    }
}

impl TurtleLayEggGoal {
    fn egg_block_state(eggs: i32) -> BlockStateId {
        let mut properties = TurtleEggLikeProperties::default(&Block::TURTLE_EGG);
        properties.eggs = eggs as u8;
        properties.to_state_id(&Block::TURTLE_EGG)
    }
}

/// Vanilla `Turtle.TurtleGoHomeGoal`: after laying or wandering off, return to the home beach.
pub struct TurtleGoHomeGoal {
    speed: f64,
    stuck: bool,
    close_to_home_try_ticks: i32,
}

impl TurtleGoHomeGoal {
    #[must_use]
    pub fn new(speed: f64) -> Box<Self> {
        Box::new(Self {
            speed,
            stuck: false,
            close_to_home_try_ticks: 0,
        })
    }
}

impl Goal for TurtleGoHomeGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(turtle) = as_turtle(mob) else {
            return false;
        };
        if turtle.is_baby() {
            return false;
        }
        if turtle.has_egg() {
            return true;
        }
        // Vanilla `reducedTickDelay(700)`.
        let interval = to_goal_ticks(700);
        if mob.get_random().random_range(0..interval) != 0 {
            return false;
        }
        !closer_to_center_than(turtle.home_pos(), &mob.get_entity().pos.load(), 64.0)
    }

    fn start(&mut self, mob: &dyn Mob) {
        if let Some(turtle) = as_turtle(mob) {
            turtle.set_going_home(true);
        }
        self.stuck = false;
        self.close_to_home_try_ticks = 0;
    }

    fn stop(&mut self, mob: &dyn Mob) {
        if let Some(turtle) = as_turtle(mob) {
            turtle.set_going_home(false);
        }
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        let Some(turtle) = as_turtle(mob) else {
            return false;
        };
        !closer_to_center_than(turtle.home_pos(), &mob.get_entity().pos.load(), 7.0)
            && !self.stuck
            && self.close_to_home_try_ticks <= self.get_tick_count(GO_HOME_GIVE_UP_TICKS)
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let Some(turtle) = as_turtle(mob) else {
            return;
        };
        let home = turtle.home_pos();
        let pos = mob.get_entity().pos.load();
        let close_to_home = closer_to_center_than(home, &pos, 16.0);
        if close_to_home {
            self.close_to_home_try_ticks += 1;
        }

        if !mob.is_navigator_idle() {
            return;
        }

        let home_vec = home.to_f64();
        let mut next_pos =
            default_random_pos::get_pos_towards(mob, 16, 3, home_vec, std::f64::consts::PI / 10.0);
        if next_pos.is_none() {
            next_pos = default_random_pos::get_pos_towards(
                mob,
                8,
                7,
                home_vec,
                std::f64::consts::FRAC_PI_2,
            );
        }

        if let Some(candidate) = next_pos {
            let world = mob.get_entity().world.load();
            let block =
                world.get_block(&BlockPos::containing(candidate.x, candidate.y, candidate.z));
            if !close_to_home && block.id != Block::WATER.id {
                next_pos = default_random_pos::get_pos_towards(
                    mob,
                    16,
                    5,
                    home_vec,
                    std::f64::consts::FRAC_PI_2,
                );
            }
        }

        let Some(next_pos) = next_pos else {
            self.stuck = true;
            return;
        };

        mob.get_mob_entity()
            .navigator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .set_progress(NavigatorGoal::new(pos, next_pos, self.speed));
    }
}

/// Vanilla `Turtle.TurtleTravelGoal`: wander through water when not carrying an egg.
pub struct TurtleTravelGoal {
    speed: f64,
    stuck: bool,
}

impl TurtleTravelGoal {
    #[must_use]
    pub fn new(speed: f64) -> Box<Self> {
        Box::new(Self {
            speed,
            stuck: false,
        })
    }
}

impl Goal for TurtleTravelGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(turtle) = as_turtle(mob) else {
            return false;
        };
        !turtle.is_going_home() && !turtle.has_egg() && mob.get_entity().is_in_water()
    }

    fn start(&mut self, mob: &dyn Mob) {
        let Some(turtle) = as_turtle(mob) else {
            return;
        };
        let mut random = mob.get_random();
        let x_offset = random.random_range(0..2 * TRAVEL_XZ_DIST + 1) - TRAVEL_XZ_DIST;
        let mut y_offset = random.random_range(0..2 * TRAVEL_Y_DIST + 1) - TRAVEL_Y_DIST;
        let z_offset = random.random_range(0..2 * TRAVEL_XZ_DIST + 1) - TRAVEL_XZ_DIST;

        let pos = mob.get_entity().pos.load();
        if f64::from(y_offset) + pos.y > f64::from(mob.get_entity().world.load().sea_level - 1) {
            y_offset = 0;
        }

        turtle.set_travel_pos(Some(BlockPos::containing(
            pos.x + f64::from(x_offset),
            pos.y + f64::from(y_offset),
            pos.z + f64::from(z_offset),
        )));
        self.stuck = false;
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        let Some(turtle) = as_turtle(mob) else {
            return false;
        };
        !mob.is_navigator_idle()
            && !self.stuck
            && !turtle.is_going_home()
            && !mob.is_in_love()
            && !turtle.has_egg()
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let Some(turtle) = as_turtle(mob) else {
            return;
        };
        let Some(travel_pos) = turtle.travel_pos() else {
            self.stuck = true;
            return;
        };

        if !mob.is_navigator_idle() {
            return;
        }

        let target = travel_pos.to_f64();
        let mut next_pos =
            default_random_pos::get_pos_towards(mob, 16, 3, target, std::f64::consts::PI / 10.0);
        if next_pos.is_none() {
            next_pos =
                default_random_pos::get_pos_towards(mob, 8, 7, target, std::f64::consts::FRAC_PI_2);
        }

        if let Some(candidate) = next_pos {
            let x = candidate.x.floor() as i32;
            let z = candidate.z.floor() as i32;
            let world = mob.get_entity().world.load();
            if !has_chunks_at(
                &world,
                x - TRAVEL_CHUNK_RADIUS,
                z - TRAVEL_CHUNK_RADIUS,
                x + TRAVEL_CHUNK_RADIUS,
                z + TRAVEL_CHUNK_RADIUS,
            ) {
                next_pos = None;
            }
        }

        let Some(next_pos) = next_pos else {
            self.stuck = true;
            return;
        };

        mob.get_mob_entity()
            .navigator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .set_progress(NavigatorGoal::new(
                mob.get_entity().pos.load(),
                next_pos,
                self.speed,
            ));
    }

    fn stop(&mut self, mob: &dyn Mob) {
        if let Some(turtle) = as_turtle(mob) {
            turtle.set_travel_pos(None);
        }
    }
}

/// Vanilla `Turtle.TurtleGoToWaterGoal` give-up tick count.
const GO_TO_WATER_GIVE_UP_TICKS: i32 = 1200;
/// Vanilla `Turtle.TurtleGoToWaterGoal.shouldRecalculatePath` interval.
const GO_TO_WATER_RECALC_PATH_TICKS: i32 = 160;

/// Vanilla `Turtle.TurtleGoToWaterGoal`: walk toward water when not on the way home.
pub struct TurtleGoToWaterGoal {
    move_to_target_pos_goal: MoveToTargetPosGoal<Self>,
}

impl TurtleGoToWaterGoal {
    #[must_use]
    pub fn new(speed: f64, is_baby: bool) -> Box<Self> {
        let mut this = Box::new(Self {
            move_to_target_pos_goal: MoveToTargetPosGoal::new(
                ParentHandle::none(),
                if is_baby { 2.0 } else { speed },
                24,
                1,
            ),
        });
        // SAFETY: `this` heap allocation address is pinned in Box and outlives `ParentHandle` references.
        this.move_to_target_pos_goal.move_to_target_pos = unsafe { ParentHandle::new(&this) };
        // Vanilla `TurtleGoToWaterGoal` sets `verticalSearchStart = -1`.
        this.move_to_target_pos_goal.lowest_y = -1;
        this
    }
}

impl MoveToTargetPos for TurtleGoToWaterGoal {
    fn is_target_pos(&self, world: Arc<World>, block_pos: BlockPos) -> bool {
        world.get_block(&block_pos).id == Block::WATER.id
    }

    fn should_recalculate_path(&self, try_ticks: i32) -> bool {
        try_ticks % GO_TO_WATER_RECALC_PATH_TICKS == 0
    }
}

impl Goal for TurtleGoToWaterGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(turtle) = as_turtle(mob) else {
            return false;
        };
        if mob.get_entity().is_in_water() {
            return false;
        }
        // Vanilla lets babies always seek water, adults only when not going home or carrying an egg.
        if !turtle.is_baby() && (turtle.is_going_home() || turtle.has_egg()) {
            return false;
        }
        self.move_to_target_pos_goal.can_start(mob)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        !mob.get_entity().is_in_water()
            && self.move_to_target_pos_goal.trying_time <= GO_TO_WATER_GIVE_UP_TICKS
            && self.is_target_pos(
                mob.get_entity().world.load_full(),
                self.move_to_target_pos_goal.target_pos,
            )
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

/// Vanilla `Turtle.TurtlePanicGoal`: like `PanicGoal`, but always searches water within 7 blocks.
pub struct TurtlePanicGoal {
    inner: Box<EscapeDangerGoal>,
}

impl TurtlePanicGoal {
    /// Vanilla `Turtle.TurtlePanicGoal.canUse` water search distance.
    const WATER_SEARCH_XZ: i32 = 7;

    #[must_use]
    pub fn new(speed: f64) -> Box<Self> {
        Box::new(Self {
            inner: EscapeDangerGoal::new(speed),
        })
    }
}

impl Goal for TurtlePanicGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        if !self.inner.should_panic(mob) {
            return false;
        }
        if let Some(water) = EscapeDangerGoal::look_for_water(mob, Self::WATER_SEARCH_XZ) {
            self.inner.set_target(water.to_f64());
            return true;
        }
        if let Some(pos) = EscapeDangerGoal::find_random_position(mob) {
            self.inner.set_target(pos);
            true
        } else {
            false
        }
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        self.inner.should_continue(mob)
    }

    fn start(&mut self, mob: &dyn Mob) {
        self.inner.start(mob);
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.inner.stop(mob);
    }

    fn controls(&self) -> Controls {
        self.inner.controls()
    }
}

/// Vanilla `Turtle.TurtleRandomStrollGoal`: only strolls on land, not while going home or carrying an egg.
pub struct TurtleRandomStrollGoal {
    inner: WanderAroundGoal,
}

impl TurtleRandomStrollGoal {
    #[must_use]
    pub fn new(speed: f64, interval: i32) -> Box<Self> {
        Box::new(Self {
            inner: WanderAroundGoal::with_interval(speed, interval),
        })
    }
}

impl Goal for TurtleRandomStrollGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(turtle) = as_turtle(mob) else {
            return false;
        };
        !mob.get_entity().is_in_water()
            && !turtle.is_going_home()
            && !turtle.has_egg()
            && self.inner.can_start(mob)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        self.inner.should_continue(mob)
    }

    fn start(&mut self, mob: &dyn Mob) {
        self.inner.start(mob);
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.inner.stop(mob);
    }

    fn controls(&self) -> Controls {
        self.inner.controls()
    }
}

/// Vanilla `Turtle.TurtleMoveControl`: adjusts the swim/land speed and adds water buoyancy.
#[derive(Default)]
pub struct TurtleMoveControl {
    inner: MoveControl,
}

impl Control for TurtleMoveControl {}

impl TurtleMoveControl {
    /// Vanilla `TurtleMoveControl.updateSpeed`.
    fn update_speed(mob: &dyn Mob, turtle: &TurtleEntity) {
        let mob_entity = mob.get_mob_entity();
        let entity = &mob_entity.living_entity.entity;
        let speed = || mob_entity.living_entity.get_speed();
        if entity.is_in_water() {
            let velocity = entity.velocity.load();
            entity
                .velocity
                .store(Vector3::new(velocity.x, velocity.y + 0.005, velocity.z));
            if !closer_to_center_than(turtle.home_pos(), &entity.pos.load(), 16.0) {
                mob_entity.set_speed((speed() / 2.0).max(0.08));
            }
            if turtle.is_baby() {
                mob_entity.set_speed((speed() / 3.0).max(0.06));
            }
        } else if entity.on_ground.load(Ordering::Relaxed) {
            mob_entity.set_speed((speed() / 2.0).max(0.06));
        }
    }
}

impl MoveControlTrait for TurtleMoveControl {
    fn tick(&mut self, mob: &dyn Mob) {
        let Some(turtle) = as_turtle(mob) else {
            return;
        };
        let mob_entity = mob.get_mob_entity();
        let living_entity = &mob_entity.living_entity;
        let entity = &living_entity.entity;

        Self::update_speed(mob, turtle);
        if self.inner.operation != Operation::MoveTo || mob_entity.steered().navigator().is_done() {
            mob_entity.set_speed(0.0);
            return;
        }

        let pos = entity.pos.load();
        let xd = self.inner.wanted_x - pos.x;
        let yd = self.inner.wanted_y - pos.y;
        let zd = self.inner.wanted_z - pos.z;
        let dd = (xd * xd + yd * yd + zd * zd).sqrt();
        if dd < f64::from(1.0e-5f32) {
            mob_entity.set_speed(0.0);
            return;
        }

        let yd = yd / dd;
        let y_rot_d = (zd.atan2(xd).to_degrees() as f32) - 90.0;
        let yaw = MoveControl::rotlerp(entity.yaw.load(), y_rot_d, MoveControl::MAX_TURN);
        entity.yaw.store(yaw);
        entity.body_yaw.store(yaw);

        let target_speed = (self.inner.speed_modifier
            * living_entity.get_attribute_value(&Attributes::MOVEMENT_SPEED))
            as f32;
        let speed = living_entity.get_speed();
        mob_entity.set_speed(speed + 0.125 * (target_speed - speed));

        let velocity = entity.velocity.load();
        entity.velocity.store(Vector3::new(
            velocity.x,
            velocity.y + f64::from(living_entity.get_speed()) * yd * 0.1,
            velocity.z,
        ));
    }

    fn set_wanted_position(&mut self, x: f64, y: f64, z: f64, speed_modifier: f64) {
        self.inner.set_wanted_position(x, y, z, speed_modifier);
    }

    fn strafe(&mut self, forward: f32, right: f32) {
        self.inner.strafe(forward, right);
    }

    fn has_wanted(&self) -> bool {
        self.inner.has_wanted()
    }
}
