use std::sync::Arc;
use std::sync::atomic::Ordering;

use pumpkin_data::entity::EntityType;

use crate::entity::mob::Mob;
use crate::entity::{Entity, EntityBase};

/// Vanilla `ConversionParams` for `ConversionType.SINGLE`.
#[derive(Clone, Copy)]
pub struct ConversionParams {
    pub keep_equipment: bool,
    pub preserve_can_pick_up_loot: bool,
}

impl ConversionParams {
    #[must_use]
    pub const fn single(keep_equipment: bool, preserve_can_pick_up_loot: bool) -> Self {
        Self {
            keep_equipment,
            preserve_can_pick_up_loot,
        }
    }
}

/// Vanilla `Mob.convertTo` with `ConversionType.SINGLE`: the new mob takes over the old one's
/// state, `after_conversion` runs before it is added to the world, and the old mob is removed.
///
/// `is_baby` is the old mob's baby state; vanilla reads `Mob.isBaby`.
// TODO: vanilla also moves the vehicle, passengers, leash, sleeping position, ANGRY_AT memory and
// scoreboard team over (`ConversionType.SINGLE.convert`).
pub fn convert_to(
    from: &dyn Mob,
    is_baby: bool,
    entity_type: &'static EntityType,
    params: ConversionParams,
    after_conversion: impl FnOnce(&Arc<dyn EntityBase>),
) -> Option<Arc<dyn EntityBase>> {
    let from_mob = from.get_mob_entity();
    let from_living = &from_mob.living_entity;
    let from_entity = &from_living.entity;
    if from_entity.is_removed() {
        return None;
    }

    let world = from_entity.world.load_full();
    let converted = crate::entity::r#type::from_type(
        entity_type,
        from_entity.pos.load(),
        &world,
        uuid::Uuid::new_v4(),
    );

    copy_entity_state(from_entity, converted.get_entity());

    if let Some(to_mob) = converted.get_mob() {
        let to_mob_entity = to_mob.get_mob_entity();
        let to_living = &to_mob_entity.living_entity;

        if params.keep_equipment {
            let from_equipment = from_living
                .entity_equipment
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut to_equipment = to_living
                .entity_equipment
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut drop_chances = to_living
                .equipment_drop_chances
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            for (slot, stack) in &from_equipment.equipment {
                if !stack.is_empty() {
                    to_equipment.put(slot, stack.clone());
                    drop_chances.insert(slot.clone(), from_mob.drop_chance(slot));
                }
            }
        }

        to_living
            .fall_distance
            .store(from_living.fall_distance.load());
        to_living.set_absorption(from_living.get_absorption());
        let effects: Vec<_> = from_living
            .active_effects
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
            .cloned()
            .collect();
        for effect in effects {
            to_living.add_effect(effect);
        }
        if is_baby && !to_mob.spawn_as_baby() {
            // Mobs without a baby state of their own keep the old age, which is what marks a baby.
            converted
                .get_entity()
                .age
                .store(from_entity.age.load(Ordering::Relaxed), Ordering::Relaxed);
        }
        if params.preserve_can_pick_up_loot {
            to_mob_entity.set_can_pick_up_loot(from_mob.can_pick_up_loot());
        }
        to_mob_entity.set_left_handed(from_mob.is_left_handed());
        to_mob_entity.set_no_ai(from_mob.is_no_ai());
        to_mob_entity.persistence_required.store(
            from_mob.persistence_required.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
    }

    after_conversion(&converted);
    world.spawn_entity(converted.clone());
    from_entity.remove();
    Some(converted)
}

/// The `Entity`-level part of `ConversionType.SINGLE`.
fn copy_entity_state(from_entity: &Entity, to_entity: &Entity) {
    to_entity.set_rotation(from_entity.yaw.load(), from_entity.pitch.load());
    to_entity.head_yaw.store(from_entity.head_yaw.load());
    to_entity.velocity.store(from_entity.velocity.load());
    to_entity.body_yaw.store(from_entity.body_yaw.load());
    to_entity.on_ground.store(
        from_entity.on_ground.load(Ordering::Relaxed),
        Ordering::Relaxed,
    );
    if let Some(custom_name) = &**from_entity.custom_name.load() {
        to_entity.set_custom_name(custom_name.clone());
    }
    to_entity.set_custom_name_visible(from_entity.custom_name_visible.load(Ordering::Relaxed));
    if from_entity.is_on_fire() {
        to_entity.set_on_fire(true);
    }
    to_entity.invulnerable.store(
        from_entity.invulnerable.load(Ordering::Relaxed),
        Ordering::Relaxed,
    );
    to_entity.has_no_gravity.store(
        from_entity.has_no_gravity.load(Ordering::Relaxed),
        Ordering::Relaxed,
    );
    to_entity.portal_cooldown.store(
        from_entity.portal_cooldown.load(Ordering::Relaxed),
        Ordering::Relaxed,
    );
    to_entity.set_silent(from_entity.is_silent());
    to_entity
        .scoreboard_tags
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone_from(
            &from_entity
                .scoreboard_tags
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
}
