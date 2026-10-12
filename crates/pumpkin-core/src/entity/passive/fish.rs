//! Shared behaviour of the vanilla fish mobs (`AbstractFish` and `AbstractSchoolingFish`).
//!
//! Cod and Salmon are the only users right now, but Pufferfish and Tropical Fish can register
//! through [`register_fish`] too once they are ported past the ground navigator.

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex, Weak};

use pumpkin_data::entity::EntityType;
use pumpkin_data::sound::Sound;
use pumpkin_util::math::vector3::Vector3;
use rand::RngExt;

use crate::entity::EntityBase;
use crate::entity::ai::control::fish_move_control::FishMoveControl;
use crate::entity::ai::goal::avoid_entity::{AvoidEntityGoal, AvoidSelector};
use crate::entity::ai::goal::escape_danger::EscapeDangerGoal;
use crate::entity::ai::goal::follow_flock_leader::FollowFlockLeaderGoal;
use crate::entity::ai::goal::random_swimming::RandomSwimmingGoal;
use crate::entity::ai::pathfinder::Navigator;
use crate::entity::ai::pathfinder::NavigatorGoal;
use crate::entity::ai::pathfinder::node::PathType;
use crate::entity::mob::{Mob, MobEntity};

/// Vanilla `AbstractFish.registerGoals`: `new PanicGoal(this, 1.25)`.
const PANIC_SPEED: f64 = 1.25;
/// Vanilla `AbstractFish.registerGoals`: `AvoidEntityGoal(this, Player.class, 8.0F, 1.6, 1.4, NO_SPECTATORS)`.
const AVOID_DISTANCE: f64 = 8.0;
const AVOID_SLOW_SPEED: f64 = 1.6;
const AVOID_FAST_SPEED: f64 = 1.4;
/// Vanilla `AbstractFish.FishSwimGoal`: `RandomSwimmingGoal(fish, 1.0, 40)`.
const SWIM_SPEED: f64 = 1.0;
const SWIM_INTERVAL: i32 = 40;
const SCHOOL_RECOUNT_RADIUS: f64 = 8.0;
/// Vanilla `AbstractSchoolingFish`: recount the school on roughly one in 200 ticks.
const SCHOOL_RECOUNT_CHANCE: i32 = 200;
/// Squared leader distance from vanilla `AbstractSchoolingFish.inRangeOfLeader`.
const LEADER_RANGE_SQ: f64 = 121.0;

/// The leader and school size vanilla `AbstractSchoolingFish` keeps per fish.
pub struct FishState {
    leader: Mutex<Option<Weak<dyn EntityBase>>>,
    school_size: AtomicI32,
}

impl Default for FishState {
    fn default() -> Self {
        Self {
            leader: Mutex::new(None),
            school_size: AtomicI32::new(1),
        }
    }
}

impl FishState {
    fn leader(&self) -> Option<Arc<dyn EntityBase>> {
        self.leader
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .and_then(Weak::upgrade)
    }

    fn school_size(&self) -> i32 {
        self.school_size.load(Ordering::Relaxed)
    }

    fn set_school_size(&self, size: i32) {
        self.school_size.store(size, Ordering::Relaxed);
    }

    fn add_follower(&self) {
        self.school_size.fetch_add(1, Ordering::Relaxed);
    }

    fn remove_follower(&self) {
        self.school_size.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Vanilla `AbstractFish`. Implementors add their flop sound; everything else is shared.
pub trait AbstractFish: Mob {
    fn flop_sound(&self) -> Sound;
}

/// Vanilla `AbstractSchoolingFish`: schooling and the `FollowFlockLeaderGoal` bookkeeping.
pub trait AbstractSchoolingFish: AbstractFish {
    fn schooling(&self) -> &FishState;

    /// Vanilla `AbstractFish.getMaxSpawnClusterSize`, the default `getMaxSchoolSize`.
    fn max_school_size(&self) -> i32 {
        8
    }

    fn school_size(&self) -> i32 {
        self.schooling().school_size()
    }

    fn has_followers(&self) -> bool {
        self.school_size() > 1
    }

    fn is_follower(&self) -> bool {
        self.schooling()
            .leader()
            .is_some_and(|leader| leader.get_entity().is_alive())
    }

    fn can_be_followed(&self) -> bool {
        self.has_followers() && self.school_size() < self.max_school_size()
    }

    fn in_range_of_leader(&self) -> bool {
        let Some(leader) = self.schooling().leader() else {
            return false;
        };
        let pos = self.get_entity().pos.load();
        pos.squared_distance_to_vec(&leader.get_entity().pos.load()) <= LEADER_RANGE_SQ
    }

    fn path_to_leader(&self) {
        let Some(leader) = self.schooling().leader() else {
            return;
        };
        let mob_pos = self.get_entity().pos.load();
        let leader_pos = leader.get_entity().pos.load();
        self.get_mob_entity()
            .navigator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .set_progress(NavigatorGoal::new(mob_pos, leader_pos, 1.0));
    }

    fn start_following(&self, leader: Weak<dyn EntityBase>) {
        if let Some(leader) = leader.upgrade()
            && let Some(fish) = leader.get_mob().and_then(Mob::as_schooling_fish)
        {
            fish.schooling().add_follower();
        }
        *self
            .schooling()
            .leader
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(leader);
    }

    fn stop_following(&self) {
        if let Some(leader) = self.schooling().leader()
            && let Some(fish) = leader.get_mob().and_then(Mob::as_schooling_fish)
        {
            fish.schooling().remove_follower();
        }
        *self
            .schooling()
            .leader
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    }
}

/// Vanilla `WaterAnimal` constructor plus `AbstractFish`: water navigation, `FishMoveControl`,
/// `PanicGoal`, `AvoidEntityGoal` and `FishSwimGoal`.
pub fn register_fish(mob_entity: &MobEntity) {
    let dimension = mob_entity.living_entity.entity.entity_dimension.load();
    {
        let mut navigator = mob_entity
            .navigator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *navigator = Navigator::water_bound(false);
        navigator.set_mob_dimensions(dimension.width, dimension.height);
        // Vanilla `WaterAnimal`: `setPathfindingMalus(PathType.WATER, 0.0F)`.
        navigator.set_pathfinding_malus(PathType::Water, 0.0);
    };
    *mob_entity
        .move_control
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Box::new(FishMoveControl::default());

    let mut goals = mob_entity
        .goals_selector
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    goals.add_goal(0, EscapeDangerGoal::new(PANIC_SPEED));
    goals.add_goal(
        2,
        Box::new(
            AvoidEntityGoal::new(
                &EntityType::PLAYER,
                AVOID_DISTANCE,
                AVOID_SLOW_SPEED,
                AVOID_FAST_SPEED,
            )
            .with_selector(AvoidSelector::ExceptSpectator),
        ),
    );
    goals.add_goal(
        4,
        Box::new(RandomSwimmingGoal::new(SWIM_SPEED, SWIM_INTERVAL)),
    );
}

/// Vanilla `AbstractSchoolingFish.registerGoals`: `FollowFlockLeaderGoal` at priority 5.
// TODO: also seed the initial school from spawn group data
// (`AbstractSchoolingFish.SchoolSpawnGroupData`), which Pumpkin's `SpawnGroupData` cannot carry.
pub fn register_schooling_goals(mob_entity: &MobEntity, mob_weak: &Weak<dyn EntityBase>) {
    mob_entity
        .goals_selector
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .add_goal(5, Box::new(FollowFlockLeaderGoal::new(mob_weak.clone())));
}

/// Vanilla `AbstractFish.aiStep`: flop on land, and `AbstractSchoolingFish.tick`.
pub fn fish_tick(fish: &dyn AbstractFish) {
    let entity = fish.get_entity();

    // `on_ground` stands in for vanilla's `verticalCollision`, which Pumpkin does not track.
    // TODO: use Entity.verticalCollision once the move code records it.
    if !entity.touching_water.load(Ordering::Relaxed) && entity.on_ground.load(Ordering::Relaxed) {
        let mut rng = rand::rng();
        let velocity = entity.velocity.load();
        entity.velocity.store(Vector3::new(
            velocity.x + f64::from((rng.random::<f32>() * 2.0 - 1.0) * 0.05),
            velocity.y + 0.4,
            velocity.z + f64::from((rng.random::<f32>() * 2.0 - 1.0) * 0.05),
        ));
        entity.on_ground.store(false, Ordering::Relaxed);
        entity.velocity_dirty.store(true, Ordering::Relaxed);
        entity.play_sound(fish.flop_sound());
    }

    if let Some(school) = fish.as_schooling_fish()
        && school.has_followers()
        && fish.get_random().random_range(0..SCHOOL_RECOUNT_CHANCE) == 1
    {
        let world = entity.world.load();
        let search_box = entity.bounding_box.load().expand(
            SCHOOL_RECOUNT_RADIUS,
            SCHOOL_RECOUNT_RADIUS,
            SCHOOL_RECOUNT_RADIUS,
        );
        let neighbors = world
            .get_all_at_box(&search_box)
            .into_iter()
            .filter(|other| other.get_entity().entity_type == entity.entity_type)
            .count();
        if neighbors <= 1 {
            school.schooling().set_school_size(1);
        }
    }
}

/// Vanilla `AbstractFish.travelInWater`.
pub fn fish_travel_in_water(
    fish: &dyn AbstractFish,
    caller: &dyn EntityBase,
    movement_input: Vector3<f64>,
) {
    let entity = fish.get_entity();
    entity.update_velocity_from_input(movement_input, 0.01);
    entity.move_entity(caller, entity.velocity.load());

    let mut velocity = entity.velocity.load() * 0.9;
    if fish.get_mob_entity().get_target().is_none() {
        velocity.y -= 0.005;
    }
    entity.velocity.store(velocity);
}
