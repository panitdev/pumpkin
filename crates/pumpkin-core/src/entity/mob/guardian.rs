use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Weak};

use pumpkin_data::damage::DamageType;
use pumpkin_data::entity::EntityType;
use pumpkin_data::sound::Sound;
use pumpkin_data::tag::{self, Taggable};
use pumpkin_data::tracked_data;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use rand::RngExt;

use crate::entity::ai::control::guardian_move_control::GuardianMoveControl;
use crate::entity::ai::goal::guardian_attack::GuardianAttackGoal;
use crate::entity::ai::goal::move_towards_restriction::MoveTowardsRestrictionGoal;
use crate::entity::ai::goal::{Controls, active_target::ActiveTargetGoal};
use crate::entity::ai::pathfinder::Navigator;
use crate::entity::custom_sound::CustomSound;
use crate::entity::{
    Entity, EntityBase,
    ai::goal::{
        look_around::RandomLookAroundGoal, look_at_entity::LookAtEntityGoal,
        wander_around::WanderAroundGoal,
    },
    mob::{Mob, MobEntity},
};

/// Vanilla `Guardian.ATTACK_TIME`.
pub const ATTACK_TIME: i32 = 80;
const STROLL_INTERVAL: i32 = 80;
const THORNS_DAMAGE: f32 = 2.0;
const WATER_WALK_TARGET_VALUE: f32 = 10.0;
/// Squared distance a target has to be away from the guardian to be picked.
const MIN_TARGET_DISTANCE_SQ: f64 = 9.0;

/// State vanilla keeps in the guardian's synced data and goals.
pub struct GuardianState {
    moving: AtomicBool,
    active_attack_target: AtomicI32,
    stroll_trigger: Arc<AtomicBool>,
}

impl Default for GuardianState {
    fn default() -> Self {
        Self {
            moving: AtomicBool::new(false),
            active_attack_target: AtomicI32::new(0),
            stroll_trigger: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl GuardianState {
    #[must_use]
    pub fn is_moving(&self) -> bool {
        self.moving.load(Ordering::Relaxed)
    }

    pub fn set_moving(&self, entity: &Entity, moving: bool) {
        if self.moving.swap(moving, Ordering::Relaxed) != moving {
            entity.set_synced_data(tracked_data::guardian::DATA_ID_MOVING, moving);
        }
    }

    #[must_use]
    pub fn has_active_attack_target(&self) -> bool {
        self.active_attack_target.load(Ordering::Relaxed) != 0
    }

    pub fn set_active_attack_target(&self, entity: &Entity, entity_id: i32) {
        if self.active_attack_target.swap(entity_id, Ordering::Relaxed) != entity_id {
            entity.set_synced_data(tracked_data::guardian::DATA_ID_ATTACK_TARGET, entity_id);
        }
    }

    /// Vanilla `randomStrollGoal.trigger`.
    pub fn trigger_stroll(&self) {
        self.stroll_trigger.store(true, Ordering::Relaxed);
    }
}

/// What the shared guardian goals and move control need from a guardian or elder guardian.
pub trait GuardianMob: Mob {
    fn guardian_state(&self) -> &GuardianState;

    /// Vanilla `Guardian.getAttackDuration`.
    fn attack_duration(&self) -> i32 {
        ATTACK_TIME
    }

    fn is_elder(&self) -> bool {
        false
    }
}

/// Vanilla `Guardian` constructor, `createNavigation` and `registerGoals`.
pub fn register_guardian(
    mob_entity: &MobEntity,
    mob_weak: &Weak<dyn Mob>,
    state: &GuardianState,
    stroll_interval: i32,
) {
    let mut navigator = mob_entity
        .navigator
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *navigator = Navigator::water_bound(false);
    navigator.set_pathfinding_malus(crate::entity::ai::pathfinder::node::PathType::Water, 0.0);
    drop(navigator);
    *mob_entity
        .move_control
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) =
        Box::new(GuardianMoveControl::default());

    let goal_selector = &mob_entity.goals_selector;
    goal_selector.add_goal(4, Box::new(GuardianAttackGoal::default()));
    goal_selector.add_goal(
        5,
        Box::new(
            MoveTowardsRestrictionGoal::new(1.0).with_controls(Controls::MOVE | Controls::LOOK),
        ),
    );
    goal_selector.add_goal(
        7,
        Box::new(
            WanderAroundGoal::with_interval(1.0, stroll_interval)
                .with_controls(Controls::MOVE | Controls::LOOK)
                .with_trigger_flag(state.stroll_trigger.clone()),
        ),
    );
    goal_selector.add_goal(
        8,
        LookAtEntityGoal::with_default(mob_weak.clone(), &EntityType::PLAYER, 8.0),
    );
    goal_selector.add_goal(
        8,
        Box::new(
            LookAtEntityGoal::new(mob_weak.clone(), &EntityType::GUARDIAN, 12.0, 0.01, false)
                .also_matching(&EntityType::ELDER_GUARDIAN),
        ),
    );
    goal_selector.add_goal(9, Box::new(RandomLookAroundGoal::default()));

    // Vanilla `Guardian.GuardianAttackSelector`.
    let guardian_weak = mob_weak.clone();
    mob_entity.target_selector.add_goal(
        1,
        ActiveTargetGoal::predicated(mob_entity, 10, true, move |target, _world| {
            let target_type = target.entity.entity_type;
            (target_type == &EntityType::PLAYER
                || target_type == &EntityType::SQUID
                || target_type == &EntityType::GLOW_SQUID
                || target_type == &EntityType::AXOLOTL)
                && guardian_weak.upgrade().is_some_and(|guardian| {
                    guardian
                        .get_entity()
                        .pos
                        .load()
                        .squared_distance_to_vec(&target.entity.pos.load())
                        > MIN_TARGET_DISTANCE_SQ
                })
        })
        .including_players(),
    );
}

/// Vanilla `Guardian.getWalkTargetValue`.
#[must_use]
pub fn guardian_walk_target_value(guardian: &dyn Mob, pos: &BlockPos) -> f32 {
    let world = guardian.get_entity().world.load();
    let light_cost = world.get_light_level_dependent_magic_value(pos) - 0.5;
    if world.get_fluid(pos).has_tag(&tag::Fluid::MINECRAFT_WATER) {
        WATER_WALK_TARGET_VALUE + light_cost
    } else {
        // Vanilla `Monster.getWalkTargetValue`.
        -light_cost
    }
}

/// The parts of vanilla `Guardian.aiStep` that run on the server before `super.aiStep`.
pub fn tick_guardian(guardian: &dyn GuardianMob) {
    let entity = guardian.get_entity();
    if !entity.is_alive() {
        return;
    }

    if !entity.touching_water.load(Ordering::Relaxed) && entity.on_ground.load(Ordering::Relaxed) {
        let mut rng = rand::rng();
        let velocity = entity.velocity.load();
        entity.velocity.store(Vector3::new(
            velocity.x + f64::from((rng.random::<f32>() * 2.0 - 1.0) * 0.4),
            velocity.y + 0.5,
            velocity.z + f64::from((rng.random::<f32>() * 2.0 - 1.0) * 0.4),
        ));
        entity.yaw.store(rng.random::<f32>() * 360.0);
        entity.on_ground.store(false, Ordering::Relaxed);
    }

    if guardian.guardian_state().has_active_attack_target() {
        entity.yaw.store(entity.head_yaw.load());
    }
}

/// Vanilla `Guardian.travelInWater`.
pub fn guardian_travel_in_water(
    guardian: &dyn GuardianMob,
    caller: &dyn EntityBase,
    movement_input: Vector3<f64>,
) {
    let entity = guardian.get_entity();
    entity.update_velocity_from_input(movement_input, 0.1);
    entity.move_entity(caller, entity.velocity.load());

    let mut velocity = entity.velocity.load() * 0.9;
    if !guardian.guardian_state().is_moving() && guardian.get_mob_entity().get_target().is_none() {
        velocity.y -= 0.005;
    }
    entity.velocity.store(velocity);
}

/// The thorns half of vanilla `Guardian.hurtServer`, which runs before the damage itself.
pub fn guardian_hurt(
    guardian: &dyn GuardianMob,
    damage_type: DamageType,
    source: Option<&dyn EntityBase>,
) {
    let state = guardian.guardian_state();
    if !state.is_moving()
        && !damage_type.has_tag(&tag::DamageType::MINECRAFT_AVOIDS_GUARDIAN_THORNS)
        && damage_type.id != DamageType::THORNS.id
        && let Some(cause) = source
        && let Some(living) = cause.get_living_entity()
    {
        living.damage_with_context(
            cause,
            THORNS_DAMAGE,
            DamageType::THORNS,
            None,
            Some(guardian),
            Some(guardian),
        );
    }
    state.trigger_stroll();
}

pub struct GuardianEntity {
    pub mob_entity: MobEntity,
    state: GuardianState,
}

impl GuardianEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let guardian = Self {
            mob_entity: MobEntity::new(entity),
            state: GuardianState::default(),
        };
        let mob_arc = Arc::new(guardian);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        register_guardian(
            &mob_arc.mob_entity,
            &mob_weak,
            &mob_arc.state,
            STROLL_INTERVAL,
        );
        mob_arc
    }
}

impl CustomSound for GuardianEntity {
    fn hurt_sound(&self) -> Option<Sound> {
        Some(
            if self.get_entity().touching_water.load(Ordering::Relaxed) {
                Sound::EntityGuardianHurt
            } else {
                Sound::EntityGuardianHurtLand
            },
        )
    }

    fn death_sound(&self) -> Option<Sound> {
        Some(
            if self.get_entity().touching_water.load(Ordering::Relaxed) {
                Sound::EntityGuardianDeath
            } else {
                Sound::EntityGuardianDeathLand
            },
        )
    }
}

impl GuardianMob for GuardianEntity {
    fn guardian_state(&self) -> &GuardianState {
        &self.state
    }
}

impl Mob for GuardianEntity {
    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn as_guardian(&self) -> Option<&dyn GuardianMob> {
        Some(self)
    }

    fn as_custom_sound(&self) -> Option<&dyn CustomSound> {
        Some(self)
    }

    fn get_walk_target_value(&self, pos: &BlockPos) -> f32 {
        guardian_walk_target_value(self, pos)
    }

    fn mob_tick(&self, _caller: &dyn EntityBase) {
        tick_guardian(self);
    }

    fn pre_damage(&self, damage_type: DamageType, source: Option<&dyn EntityBase>) -> bool {
        guardian_hurt(self, damage_type, source);
        true
    }

    fn travel_in_water(&self, caller: &dyn EntityBase, _gravity: f64, _falling: bool) {
        let movement_input = self.get_mob_entity().living_entity.movement_input.load();
        guardian_travel_in_water(self, caller, movement_input);
    }
}
