use std::any::Any;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use crate::enchantment::helper::EnchantmentHelper;
use crate::entity::Entity;
use crate::entity::player::Player;
use crate::entity::projectile::fishing_bobber::FishingBobberEntity;
use crate::item::{ItemBehaviour, ItemMetadata};
use pumpkin_data::entity::EntityType;
use pumpkin_data::item::Item;
use pumpkin_data::sound::{Sound, SoundCategory};

pub struct FishingRodItem;

impl ItemMetadata for FishingRodItem {
    fn ids() -> Box<[u16]> {
        Box::new([Item::FISHING_ROD.id])
    }
}

impl ItemBehaviour for FishingRodItem {
    // FishingRodItem.use
    fn normal_use(&self, _item: &Item, player: &Player) {
        let world = player.world();
        let bobber_id = player.fishing_bobber.load(Ordering::Relaxed);
        let rod = player.inventory().held_item();
        let pitch = 0.4 / (rand::random::<f32>() * 0.4 + 0.8);

        if bobber_id == -1 {
            world.play_sound_fine(
                Sound::EntityFishingBobberThrow,
                SoundCategory::Neutral,
                &player.position(),
                0.5,
                pitch,
            );

            let lure_speed =
                (EnchantmentHelper::modify_fishing_time_reduction(&rod, 0.0) * 20.0) as i32;
            let luck = EnchantmentHelper::modify_fishing_luck_bonus(&rod, 0.0) as i32;
            let bobber_entity = Entity::new(
                world.clone(),
                player.position(),
                &EntityType::FISHING_BOBBER,
            );
            let bobber = FishingBobberEntity::new(bobber_entity, player, luck, lure_speed);

            player
                .fishing_bobber
                .store(bobber.entity.entity_id, Ordering::Relaxed);

            let bobber_arc: Arc<FishingBobberEntity> = Arc::new(bobber);
            world.spawn_entity(bobber_arc);
        } else {
            if let Some(bobber_base) = world.get_entity_by_id(bobber_id) {
                if let Some(bobber) = bobber_base.cast_any().downcast_ref::<FishingBobberEntity>() {
                    let damage = bobber.retrieve(player, &rod);
                    player.damage_held_item(damage);
                }
                bobber_base.get_entity().remove();
            }
            player.fishing_bobber.store(-1, Ordering::Relaxed);

            world.play_sound_fine(
                Sound::EntityFishingBobberRetrieve,
                SoundCategory::Neutral,
                &player.position(),
                1.0,
                pitch,
            );
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
