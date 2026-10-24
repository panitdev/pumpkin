use std::sync::Arc;
use std::sync::atomic::Ordering;

use crate::entity::EntityBase;
use crate::entity::player::Player;
use crate::item::{ItemBehaviour, ItemMetadata};
use pumpkin_data::data_component_impl::CustomNameImpl;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;

pub struct NameTagItem;

impl ItemMetadata for NameTagItem {
    fn ids() -> Box<[u16]> {
        [Item::NAME_TAG.id].into()
    }
}

impl ItemBehaviour for NameTagItem {
    // NameTagItem.interactLivingEntity
    fn use_on_entity(&self, item: &mut ItemStack, player: &Player, entity: Arc<dyn EntityBase>) {
        if entity.get_living_entity().is_none() {
            return;
        }
        let base = entity.get_entity();
        if !base.entity_type.saveable || !base.is_alive() {
            return;
        }
        let Some(name) = item.get_data_component::<CustomNameImpl>() else {
            return;
        };
        base.set_custom_name(name.name.clone());
        if let Some(mob) = entity.get_mob() {
            mob.get_mob_entity()
                .persistence_required
                .store(true, Ordering::Relaxed);
            mob.on_custom_name_set(&name.name);
        }
        item.decrement_unless_creative(player.gamemode.load(), 1);
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
