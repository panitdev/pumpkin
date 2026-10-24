use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};

use pumpkin_data::damage::DamageType;
use pumpkin_data::particle::Particle;
use pumpkin_data::sound::Sound;
use pumpkin_data::tracked_data;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::vector3::Vector3;
use rand::RngExt;

use crate::entity::ageable::{AgeableData, AgeableMob};
use crate::entity::mob::{Mob, MobEntity};
use crate::entity::passive::squid::{
    SquidMob, SquidState, register_squid, reset_ambient_sound_time, spawn_ink_if_hurt,
    squid_travel_in_water, tick_ambient_sound, tick_squid,
};
use crate::entity::{Entity, EntityBase};

/// Vanilla `GlowSquid.hurtServer` dark ticks.
const DARK_TICKS_ON_HURT: i32 = 100;
/// Vanilla `GlowSquid.aiStep` glow particle spread.
const GLOW_PARTICLE_SPREAD: f64 = 0.6;

/// Represents a Glow Squid, a passive aquatic mob that emits a glowing particle effect.
///
/// Wiki: <https://minecraft.wiki/w/Glow_Squid>
pub struct GlowSquidEntity {
    pub mob_entity: MobEntity,
    pub ageable_data: AgeableData,
    state: SquidState,
    dark_ticks_remaining: AtomicI32,
}

impl GlowSquidEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let glow_squid = Self {
            mob_entity,
            ageable_data: AgeableData::default(),
            state: SquidState::new(),
            dark_ticks_remaining: AtomicI32::new(0),
        };
        let mob_arc = Arc::new(glow_squid);

        register_squid(&mob_arc.mob_entity);
        mob_arc
    }

    /// Vanilla `GlowSquid.setDarkTicks` / `getDarkTicksRemaining`.
    fn set_dark_ticks(&self, ticks: i32) {
        self.dark_ticks_remaining.store(ticks, Ordering::Relaxed);
        self.get_entity()
            .set_synced_data(tracked_data::glow_squid::DATA_DARK_TICKS_REMAINING, ticks);
    }

    /// Vanilla `GlowSquid.aiStep`: count the dark ticks down and emit a glow particle.
    fn tick_glow(&self) {
        let dark_ticks = self.dark_ticks_remaining.load(Ordering::Relaxed);
        if dark_ticks > 0 {
            self.set_dark_ticks(dark_ticks - 1);
        }

        let entity = self.get_entity();
        let pos = entity.pos.load();
        let dimensions = entity.entity_dimension.load();
        let mut rng = rand::rng();
        let x = pos.x
            + f64::from(dimensions.width)
                * (2.0 * rng.random::<f64>() - 1.0)
                * GLOW_PARTICLE_SPREAD;
        let y = pos.y + f64::from(dimensions.height) * rng.random::<f64>();
        let z = pos.z
            + f64::from(dimensions.width)
                * (2.0 * rng.random::<f64>() - 1.0)
                * GLOW_PARTICLE_SPREAD;
        entity.world.load().spawn_particles(
            Particle::Glow,
            Vector3::new(x, y, z),
            0,
            Vector3::new(0.0, 0.0, 0.0),
            0.0,
        );
    }
}

impl AgeableMob for GlowSquidEntity {
    fn get_ageable_data(&self) -> &AgeableData {
        &self.ageable_data
    }
}

impl SquidMob for GlowSquidEntity {
    fn squid_state(&self) -> &SquidState {
        &self.state
    }

    fn ink_particle(&self) -> Particle {
        Particle::GlowSquidInk
    }

    fn squirt_sound(&self) -> Sound {
        Sound::EntityGlowSquidSquirt
    }

    fn ambient_sound(&self) -> Sound {
        Sound::EntityGlowSquidAmbient
    }
}

impl Mob for GlowSquidEntity {
    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn as_squid(&self) -> Option<&dyn SquidMob> {
        Some(self)
    }

    fn as_ageable(&self) -> Option<&dyn AgeableMob> {
        Some(self)
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        nbt.put_int(
            "DarkTicksRemaining",
            self.dark_ticks_remaining.load(Ordering::Relaxed),
        );
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.set_dark_ticks(nbt.get_int("DarkTicksRemaining").unwrap_or(0));
    }

    fn post_tick(&self) {
        tick_squid(self);
        self.tick_glow();
        tick_ambient_sound(self);
    }

    fn travel_in_water(&self, caller: &dyn EntityBase, _gravity: f64, _falling: bool) {
        let movement_input = self.get_mob_entity().living_entity.movement_input.load();
        squid_travel_in_water(self, caller, movement_input);
    }

    fn mob_is_pushed_by_fluids(&self) -> bool {
        false
    }

    fn on_damage(&self, _damage_type: DamageType, _source: Option<&dyn EntityBase>) {
        // Vanilla `GlowSquid.hurtServer`: `super.hurtServer` (Squid), then dark ticks. Dark
        // ticks are set only when `Squid.hurtServer` returns true, which needs a living attacker.
        reset_ambient_sound_time(self);
        if spawn_ink_if_hurt(self) {
            self.set_dark_ticks(DARK_TICKS_ON_HURT);
        }
    }
}
