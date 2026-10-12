use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};

use crossbeam::atomic::AtomicCell;
use pumpkin_data::damage::DamageType;
use pumpkin_data::entity::EntityStatus;
use pumpkin_data::particle::Particle;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use rand::RngExt;

use crate::entity::ageable::{AgeableData, AgeableMob};
use crate::entity::ai::goal::{Goal, to_goal_ticks};
use crate::entity::ai::pathfinder::Navigator;
use crate::entity::ai::pathfinder::node::PathType;
use crate::entity::mob::{Mob, MobEntity};
use crate::entity::{Entity, EntityBase};

/// Vanilla `Squid.SquidRandomMovementGoal`: `reducedTickDelay(50)`.
const RANDOM_MOVEMENT_INTERVAL: i32 = 50;
/// Vanilla `Squid.SquidFleeGoal.SQUID_FLEE_SPEED`.
const SQUID_FLEE_SPEED: f64 = 3.0;
/// Vanilla `Squid.SquidFleeGoal.SQUID_FLEE_MIN_DISTANCE`.
const SQUID_FLEE_MIN_DISTANCE: f64 = 5.0;
/// Vanilla `Squid.SquidFleeGoal.SQUID_FLEE_MAX_DISTANCE`.
const SQUID_FLEE_MAX_DISTANCE: f64 = 10.0;
/// Vanilla `Squid.SquidFleeGoal` bubble interval.
const SQUID_FLEE_BUBBLE_INTERVAL: i32 = 10;
/// Vanilla `Squid.SquidFleeGoal` bubble tick within the interval.
const SQUID_FLEE_BUBBLE_TICK: i32 = 5;
/// Vanilla `AgeableWaterCreature.getAmbientSoundInterval`.
const AMBIENT_SOUND_INTERVAL: i32 = 120;
/// Vanilla `Squid.getSoundVolume`.
const SQUID_SOUND_VOLUME: f32 = 0.4;
const TAU: f32 = std::f32::consts::TAU;
const PI: f32 = std::f32::consts::PI;

/// Server side movement state of a squid (the vanilla `Squid` fields the server acts on).
///
/// The render-only fields (`zBodyRot`, `rotateSpeed`, `tentacleAngle`, the old tentacle values)
/// are not tracked; only what the movement, the entity event and the ink direction need is.
pub struct SquidState {
    movement_vector: Mutex<Vector3<f64>>,
    tentacle_movement: AtomicCell<f32>,
    tentacle_speed: AtomicCell<f32>,
    x_body_rot: AtomicCell<f32>,
    x_body_rot_old: AtomicCell<f32>,
    body_yaw_old: AtomicCell<f32>,
    ambient_sound_time: AtomicI32,
}

impl SquidState {
    #[must_use]
    pub fn new() -> Self {
        // Vanilla `Squid` constructor: tentacleSpeed = 1.0F / (random.nextFloat() + 1.0F) * 0.2F.
        let tentacle_speed = 1.0 / (rand::rng().random::<f32>() + 1.0) * 0.2;
        Self {
            movement_vector: Mutex::new(Vector3::new(0.0, 0.0, 0.0)),
            tentacle_movement: AtomicCell::new(0.0),
            tentacle_speed: AtomicCell::new(tentacle_speed),
            x_body_rot: AtomicCell::new(0.0),
            x_body_rot_old: AtomicCell::new(0.0),
            body_yaw_old: AtomicCell::new(0.0),
            ambient_sound_time: AtomicI32::new(0),
        }
    }

    fn movement_vector(&self) -> Vector3<f64> {
        *self
            .movement_vector
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn set_movement_vector(&self, vector: Vector3<f64>) {
        *self
            .movement_vector
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = vector;
    }

    /// Vanilla `Squid.hasMovementVector`.
    fn has_movement_vector(&self) -> bool {
        self.movement_vector().length_squared() > 1.0e-5
    }
}

impl Default for SquidState {
    fn default() -> Self {
        Self::new()
    }
}

/// What the shared squid goals, movement and ink cloud need from a squid or glow squid.
pub trait SquidMob: Mob {
    fn squid_state(&self) -> &SquidState;

    /// Vanilla `Squid.getInkParticle`.
    fn ink_particle(&self) -> Particle {
        Particle::SquidInk
    }

    /// Vanilla `Squid.getSquirtSound`.
    fn squirt_sound(&self) -> Sound {
        Sound::EntitySquidSquirt
    }

    /// Vanilla `Squid.getAmbientSound`.
    fn ambient_sound(&self) -> Sound {
        Sound::EntitySquidAmbient
    }
}

/// Vanilla `Squid` constructor water malus and `registerGoals`.
pub fn register_squid(mob_entity: &MobEntity) {
    let dimension = mob_entity.living_entity.entity.entity_dimension.load();
    let mut navigator = mob_entity
        .navigator
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *navigator = Navigator::water_bound(false);
    navigator.set_mob_dimensions(dimension.width, dimension.height);
    navigator.set_pathfinding_malus(PathType::Water, 0.0);
    drop(navigator);

    // Vanilla `Squid.registerGoals` only has these two goals; it has no path navigation,
    // wander or look goals.
    let mut goal_selector = mob_entity
        .goals_selector
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    goal_selector.add_goal(0, Box::new(SquidRandomMovementGoal));
    goal_selector.add_goal(1, Box::new(SquidFleeGoal::default()));
}

/// Vanilla `Squid.aiStep` movement and body rotation, run after `travel`.
pub fn tick_squid(squid: &dyn SquidMob) {
    let entity = squid.get_entity();
    let state = squid.squid_state();

    state.x_body_rot_old.store(state.x_body_rot.load());
    state.body_yaw_old.store(entity.body_yaw.load());

    let mut tentacle_movement = state.tentacle_movement.load() + state.tentacle_speed.load();
    if tentacle_movement > TAU {
        tentacle_movement -= TAU;
        let mut rng = rand::rng();
        if rng.random_range(0..10) == 0 {
            state
                .tentacle_speed
                .store(1.0 / (rng.random::<f32>() + 1.0) * 0.2);
        }
        entity
            .world
            .load()
            .broadcast_entity_event(entity, EntityStatus::SquidAnimSynch, None);
    }
    state.tentacle_movement.store(tentacle_movement);

    if entity.touching_water.load(Ordering::Relaxed) {
        if tentacle_movement < PI {
            // Vanilla drives the whole swim with the movement vector once the tentacles are
            // far enough through their cycle.
            if tentacle_movement / PI > 0.75 {
                entity.velocity.store(state.movement_vector());
            }
        } else {
            entity.velocity.store(entity.velocity.load() * 0.9);
        }

        let movement = entity.velocity.load();
        let horizontal = movement.horizontal_length();
        let mut y_body_rot = entity.body_yaw.load();
        y_body_rot += (-(movement.x.atan2(movement.z) as f32).to_degrees() - y_body_rot) * 0.1;
        entity.body_yaw.store(y_body_rot);
        entity.yaw.store(y_body_rot);
        let mut x_body_rot = state.x_body_rot.load();
        x_body_rot += (-(horizontal.atan2(movement.y) as f32).to_degrees() - x_body_rot) * 0.1;
        state.x_body_rot.store(x_body_rot);
    } else {
        let mut x_body_rot = state.x_body_rot.load();
        x_body_rot += (-90.0 - x_body_rot) * 0.02;
        state.x_body_rot.store(x_body_rot);
    }
}

/// Vanilla `Squid.travel`: move by the squid's own delta movement, ignoring the input.
pub fn squid_travel_in_water(
    squid: &dyn SquidMob,
    caller: &dyn EntityBase,
    _movement_input: Vector3<f64>,
) {
    let entity = squid.get_entity();
    entity.move_entity(caller, entity.velocity.load());
}

/// Vanilla `Squid.hurtServer`: spawn the ink cloud, but only when a living attacker hurt it.
/// Returns whether the ink was spawned, matching `Squid.hurtServer`'s return value.
pub fn spawn_ink_if_hurt(mob: &dyn Mob) -> bool {
    if mob
        .get_mob_entity()
        .living_entity
        .get_last_hurt_by_mob()
        .is_none()
    {
        return false;
    }
    if let Some(squid) = mob.as_squid() {
        spawn_ink(squid);
        return true;
    }
    false
}

/// Vanilla `LivingEntity.makeSound` at `Squid.getSoundVolume`.
fn play_squid_sound(squid: &dyn SquidMob, sound: Sound) {
    let entity = squid.get_entity();
    let pitch = squid.get_mob_entity().living_entity.get_pitch();
    entity.world.load().play_sound_fine(
        sound,
        SoundCategory::Neutral,
        &entity.pos.load(),
        SQUID_SOUND_VOLUME,
        pitch,
    );
}

/// Vanilla `Squid.spawnInk`: the squirt sound plus a burst of ink particles.
pub fn spawn_ink(squid: &dyn SquidMob) {
    let entity = squid.get_entity();
    let state = squid.squid_state();
    play_squid_sound(squid, squid.squirt_sound());
    let pos = entity.pos.load();
    let origin = rotate_vector(
        Vector3::new(0.0, -1.0, 0.0),
        state.x_body_rot_old.load(),
        state.body_yaw_old.load(),
    )
    .add_raw(pos.x, pos.y, pos.z);

    let ink_offset_scale = if entity.age.load(Ordering::Relaxed) < 0 {
        0.1
    } else {
        0.3
    };
    let world = entity.world.load();
    let mut rng = rand::rng();
    for _ in 0..30 {
        let dir = rotate_vector(
            Vector3::new(
                f64::from(rng.random::<f32>() * 0.6 - 0.3),
                -1.0,
                f64::from(rng.random::<f32>() * 0.6 - 0.3),
            ),
            state.x_body_rot_old.load(),
            state.body_yaw_old.load(),
        );
        let dir_offset = dir * f64::from(ink_offset_scale + rng.random::<f32>() * 2.0);
        world.spawn_particles(
            squid.ink_particle(),
            Vector3::new(origin.x, origin.y + 0.5, origin.z),
            0,
            dir_offset.to_f32_lossy(),
            0.1,
        );
    }
}

/// Vanilla `Mob.baseTick` ambient sound timer, using `AgeableWaterCreature`'s 120 tick interval.
pub fn tick_ambient_sound(squid: &dyn SquidMob) {
    let entity = squid.get_entity();
    if !entity.is_alive() {
        return;
    }
    let state = squid.squid_state();
    let time = state.ambient_sound_time.fetch_add(1, Ordering::Relaxed);
    if rand::rng().random_range(0..1000) < time {
        state
            .ambient_sound_time
            .store(-AMBIENT_SOUND_INTERVAL, Ordering::Relaxed);
        play_squid_sound(squid, squid.ambient_sound());
    }
}

/// Vanilla `Mob.playHurtSound`: reset the ambient sound timer when hurt.
pub fn reset_ambient_sound_time(squid: &dyn SquidMob) {
    squid
        .squid_state()
        .ambient_sound_time
        .store(-AMBIENT_SOUND_INTERVAL, Ordering::Relaxed);
}

/// Vanilla `Squid.rotateVector`: rotate around X, then Y.
fn rotate_vector(vec: Vector3<f64>, x_body_rot: f32, y_body_rot: f32) -> Vector3<f64> {
    let x_angle = x_body_rot * (PI / 180.0);
    let (sin_x, cos_x) = x_angle.sin_cos();
    let rotated = Vector3::new(
        vec.x,
        vec.y * f64::from(cos_x) + vec.z * f64::from(sin_x),
        vec.z * f64::from(cos_x) - vec.y * f64::from(sin_x),
    );

    let y_angle = -y_body_rot * (PI / 180.0);
    let (sin_y, cos_y) = y_angle.sin_cos();
    Vector3::new(
        rotated.x * f64::from(cos_y) + rotated.z * f64::from(sin_y),
        rotated.y,
        rotated.z * f64::from(cos_y) - rotated.x * f64::from(sin_y),
    )
}

/// Vanilla `Squid.SquidRandomMovementGoal`.
pub struct SquidRandomMovementGoal;

impl Goal for SquidRandomMovementGoal {
    fn can_start(&mut self, _mob: &dyn Mob) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn Mob) {
        let Some(squid) = mob.as_squid() else {
            return;
        };
        let entity = mob.get_entity();
        let state = squid.squid_state();
        // TODO: vanilla also zeroes the movement vector once `Mob.getNoActionTime` exceeds
        // 100; Pumpkin does not track the despawn no-action time yet.
        let mut rng = rand::rng();
        if rng.random_range(0..to_goal_ticks(RANDOM_MOVEMENT_INTERVAL)) == 0
            || !entity.touching_water.load(Ordering::Relaxed)
            || !state.has_movement_vector()
        {
            let angle = rng.random::<f32>() * TAU;
            state.set_movement_vector(Vector3::new(
                f64::from(angle.cos() * 0.2),
                f64::from(-0.1 + rng.random::<f32>() * 0.2),
                f64::from(angle.sin() * 0.2),
            ));
        }
    }
}

/// Vanilla `Squid.SquidFleeGoal`.
#[derive(Default)]
pub struct SquidFleeGoal {
    flee_ticks: i32,
}

impl Goal for SquidFleeGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let entity = mob.get_entity();
        entity.touching_water.load(Ordering::Relaxed)
            && mob
                .get_mob_entity()
                .living_entity
                .get_last_hurt_by_mob()
                .is_some_and(|attacker| {
                    entity
                        .pos
                        .load()
                        .squared_distance_to_vec(&attacker.get_entity().pos.load())
                        < SQUID_FLEE_MAX_DISTANCE * SQUID_FLEE_MAX_DISTANCE
                })
    }

    fn start(&mut self, _mob: &dyn Mob) {
        self.flee_ticks = 0;
    }

    fn should_run_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, mob: &dyn Mob) {
        self.flee_ticks += 1;
        let Some(attacker) = mob.get_mob_entity().living_entity.get_last_hurt_by_mob() else {
            return;
        };
        let entity = mob.get_entity();
        let squid_pos = entity.pos.load();
        let mut flee_to = squid_pos.sub(&attacker.get_entity().pos.load());

        let world = entity.world.load();
        let check_pos = BlockPos::floored_v(squid_pos.add(&flee_to));
        let block_state = world.get_block_state(&check_pos);
        let fluid_state = world.get_fluid(&check_pos);
        if fluid_state.has_tag(&tag::Fluid::MINECRAFT_WATER) || block_state.is_air() {
            let length = flee_to.length();
            if length > 0.0 {
                flee_to = flee_to.normalize();
                let mut avoid_speed = SQUID_FLEE_SPEED;
                if length > SQUID_FLEE_MIN_DISTANCE {
                    avoid_speed -= (length - SQUID_FLEE_MIN_DISTANCE) / SQUID_FLEE_MIN_DISTANCE;
                }
                if avoid_speed > 0.0 {
                    flee_to = flee_to * avoid_speed;
                }
            }
            if block_state.is_air() {
                flee_to = flee_to.sub_raw(0.0, flee_to.y, 0.0);
            }
            if let Some(squid) = mob.as_squid() {
                squid.squid_state().set_movement_vector(Vector3::new(
                    flee_to.x / 20.0,
                    flee_to.y / 20.0,
                    flee_to.z / 20.0,
                ));
            }
        }

        if self.flee_ticks % SQUID_FLEE_BUBBLE_INTERVAL == SQUID_FLEE_BUBBLE_TICK {
            world.spawn_particles(
                Particle::Bubble,
                squid_pos,
                0,
                Vector3::new(0.0, 0.0, 0.0),
                0.0,
            );
        }
    }
}

pub struct SquidEntity {
    pub mob_entity: MobEntity,
    pub ageable_data: AgeableData,
    state: SquidState,
}

impl SquidEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let squid = Self {
            mob_entity,
            ageable_data: AgeableData::default(),
            state: SquidState::new(),
        };
        let mob_arc = Arc::new(squid);

        register_squid(&mob_arc.mob_entity);
        mob_arc
    }
}

impl AgeableMob for SquidEntity {
    fn get_ageable_data(&self) -> &AgeableData {
        &self.ageable_data
    }
}

impl SquidMob for SquidEntity {
    fn squid_state(&self) -> &SquidState {
        &self.state
    }
}

impl Mob for SquidEntity {
    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn as_squid(&self) -> Option<&dyn SquidMob> {
        Some(self)
    }

    fn as_ageable(&self) -> Option<&dyn AgeableMob> {
        Some(self)
    }

    fn post_tick(&self) {
        self.ageable_ai_step();
        tick_squid(self);
        tick_ambient_sound(self);
    }

    fn travel_in_water(&self, caller: &dyn EntityBase, movement_input: Vector3<f64>) -> bool {
        squid_travel_in_water(self, caller, movement_input);
        true
    }

    fn is_pushed_by_fluids(&self) -> bool {
        false
    }

    fn on_damage(&self, _damage_type: DamageType, _source: Option<&dyn EntityBase>) {
        reset_ambient_sound_time(self);
        spawn_ink_if_hurt(self);
    }
}
