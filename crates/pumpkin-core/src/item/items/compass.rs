use std::any::Any;

use crate::block::registry::BlockActionResult;
use crate::entity::player::Player;
use crate::item::{ItemBehaviour, ItemMetadata};
use crate::server::Server;
use pumpkin_data::data_component_impl::{LodestoneTarget, LodestoneTrackerImpl};
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::{Block, BlockDirection};
use pumpkin_util::GameMode;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;

pub struct CompassItem;

impl ItemMetadata for CompassItem {
    fn ids() -> Box<[u16]> {
        Box::new([Item::COMPASS.id])
    }
}

impl ItemBehaviour for CompassItem {
    fn use_on_block(
        &self,
        item: &mut ItemStack,
        player: &Player,
        location: BlockPos,
        _face: BlockDirection,
        _cursor_pos: Vector3<f32>,
        block: &Block,
        _server: &Server,
    ) -> BlockActionResult {
        if block.id != Block::LODESTONE.id {
            return BlockActionResult::Pass;
        }

        let world = player.world();
        world.play_sound(
            Sound::ItemLodestoneCompassLock,
            SoundCategory::Players,
            &location.to_f64(),
        );

        let target = LodestoneTrackerImpl {
            target: Some(LodestoneTarget {
                dimension: world.dimension.minecraft_name.to_string(),
                x: location.0.x,
                y: location.0.y,
                z: location.0.z,
            }),
            tracked: true,
        };

        let gamemode = player.gamemode.load();
        // Vanilla `CompassItem.useOn`: a single non-creative compass is bound in place,
        // otherwise a fresh compass is created and one item is consumed.
        let replace_existing_stack = gamemode != GameMode::Creative && item.item_count == 1;
        if replace_existing_stack {
            item.set_data_component(target);
        } else {
            let mut lodestone_compass = item.copy_with_count(1);
            lodestone_compass.item = &Item::COMPASS;
            item.decrement_unless_creative(gamemode, 1);
            lodestone_compass.set_data_component(target);
            let was_added = player
                .inventory
                .insert_stack_anywhere(&mut lodestone_compass);
            if !was_added && !lodestone_compass.is_empty() {
                world.drop_stack(&player.position().to_block_pos(), lodestone_compass);
            }
        }

        BlockActionResult::Success
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
