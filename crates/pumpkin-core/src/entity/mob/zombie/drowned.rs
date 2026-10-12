use std::sync::Arc;

use crate::entity::Entity;
use crate::entity::mob::equipment::RegionalDifficulty;
use crate::world::World;
use pumpkin_data::data_component_impl::EquipmentSlot;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;

/// Rolls above this give a drowned a trident or fishing rod.
const DROWNED_HELD_ITEM_CHANCE: f32 = 0.9;
use crate::entity::mob::zombie::{ZombieEntityBase, ZombieKind};

pub struct DrownedEntity {
    entity: Arc<ZombieEntityBase>,
}

impl DrownedEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let entity = ZombieEntityBase::new(entity, ZombieKind::Drowned);
        let zombie = Self { entity };
        let mob_arc = Arc::new(zombie);
        // Fix duplicated since already in ZombieEntity::new()
        {
            //let mut target_selector = mob_arc.entity.mob_entity.target_selector.lock().unwrap_or_else(std::sync::PoisonError::into_inner);

            // TODO
            // target_selector.add_goal(
            //     2,
            //     ActiveTargetGoal::with_default(
            //         &mob_arc.entity.mob_entity,
            //         &EntityType::PLAYER,
            //         true,
            //     ),
            // );
        };

        mob_arc
    }

    #[must_use]
    pub fn with_can_break_doors(entity: Entity, can_break_doors: bool) -> Arc<Self> {
        let entity =
            ZombieEntityBase::with_can_break_doors(entity, ZombieKind::Drowned, can_break_doors);
        let zombie = Self { entity };
        Arc::new(zombie)
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
});
