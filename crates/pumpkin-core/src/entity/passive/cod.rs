use std::sync::{Arc, Weak};

use pumpkin_data::sound::Sound;
use pumpkin_util::math::vector3::Vector3;

use crate::entity::{
    Entity, EntityBase,
    mob::{Mob, MobEntity},
    passive::fish::{self, AbstractFish, AbstractSchoolingFish, FishState},
};

/// Represents a Cod, a common passive aquatic mob.
///
/// Wiki: <https://minecraft.wiki/w/Cod>
pub struct CodEntity {
    pub mob_entity: MobEntity,
    schooling: FishState,
}

impl CodEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let cod = Self {
            mob_entity,
            schooling: FishState::default(),
        };
        let mob_arc = Arc::new(cod);
        let entity_weak: Weak<dyn EntityBase> = {
            let entity_arc: Arc<dyn EntityBase> = mob_arc.clone();
            Arc::downgrade(&entity_arc)
        };

        fish::register_fish(&mob_arc.mob_entity);
        fish::register_schooling_goals(&mob_arc.mob_entity, &entity_weak);

        mob_arc
    }
}

impl AbstractFish for CodEntity {
    fn flop_sound(&self) -> Sound {
        Sound::EntityCodFlop
    }
}

impl AbstractSchoolingFish for CodEntity {
    fn schooling(&self) -> &FishState {
        &self.schooling
    }
}

impl Mob for CodEntity {
    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn as_schooling_fish(&self) -> Option<&dyn AbstractSchoolingFish> {
        Some(self)
    }

    fn mob_tick(&self, _caller: &dyn EntityBase) {
        fish::fish_tick(self);
    }

    fn travel_in_water(&self, caller: &dyn EntityBase, movement_input: Vector3<f64>) -> bool {
        fish::fish_travel_in_water(self, caller, movement_input);
        true
    }
}
