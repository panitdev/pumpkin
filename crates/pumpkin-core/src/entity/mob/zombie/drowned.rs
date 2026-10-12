use std::sync::{Arc, Weak};

use crate::entity::Entity;
use crate::entity::EntityBase;
use crate::entity::ai::control::{Control, MoveControlTrait};
use crate::entity::ai::goal::{
    Controls, Goal, ParentHandle,
    active_target::ActiveTargetGoal,
    break_door::BreakDoorGoal,
    destroy_egg::DestroyEggGoal,
    look_around::RandomLookAroundGoal,
    look_at_entity::LookAtEntityGoal,
    move_to_target_pos::{MoveToTargetPos, MoveToTargetPosGoal},
    ranged_attack::RangedAttackGoal,
    revenge::RevengeGoal,
    swim::SwimGoal,
    wander_around::WanderAroundGoal,
    zombie_attack::ZombieAttackGoal,
};
use crate::entity::ai::pathfinder::{Navigator, NavigatorGoal};
use crate::entity::ai::util::default_random_pos;
use crate::entity::living::LivingEntity;
use crate::entity::mob::equipment::RegionalDifficulty;
use crate::entity::mob::zombie::{ZombieEntityBase, ZombieKind};
use crate::entity::mob::{Mob, RangedAttackMob};
use crate::entity::projectile::arrow::ArrowPickup;
use crate::entity::projectile::trident::TridentEntity;
use crate::world::World;
use pumpkin_data::data_component_impl::EquipmentSlot;
use pumpkin_data::entity::EntityType;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag;
use pumpkin_data::{Block, BlockDirection};
use pumpkin_util::Hand;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use rand::RngExt;

/// Rolls above this give a drowned a trident or fishing rod.
const DROWNED_HELD_ITEM_CHANCE: f32 = 0.9;

pub struct DrownedEntity {
    entity: Arc<ZombieEntityBase>,
}

impl DrownedEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        Self::from_base(ZombieEntityBase::new(entity, ZombieKind::Drowned))
    }

    #[must_use]
    pub fn with_can_break_doors(entity: Entity, can_break_doors: bool) -> Arc<Self> {
        Self::from_base(ZombieEntityBase::with_can_break_doors(
            entity,
            ZombieKind::Drowned,
            can_break_doors,
        ))
    }

    fn from_base(entity: Arc<ZombieEntityBase>) -> Arc<Self> {
        let mob_arc = Arc::new(Self { entity });
        register_drowned_goals(&mob_arc);
        mob_arc
    }
}

/// Vanilla `Drowned.addBehaviourGoals`, `Drowned.createNavigation` and the constructor's
/// `DrownedMoveControl`. Runs once the `DrownedEntity` arc exists because the trident goal
/// holds a weak `RangedAttackMob`.
fn register_drowned_goals(mob: &Arc<DrownedEntity>) {
    let mob_weak: Weak<dyn Mob> = {
        let arc: Arc<dyn Mob> = mob.clone();
        Arc::downgrade(&arc)
    };
    let ranged_weak: Weak<dyn RangedAttackMob> = {
        let arc: Arc<dyn RangedAttackMob> = mob.clone();
        Arc::downgrade(&arc)
    };

    let mob_entity = &mob.entity.mob_entity;
    // Vanilla `Drowned.createNavigation` and constructor `moveControl = new DrownedMoveControl`.
    *mob_entity
        .navigator
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Navigator::amphibious(false);
    *mob_entity
        .move_control
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Box::new(DrownedMoveControl);

    let sea_level = mob_entity.living_entity.entity.world.load().sea_level;

    let mut goal_selector = mob_entity
        .goals_selector
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut target_selector = mob_entity
        .target_selector
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    goal_selector.add_goal(0, Box::new(SwimGoal::default()));
    goal_selector.add_goal(1, Box::new(DrownedGoToWaterGoal::new(1.0)));
    if mob.entity.can_break_doors() {
        goal_selector.add_goal(1, Box::new(BreakDoorGoal::default()));
    }
    goal_selector.add_goal(
        2,
        Box::new(DrownedTridentAttackGoal::new(ranged_weak, 1.0, 40, 10.0)),
    );
    goal_selector.add_goal(2, Box::new(DrownedAttackGoal::new(1.0, false)));
    goal_selector.add_goal(4, DestroyEggGoal::new(1.0, 3));
    goal_selector.add_goal(5, DrownedGoToBeachGoal::new(1.0));
    goal_selector.add_goal(6, Box::new(DrownedSwimUpGoal::new(1.0, sea_level)));
    goal_selector.add_goal(7, Box::new(WanderAroundGoal::new(1.0)));
    goal_selector.add_goal(
        8,
        LookAtEntityGoal::with_default(mob_weak, &EntityType::PLAYER, 8.0),
    );
    goal_selector.add_goal(8, Box::new(RandomLookAroundGoal::default()));

    target_selector.add_goal(
        1,
        Box::new(
            RevengeGoal::new(true)
                .ignoring(|entity_type| entity_type == &EntityType::DROWNED)
                .alerting_others_except(|entity_type| entity_type == &EntityType::ZOMBIFIED_PIGLIN),
        ),
    );
    target_selector.add_goal(
        2,
        Box::new(ActiveTargetGoal::new(
            &mob.entity.mob_entity,
            &EntityType::PLAYER,
            10,
            true,
            false,
            Some(|target: &LivingEntity, world: &World| ok_target(world, Some(target))),
        )),
    );
    // AbstractVillager, which needs no line of sight.
    target_selector.add_goal(
        3,
        ActiveTargetGoal::predicated(&mob.entity.mob_entity, 10, false, |target, _world| {
            target.entity.entity_type == &EntityType::VILLAGER
                || target.entity.entity_type == &EntityType::WANDERING_TRADER
        }),
    );
    target_selector.add_goal(
        3,
        ActiveTargetGoal::with_default(&mob.entity.mob_entity, &EntityType::IRON_GOLEM, true),
    );
    target_selector.add_goal(
        3,
        ActiveTargetGoal::with_default(&mob.entity.mob_entity, &EntityType::AXOLOTL, true),
    );
    target_selector.add_goal(
        5,
        ActiveTargetGoal::baby_turtle_on_land(&mob.entity.mob_entity),
    );
}

/// Vanilla `Drowned.okTarget`.
fn ok_target(world: &World, target: Option<&dyn EntityBase>) -> bool {
    target.is_some_and(|target| !world.is_bright_outside() || target.get_entity().is_in_water())
}

/// Vanilla `Drowned.closeToNextPos`.
fn close_to_next_pos(mob: &dyn Mob) -> bool {
    let navigator = mob
        .get_mob_entity()
        .navigator
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(path) = navigator.get_path() else {
        return false;
    };
    let target = path.get_target();
    let pos = mob.get_entity().pos.load();
    let dx = pos.x - f64::from(target.0.x);
    let dy = pos.y - f64::from(target.0.y);
    let dz = pos.z - f64::from(target.0.z);
    dx * dx + dy * dy + dz * dz < 4.0
}

fn set_searching_for_land(mob: &dyn Mob, searching: bool) {
    if let Some(zombie) = mob.as_zombie() {
        zombie.set_searching_for_land(searching);
    }
}

/// Vanilla `Drowned.DrownedGoToWaterGoal`. Moves onto nearby water during the day.
pub struct DrownedGoToWaterGoal {
    speed: f64,
    wanted: Option<Vector3<f64>>,
}

impl DrownedGoToWaterGoal {
    #[must_use]
    const fn new(speed: f64) -> Self {
        Self {
            speed,
            wanted: None,
        }
    }

    /// Vanilla `DrownedGoToWaterGoal.getWaterPos`.
    fn find_water_pos(mob: &dyn Mob) -> Option<Vector3<f64>> {
        let entity = mob.get_entity();
        let world = entity.world.load();
        let pos = entity.block_pos.load();
        let mut random = mob.get_random();

        for _ in 0..10 {
            let candidate = BlockPos::new(
                pos.0.x + random.random_range(-10..10),
                pos.0.y + 2 - random.random_range(0..8),
                pos.0.z + random.random_range(-10..10),
            );
            if world.get_block(&candidate).id == Block::WATER.id {
                return Some(Vector3::new(
                    f64::from(candidate.0.x) + 0.5,
                    f64::from(candidate.0.y),
                    f64::from(candidate.0.z) + 0.5,
                ));
            }
        }

        None
    }
}

impl Goal for DrownedGoToWaterGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let entity = mob.get_entity();
        if !entity.world.load().is_bright_outside() || entity.is_in_water() {
            return false;
        }
        match Self::find_water_pos(mob) {
            Some(pos) => {
                self.wanted = Some(pos);
                true
            }
            None => false,
        }
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        !mob.is_navigator_idle()
    }

    fn start(&mut self, mob: &dyn Mob) {
        if let Some(wanted) = self.wanted {
            mob.get_mob_entity()
                .navigator
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .set_progress(NavigatorGoal::new(
                    mob.get_entity().pos.load(),
                    wanted,
                    self.speed,
                ));
        }
    }

    fn controls(&self) -> Controls {
        Controls::MOVE
    }
}

/// Vanilla `Drowned.DrownedGoToBeachGoal`.
pub struct DrownedGoToBeachGoal {
    move_to_target_pos_goal: MoveToTargetPosGoal<Self>,
}

impl DrownedGoToBeachGoal {
    #[must_use]
    pub fn new(speed: f64) -> Box<Self> {
        let mut this = Box::new(Self {
            move_to_target_pos_goal: MoveToTargetPosGoal::new(ParentHandle::none(), speed, 8, 2),
        });
        // SAFETY: `this` heap allocation address is pinned in Box and outlives `ParentHandle` references.
        this.move_to_target_pos_goal.move_to_target_pos = unsafe { ParentHandle::new(&this) };
        this
    }
}

impl MoveToTargetPos for DrownedGoToBeachGoal {
    /// Vanilla `DrownedGoToBeachGoal.isValidTarget`: `entityCanStandOn` is
    /// `Block.isFaceFull(collisionShape, UP)`. Pumpkin has no collision-shape face test, so
    /// `is_side_solid(UP)` (`isFaceSturdy`, the closest full-top-face predicate) stands in.
    fn is_target_pos(&self, world: Arc<World>, block_pos: BlockPos) -> bool {
        let above = block_pos.up();
        if !world.get_block_state(&above).is_air() || !world.get_block_state(&above.up()).is_air() {
            return false;
        }
        world
            .get_block_state(&block_pos)
            .is_side_solid(BlockDirection::Up)
    }
}

impl Goal for DrownedGoToBeachGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        // Vanilla runs `super.canUse()` first, so the search cooldown advances either way.
        self.move_to_target_pos_goal.can_start(mob) && {
            let entity = mob.get_entity();
            let world = entity.world.load();
            !world.is_bright_outside()
                && entity.is_in_water()
                && entity.pos.load().y >= f64::from(world.sea_level - 3)
        }
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        self.move_to_target_pos_goal.should_continue(mob)
    }

    fn start(&mut self, mob: &dyn Mob) {
        set_searching_for_land(mob, false);
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

/// Vanilla `Drowned.DrownedSwimUpGoal`.
pub struct DrownedSwimUpGoal {
    speed: f64,
    sea_level: i32,
    stuck: bool,
}

impl DrownedSwimUpGoal {
    #[must_use]
    const fn new(speed: f64, sea_level: i32) -> Self {
        Self {
            speed,
            sea_level,
            stuck: false,
        }
    }
}

impl Goal for DrownedSwimUpGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let entity = mob.get_entity();
        !entity.world.load().is_bright_outside()
            && entity.is_in_water()
            && entity.pos.load().y < f64::from(self.sea_level - 2)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        self.can_start(mob) && !self.stuck
    }

    fn start(&mut self, mob: &dyn Mob) {
        set_searching_for_land(mob, true);
        self.stuck = false;
    }

    fn stop(&mut self, mob: &dyn Mob) {
        set_searching_for_land(mob, false);
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let pos = mob.get_entity().pos.load();
        if pos.y < f64::from(self.sea_level - 1)
            && (mob.is_navigator_idle() || close_to_next_pos(mob))
        {
            let towards = Vector3::new(pos.x, f64::from(self.sea_level - 1), pos.z);
            let Some(target) = default_random_pos::get_pos_towards(
                mob,
                4,
                8,
                towards,
                std::f64::consts::FRAC_PI_2,
            ) else {
                self.stuck = true;
                return;
            };
            mob.get_mob_entity()
                .navigator
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .set_progress(NavigatorGoal::new(pos, target, self.speed));
        }
    }
}

/// Vanilla `Drowned.DrownedMoveControl`. The amphibious navigator already drives the swimming
/// movement in Pumpkin, so this only adds the vanilla buoyancy.
// TODO: `Drowned.DrownedMoveControl` speed lerp and direct velocity input once Pumpkin's
// amphibious navigator stops setting `movement_input` itself.
pub struct DrownedMoveControl;

impl Control for DrownedMoveControl {}

impl MoveControlTrait for DrownedMoveControl {
    fn tick(&mut self, mob: &dyn Mob) {
        let entity = mob.get_entity();
        let Some(zombie) = mob.as_zombie() else {
            return;
        };
        if zombie.wants_to_swim() && entity.is_in_water() {
            let target_above = mob
                .get_mob_entity()
                .get_target()
                .is_some_and(|target| target.get_entity().pos.load().y > entity.pos.load().y);
            if target_above || zombie.is_searching_for_land() {
                let mut velocity = entity.velocity.load();
                velocity.y += 0.002;
                entity.velocity.store(velocity);
            }
        }
    }
}

/// Vanilla `Drowned.DrownedAttackGoal`: a `ZombieAttackGoal` that also requires `okTarget`.
pub struct DrownedAttackGoal {
    inner: ZombieAttackGoal,
}

impl DrownedAttackGoal {
    #[must_use]
    fn new(speed: f64, pause_when_mob_idle: bool) -> Self {
        Self {
            inner: *ZombieAttackGoal::new(speed, pause_when_mob_idle),
        }
    }
}

impl Goal for DrownedAttackGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        self.inner.can_start(mob)
            && ok_target(
                &mob.get_entity().world.load(),
                mob.get_mob_entity().get_target().as_deref(),
            )
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        self.inner.should_continue(mob)
            && ok_target(
                &mob.get_entity().world.load(),
                mob.get_mob_entity().get_target().as_deref(),
            )
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

    fn should_run_every_tick(&self) -> bool {
        self.inner.should_run_every_tick()
    }

    fn controls(&self) -> Controls {
        self.inner.controls()
    }
}

/// Vanilla `Drowned.DrownedTridentAttackGoal`: a `RangedAttackGoal` that needs a trident in
/// the main hand and raises the arm while aiming.
pub struct DrownedTridentAttackGoal {
    inner: RangedAttackGoal,
}

impl DrownedTridentAttackGoal {
    #[must_use]
    fn new(mob: Weak<dyn RangedAttackMob>, speed: f64, interval: i32, radius: f32) -> Self {
        Self {
            inner: RangedAttackGoal::new(mob, speed, interval, radius),
        }
    }

    fn main_hand_trident(mob: &dyn Mob) -> Option<ItemStack> {
        let stack = mob
            .get_mob_entity()
            .living_entity
            .entity_equipment
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&EquipmentSlot::MAIN_HAND);
        (stack.item.id == Item::TRIDENT.id).then_some(stack)
    }
}

impl Goal for DrownedTridentAttackGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        self.inner.can_start(mob) && Self::main_hand_trident(mob).is_some()
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        self.inner.should_continue(mob)
    }

    fn start(&mut self, mob: &dyn Mob) {
        self.inner.start(mob);
        mob.get_mob_entity().set_attacking(true);
        if let Some(stack) = Self::main_hand_trident(mob) {
            mob.get_mob_entity()
                .living_entity
                .set_active_hand(Hand::Right, stack, i32::MAX);
        }
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.inner.stop(mob);
        mob.get_mob_entity().living_entity.clear_active_hand();
        mob.get_mob_entity().set_attacking(false);
    }

    fn tick(&mut self, mob: &dyn Mob) {
        self.inner.tick(mob);
    }

    fn should_run_every_tick(&self) -> bool {
        self.inner.should_run_every_tick()
    }

    fn controls(&self) -> Controls {
        self.inner.controls()
    }
}

impl RangedAttackMob for DrownedEntity {
    /// Vanilla `Drowned.performRangedAttack`: throws a trident at the target.
    fn perform_ranged_attack(&self, target: &Arc<dyn EntityBase>, _power: f32) {
        let entity = &self.entity.mob_entity.living_entity.entity;
        let world = entity.world.load_full();

        let main_hand = self
            .entity
            .mob_entity
            .living_entity
            .entity_equipment
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&EquipmentSlot::MAIN_HAND);
        let trident_stack = if main_hand.item.id == Item::TRIDENT.id {
            main_hand
        } else {
            ItemStack::new(1, &Item::TRIDENT)
        };

        let projectile_entity = Entity::new(world.clone(), entity.pos.load(), &EntityType::TRIDENT);
        let trident = TridentEntity::new_shot(
            projectile_entity,
            entity,
            trident_stack.clone(),
            ArrowPickup::Disallowed,
        );

        let mob_pos = entity.pos.load();
        let target_entity = target.get_entity();
        let target_pos = target_entity.pos.load();
        let dx = target_pos.x - mob_pos.x;
        let dy = (target_pos.y + f64::from(target_entity.entity_dimension.load().height) / 3.0)
            - trident.entity.pos.load().y;
        let dz = target_pos.z - mob_pos.z;
        let horizontal_distance = dx.hypot(dz);

        // RangedAttackMob.rangedAttackUncertainty
        let difficulty = world.level_info.load().difficulty as i32;
        let uncertainty = f64::from(14 - difficulty * 4);

        trident.set_velocity(
            dx,
            horizontal_distance.mul_add(0.2, dy),
            dz,
            1.6,
            uncertainty,
        );
        trident.apply_on_projectile_spawned(&trident_stack);
        world.spawn_entity(Arc::new(trident));

        world.play_sound_fine(
            Sound::EntityDrownedShoot,
            SoundCategory::Hostile,
            &mob_pos,
            1.0,
            1.0 / (rand::random::<f32>() * 0.4 + 0.8),
        );
    }
}

zombie_mob!(DrownedEntity, entity, {
    /// Vanilla `Drowned.populateDefaultEquipmentSlots`, which does not call super.
    fn populate_default_equipment_slots(
        &self,
        _world: &Arc<World>,
        _difficulty: &RegionalDifficulty,
    ) {
        if rand::random::<f32>() > DROWNED_HELD_ITEM_CHANCE {
            let item = if rand::random_range(0..16) < 10 {
                &Item::TRIDENT
            } else {
                &Item::FISHING_ROD
            };
            self.entity
                .mob_entity
                .living_entity
                .entity_equipment
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .put(&EquipmentSlot::MAIN_HAND, ItemStack::new(1, item));
        }
    }

    /// Vanilla `Drowned.getPreferredWeaponType`.
    fn get_preferred_weapon_type(&self) -> Option<&'static tag::Tag> {
        Some(&tag::Item::MINECRAFT_DROWNED_PREFERRED_WEAPONS)
    }

    /// Vanilla `Drowned.canReplaceCurrentItem`: a nautilus shell is never replaced.
    fn can_replace_current_item(
        &self,
        new_item: &ItemStack,
        current_item: &ItemStack,
        slot: &EquipmentSlot,
    ) -> bool {
        if current_item.item.id == Item::NAUTILUS_SHELL.id {
            return false;
        }
        crate::entity::mob::equipment::can_replace_current_item(
            &self.entity.mob_entity,
            self.get_preferred_weapon_type(),
            new_item,
            current_item,
            slot,
        )
    }

    /// Vanilla `Drowned.isPushedByFluid`.
    fn is_pushed_by_fluids(&self) -> bool {
        !self.entity.mob_entity.living_entity.entity.is_swimming()
    }
});
