use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};

use pumpkin_data::damage::DamageType;
use pumpkin_data::effect::StatusEffect;
use pumpkin_data::potion::Effect;
use pumpkin_data::sound::Sound;
use pumpkin_data::tag::{self, Taggable};
use pumpkin_data::tracked_data;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::codec::var_int::VarInt;
use pumpkin_protocol::java::client::play::{CGameEvent, GameEvent};
use pumpkin_util::math::vector3::Vector3;

use crate::entity::ai::goal::Goal;
use crate::entity::ai::target_predicate::TargetPredicate;
use crate::entity::{
    Entity, EntityBase,
    mob::{Mob, MobEntity},
    passive::fish::{self, AbstractFish},
    player::Player,
};

/// Vanilla `Pufferfish.STATE_SMALL`, `STATE_MID` and `STATE_FULL`.
pub const STATE_SMALL: i32 = 0;
pub const STATE_MID: i32 = 1;
pub const STATE_FULL: i32 = 2;
const DEFAULT_PUFF_STATE: i32 = STATE_SMALL;

/// `Pufferfish.tick` sound thresholds, in ticks.
const INFLATE_FULL_TICKS: i32 = 40;
const DEFLATE_TO_MID_TICKS: i32 = 60;
const DEFLATE_TO_SMALL_TICKS: i32 = 100;
/// `Pufferfish.touch`/`playerTouch`: poison lasts 60 ticks per puff state.
const POISON_TICKS_PER_STATE: i32 = 60;
/// `PufferfishPuffGoal.canUse` and `Pufferfish.aiStep` touch boxes.
const PUFF_GOAL_RADIUS: f64 = 2.0;
const TOUCH_RADIUS: f64 = 0.3;

/// Represents a Pufferfish, a passive aquatic mob that can inflate when threatened.
///
/// Wiki: <https://minecraft.wiki/w/Pufferfish>
pub struct PufferfishEntity {
    pub mob_entity: MobEntity,
    puff_state: AtomicI32,
    inflate_counter: AtomicI32,
    deflate_timer: AtomicI32,
}

impl PufferfishEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let pufferfish = Self {
            mob_entity,
            puff_state: AtomicI32::new(DEFAULT_PUFF_STATE),
            inflate_counter: AtomicI32::new(0),
            deflate_timer: AtomicI32::new(0),
        };
        let mob_arc = Arc::new(pufferfish);

        fish::register_fish(&mob_arc.mob_entity);
        mob_arc
            .mob_entity
            .goals_selector
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .add_goal(1, Box::new(PufferfishPuffGoal::new(mob_arc.clone())));
        // Vanilla `Pufferfish` constructor calls `refreshDimensions`.
        mob_arc.apply_dimensions();

        mob_arc
    }

    #[must_use]
    pub fn get_puff_state(&self) -> i32 {
        self.puff_state.load(Ordering::Relaxed)
    }

    pub fn set_puff_state(&self, state: i32) {
        self.puff_state.store(state, Ordering::Relaxed);
        // Vanilla `Pufferfish.onSyncedDataUpdated(PUFF_STATE)` refreshes the dimensions.
        self.get_entity()
            .set_synced_data(tracked_data::pufferfish::PUFF_STATE, VarInt(state));
        self.apply_dimensions();
    }

    /// Vanilla `Pufferfish.getDefaultDimensions`: the type's box scaled by the puff state.
    fn apply_dimensions(&self) {
        fish::apply_scaled_dimensions(self.get_entity(), get_scale(self.get_puff_state()));
    }

    /// Vanilla `Pufferfish.tick`: inflate and deflate over time, sounding the transitions.
    fn tick_puff_state(&self) {
        let entity = self.get_entity();
        if !entity.is_alive() || self.mob_entity.is_no_ai() {
            return;
        }

        let inflate_counter = self.inflate_counter.load(Ordering::Relaxed);
        if inflate_counter > 0 {
            let state = self.get_puff_state();
            if state == STATE_SMALL {
                entity.play_sound(Sound::EntityPufferFishBlowUp);
                self.set_puff_state(STATE_MID);
            } else if inflate_counter > INFLATE_FULL_TICKS && state == STATE_MID {
                entity.play_sound(Sound::EntityPufferFishBlowUp);
                self.set_puff_state(STATE_FULL);
            }
            self.inflate_counter
                .store(inflate_counter + 1, Ordering::Relaxed);
        } else if self.get_puff_state() != STATE_SMALL {
            let deflate_timer = self.deflate_timer.load(Ordering::Relaxed);
            let state = self.get_puff_state();
            if deflate_timer > DEFLATE_TO_MID_TICKS && state == STATE_FULL {
                entity.play_sound(Sound::EntityPufferFishBlowOut);
                self.set_puff_state(STATE_MID);
            } else if deflate_timer > DEFLATE_TO_SMALL_TICKS && state == STATE_MID {
                entity.play_sound(Sound::EntityPufferFishBlowOut);
                self.set_puff_state(STATE_SMALL);
            }
            self.deflate_timer
                .store(deflate_timer + 1, Ordering::Relaxed);
        }
    }

    /// Vanilla `Pufferfish.aiStep`: sting nearby mobs while inflated.
    fn touch_nearby_mobs(&self) {
        if !self.get_entity().is_alive() || self.get_puff_state() <= 0 {
            return;
        }

        let entity = self.get_entity();
        let world = entity.world.load();
        let search_box =
            entity
                .bounding_box
                .load()
                .expand(TOUCH_RADIUS, TOUCH_RADIUS, TOUCH_RADIUS);
        for target in world.get_all_at_box(&search_box) {
            if !target.get_entity().is_alive()
                || target.get_mob().is_none()
                || !is_scary(target.as_ref())
            {
                continue;
            }
            self.touch_mob(target.as_ref());
        }
    }

    /// Vanilla `Pufferfish.touch`.
    fn touch_mob(&self, mob: &dyn EntityBase) {
        if !do_teams_allow_damage(self, mob) {
            return;
        }
        let puff_state = self.get_puff_state();
        let attacker: &dyn EntityBase = self;
        if mob.damage_with_context(
            mob,
            (1 + puff_state) as f32,
            DamageType::MOB_ATTACK,
            None,
            Some(attacker),
            Some(attacker),
        ) {
            if let Some(living) = mob.get_living_entity() {
                living.add_effect(poison_effect(puff_state));
            }
            self.get_entity().play_sound(Sound::EntityPufferFishSting);
        }
    }
}

impl AbstractFish for PufferfishEntity {
    fn flop_sound(&self) -> Sound {
        Sound::EntityPufferFishFlop
    }
}

impl Mob for PufferfishEntity {
    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn mob_tick(&self, _caller: &dyn EntityBase) {
        self.tick_puff_state();
        fish::fish_tick(self);
        self.touch_nearby_mobs();
    }

    fn travel_in_water(&self, caller: &dyn EntityBase, movement_input: Vector3<f64>) -> bool {
        fish::fish_travel_in_water(self, caller, movement_input);
        true
    }

    fn mob_player_collision(&self, player: &Arc<Player>) {
        // Vanilla `Pufferfish.playerTouch`.
        if !do_teams_allow_damage(self, &**player) {
            return;
        }
        let puff_state = self.get_puff_state();
        if puff_state <= 0 {
            return;
        }
        let attacker: &dyn EntityBase = self;
        if player.damage_with_context(
            &**player,
            (1 + puff_state) as f32,
            DamageType::MOB_ATTACK,
            None,
            Some(attacker),
            Some(attacker),
        ) {
            if !self.get_entity().is_silent() {
                player.try_send_client_packet(&CGameEvent::new(
                    GameEvent::PlayPufferfishStringSound,
                    0.0,
                ));
            }
            player.add_effect(poison_effect(puff_state));
        }
    }

    fn mob_init_data_tracker(&self) {
        self.get_entity().set_synced_data(
            tracked_data::pufferfish::PUFF_STATE,
            VarInt(self.get_puff_state()),
        );
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        nbt.put_int("PuffState", self.get_puff_state());
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        if let Some(state) = nbt.get_int("PuffState") {
            self.set_puff_state(state.min(STATE_FULL));
        }
    }
}

/// Vanilla `PufferfishPuffGoal`, priority 1: inflate while a scary living entity is near.
struct PufferfishPuffGoal {
    fish: Arc<PufferfishEntity>,
    target_predicate: TargetPredicate,
}

impl PufferfishPuffGoal {
    fn new(fish: Arc<PufferfishEntity>) -> Self {
        // Vanilla `Pufferfish.TARGETING_CONDITIONS`: non-combat, ignoring invisibility and line
        // of sight. The `SCARY_MOB` selector stays outside `test` because the predicate only sees
        // the inner `LivingEntity`, which would lose the `Player` needed for the creative check.
        let target_predicate = TargetPredicate::create_non_attackable()
            .ignore_visibility()
            .ignore_distance_scaling_factor();
        Self {
            fish,
            target_predicate,
        }
    }
}

impl Goal for PufferfishPuffGoal {
    fn can_start(&mut self, _mob: &dyn Mob) -> bool {
        let entity = self.fish.get_entity();
        let world = entity.world.load();
        let search_box =
            entity
                .bounding_box
                .load()
                .expand(PUFF_GOAL_RADIUS, PUFF_GOAL_RADIUS, PUFF_GOAL_RADIUS);
        let fish: &dyn EntityBase = self.fish.as_ref();
        world.get_all_at_box(&search_box).into_iter().any(|target| {
            is_scary(target.as_ref())
                && self
                    .target_predicate
                    .test(&world, Some(fish), target.as_ref())
        })
    }

    fn start(&mut self, _mob: &dyn Mob) {
        self.fish.inflate_counter.store(1, Ordering::Relaxed);
        self.fish.deflate_timer.store(0, Ordering::Relaxed);
    }

    fn stop(&mut self, _mob: &dyn Mob) {
        self.fish.inflate_counter.store(0, Ordering::Relaxed);
    }
}

/// Vanilla `Pufferfish.SCARY_MOB`: nothing tagged `not_scary_for_pufferfish`, and creative
/// players are ignored.
fn is_scary(target: &dyn EntityBase) -> bool {
    if let Some(player) = target.get_player()
        && player.is_creative()
    {
        return false;
    }
    !target
        .get_entity()
        .entity_type
        .has_tag(&tag::EntityType::MINECRAFT_NOT_SCARY_FOR_PUFFERFISH)
}

/// Vanilla `Entity.doTeamsAllowDamage`.
fn do_teams_allow_damage(attacker: &dyn EntityBase, other: &dyn EntityBase) -> bool {
    let Some(own_team) = attacker.get_team() else {
        return true;
    };
    let allied = other
        .get_team()
        .is_some_and(|their_team| their_team.name == own_team.name);
    !allied || (own_team.options & 1) != 0
}

const fn poison_effect(puff_state: i32) -> Effect {
    Effect {
        effect_type: &StatusEffect::POISON,
        duration: POISON_TICKS_PER_STATE * puff_state,
        amplifier: 0,
        ambient: false,
        show_particles: true,
        show_icon: true,
        blend: false,
    }
}

const fn get_scale(state: i32) -> f32 {
    match state {
        STATE_SMALL => 0.5,
        STATE_MID => 0.7,
        _ => 1.0,
    }
}
