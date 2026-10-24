use std::sync::atomic::Ordering;
use std::sync::{Arc, Weak};

use pumpkin_data::damage::DamageType;
use pumpkin_data::effect::StatusEffect;
use pumpkin_data::potion::Effect;
use pumpkin_data::sound::Sound;
use pumpkin_protocol::java::client::play::{CGameEvent, GameEvent};
use pumpkin_util::GameMode;
use pumpkin_util::math::position::BlockPos;

use crate::entity::custom_sound::CustomSound;
use crate::entity::mob::guardian::{
    GuardianMob, GuardianState, guardian_hurt, guardian_travel_in_water,
    guardian_walk_target_value, register_guardian, tick_guardian,
};
use crate::entity::{
    Entity, EntityBase,
    mob::{Mob, MobEntity},
};

const ATTACK_DURATION: i32 = 60;
const STROLL_INTERVAL: i32 = 400;
const HOME_RADIUS: i32 = 16;
const EFFECT_INTERVAL: i32 = 1200;
const EFFECT_RADIUS: f64 = 50.0;
const EFFECT_DURATION: i32 = 6000;
const EFFECT_AMPLIFIER: u8 = 2;
const EFFECT_DISPLAY_LIMIT: i32 = 1200;

pub struct ElderGuardianEntity {
    pub mob_entity: MobEntity,
    state: GuardianState,
}

impl ElderGuardianEntity {
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
            .mob_entity
            .persistence_required
            .store(true, Ordering::Relaxed);
        mob_arc
    }

    /// Vanilla `MobEffectUtil.addEffectToPlayersAround` with the mining fatigue curse.
    fn curse_players_around(&self) {
        let entity = &self.mob_entity.living_entity.entity;
        let world = entity.world.load();
        let event_value = if entity.is_silent() { 0.0 } else { 1.0 };
        let packet = CGameEvent::new(GameEvent::PlayElderGuardianMobAppearance, event_value);

        for player in world.get_nearby_players(entity.pos.load(), EFFECT_RADIUS) {
            if !matches!(
                player.gamemode.load(),
                GameMode::Survival | GameMode::Adventure
            ) || (self as &dyn EntityBase).is_allied_to(player.as_ref())
            {
                continue;
            }
            let needs_effect = player
                .living_entity
                .get_effect(&StatusEffect::MINING_FATIGUE)
                .is_none_or(|current| {
                    current.amplifier < EFFECT_AMPLIFIER || current.duration < EFFECT_DISPLAY_LIMIT
                });
            if !needs_effect {
                continue;
            }

            player.living_entity.add_effect(Effect {
                effect_type: &StatusEffect::MINING_FATIGUE,
                duration: EFFECT_DURATION,
                amplifier: EFFECT_AMPLIFIER,
                ambient: false,
                show_particles: true,
                show_icon: true,
                blend: false,
            });
            player.try_send_client_packet(&packet);
        }
    }
}

impl CustomSound for ElderGuardianEntity {
    fn hurt_sound(&self) -> Option<Sound> {
        Some(
            if self.get_entity().touching_water.load(Ordering::Relaxed) {
                Sound::EntityElderGuardianHurt
            } else {
                Sound::EntityElderGuardianHurtLand
            },
        )
    }

    fn death_sound(&self) -> Option<Sound> {
        Some(
            if self.get_entity().touching_water.load(Ordering::Relaxed) {
                Sound::EntityElderGuardianDeath
            } else {
                Sound::EntityElderGuardianDeathLand
            },
        )
    }
}

impl GuardianMob for ElderGuardianEntity {
    fn guardian_state(&self) -> &GuardianState {
        &self.state
    }

    fn attack_duration(&self) -> i32 {
        ATTACK_DURATION
    }

    fn is_elder(&self) -> bool {
        true
    }
}

impl Mob for ElderGuardianEntity {
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

    fn custom_server_ai_step(&self, _caller: &dyn EntityBase) {
        let entity = &self.mob_entity.living_entity.entity;
        if (entity.tick_count.load(Ordering::Relaxed) + entity.entity_id) % EFFECT_INTERVAL == 0 {
            self.curse_players_around();
        }

        if !self.mob_entity.has_position_target() {
            self.mob_entity
                .position_target
                .store(entity.block_pos.load());
            self.mob_entity
                .position_target_range
                .store(HOME_RADIUS, Ordering::Relaxed);
        }
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
