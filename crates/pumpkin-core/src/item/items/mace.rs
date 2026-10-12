use std::sync::Arc;
use std::sync::atomic::Ordering;

use crate::enchantment::helper::EnchantmentHelper;
use crate::entity::decoration::armor_stand::ArmorStandEntity;
use crate::entity::player::Player;
use crate::entity::{EntityBase, living::LivingEntity};
use crate::item::{ItemBehaviour, ItemMetadata};
use pumpkin_data::attributes::Attributes;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::world::WorldEvent;
use pumpkin_util::GameMode;
use pumpkin_util::math::vector3::Vector3;

pub struct MaceItem;

impl MaceItem {
    pub const SMASH_ATTACK_FALL_THRESHOLD: f32 = 1.5;
    const SMASH_ATTACK_HEAVY_THRESHOLD: f32 = 5.0;
    pub const SMASH_ATTACK_KNOCKBACK_RADIUS: f64 = 3.5;
    const SMASH_ATTACK_KNOCKBACK_POWER: f64 = 0.7;
    /// Particle count used by the level event 2013.
    const SMASH_ATTACK_PARTICLE_DATA: i32 = 750;

    /// `MaceItem.canSmashAttack`
    pub fn can_smash_attack(attacker: &Player) -> bool {
        attacker.living_entity.fall_distance.load() > Self::SMASH_ATTACK_FALL_THRESHOLD
            && !attacker.living_entity.entity.is_fall_flying()
    }

    /// `MaceItem.getAttackDamageBonus`
    pub fn get_attack_damage_bonus(attacker: &Player, weapon: &ItemStack) -> f32 {
        if !Self::can_smash_attack(attacker) {
            return 0.0;
        }
        let fall_distance = f64::from(attacker.living_entity.fall_distance.load());
        let damage = if fall_distance <= 3.0 {
            4.0 * fall_distance
        } else if fall_distance <= 8.0 {
            12.0 + 2.0 * (fall_distance - 3.0)
        } else {
            22.0 + fall_distance - 8.0
        };
        let per_block = EnchantmentHelper::modify_fall_based_damage(weapon, 0.0);
        (damage + per_block * fall_distance) as f32
    }

    /// `MaceItem.hurtEnemy`
    pub fn hurt_enemy(victim: &Arc<dyn EntityBase>, attacker: &Player) {
        if !Self::can_smash_attack(attacker) {
            return;
        }
        let world = attacker.world();
        let attacker_entity = &attacker.living_entity.entity;
        let velocity = attacker_entity.velocity.load();
        attacker_entity.set_velocity(Vector3::new(velocity.x, 0.01, velocity.z));
        // TODO: setIgnoreFallDamageFromCurrentImpulse is not modelled, so the attacker still takes
        // the fall damage of the landing after the smash.

        let victim_entity = victim.get_entity();
        let sound = if victim_entity.on_ground.load(Ordering::Relaxed) {
            attacker
                .spawn_extra_particles_on_fall
                .store(true, Ordering::Relaxed);
            if attacker.living_entity.fall_distance.load() > Self::SMASH_ATTACK_HEAVY_THRESHOLD {
                Sound::ItemMaceSmashGroundHeavy
            } else {
                Sound::ItemMaceSmashGround
            }
        } else {
            Sound::ItemMaceSmashAir
        };
        world.play_sound(sound, SoundCategory::Players, &attacker_entity.pos.load());

        Self::knockback(attacker, victim);
    }

    /// `MaceItem.postHurtEnemy`
    pub fn post_hurt_enemy(attacker: &Player) {
        if Self::can_smash_attack(attacker) {
            attacker.living_entity.fall_distance.store(0.0);
        }
    }

    fn knockback(attacker: &Player, entity: &Arc<dyn EntityBase>) {
        let world = attacker.world();
        let target = entity.get_entity();
        let target_pos = target.pos.load();
        world.sync_world_event(
            WorldEvent::ParticlesSmashAttack,
            target.get_on_pos(),
            Self::SMASH_ATTACK_PARTICLE_DATA,
        );

        let area = target
            .bounding_box
            .load()
            .expand_all(Self::SMASH_ATTACK_KNOCKBACK_RADIUS);
        for nearby in world.get_all_at_box(&area) {
            let Some(living) = nearby.get_living_entity() else {
                continue;
            };
            if !Self::knockback_predicate(attacker, entity, &nearby) {
                continue;
            }
            let direction = nearby.get_entity().pos.load().sub(&target_pos);
            let power = Self::get_knockback_power(attacker, living, direction.length());
            if power > 0.0 {
                let scaled = direction.normalize().multiply(power, 0.0, power);
                nearby.get_entity().add_velocity(Vector3::new(
                    scaled.x,
                    Self::SMASH_ATTACK_KNOCKBACK_POWER,
                    scaled.z,
                ));
            }
        }
    }

    fn knockback_predicate(
        attacker: &Player,
        entity: &Arc<dyn EntityBase>,
        nearby: &Arc<dyn EntityBase>,
    ) -> bool {
        let nearby_entity = nearby.get_entity();
        let not_player = nearby_entity.entity_id != attacker.living_entity.entity.entity_id
            && nearby_entity.entity_id != entity.get_entity().entity_id;
        let not_tamed_by_player = !nearby.get_mob().is_some_and(|mob| {
            // Vanilla compares the owner with the entity that was hit, not the attacker.
            mob.is_tamed() && mob.get_owner_uuid() == Some(entity.get_entity().entity_uuid)
        });
        let not_marker_armor_stand = !nearby
            .cast_any()
            .downcast_ref::<ArmorStandEntity>()
            .is_some_and(ArmorStandEntity::is_marker);
        let within_range = entity
            .get_entity()
            .pos
            .load()
            .squared_distance_to_vec(&nearby_entity.pos.load())
            <= Self::SMASH_ATTACK_KNOCKBACK_RADIUS * Self::SMASH_ATTACK_KNOCKBACK_RADIUS;
        let not_flying_in_creative =
            nearby
                .cast_any()
                .downcast_ref::<Player>()
                .is_none_or(|player| {
                    !(player.is_creative()
                        && player
                            .abilities
                            .lock()
                            .is_ok_and(|abilities| abilities.flying))
                });
        !nearby.is_spectator()
            && not_player
            && !(attacker as &dyn EntityBase).is_allied_to(nearby.as_ref())
            && not_tamed_by_player
            && not_marker_armor_stand
            && within_range
            && not_flying_in_creative
    }

    fn get_knockback_power(attacker: &Player, nearby: &LivingEntity, distance: f64) -> f64 {
        let heavy =
            if attacker.living_entity.fall_distance.load() > Self::SMASH_ATTACK_HEAVY_THRESHOLD {
                2.0
            } else {
                1.0
            };
        (Self::SMASH_ATTACK_KNOCKBACK_RADIUS - distance)
            * Self::SMASH_ATTACK_KNOCKBACK_POWER
            * heavy
            * (1.0 - nearby.get_attribute_value(&Attributes::KNOCKBACK_RESISTANCE))
    }
}

impl ItemMetadata for MaceItem {
    fn ids() -> Box<[u16]> {
        [Item::MACE.id].into()
    }
}
impl ItemBehaviour for MaceItem {
    fn can_mine(&self, player: &Player) -> bool {
        player.gamemode.load() != GameMode::Creative
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
