use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::Ordering;

use crate::world::World;
use pumpkin_data::FacingExt;
use pumpkin_util::math::boundingbox::BoundingBox;
use pumpkin_util::math::position::BlockPos;

use crate::block::{
    GetComparatorOutputArgs, GetScreenHandlerFactoryArgs, OnPlaceArgs, OnSyncedBlockEventArgs,
    PlacedArgs,
};
use crate::block::{
    registry::BlockActionResult,
    {BlockBehaviour, NormalUseArgs},
};

use crate::block::entities::shulker_box::ShulkerBoxBlockEntity;
use pumpkin_data::BlockStateId;
use pumpkin_data::translation;
use pumpkin_inventory::Inventory;
use pumpkin_inventory::generic_container_screen_handler::create_generic_9x3;
use pumpkin_inventory::player::player_inventory::PlayerInventory;
use pumpkin_inventory::screen_handler::{
    InventoryPlayer, ScreenHandlerFactory, SharedScreenHandler,
};
use pumpkin_macros::pumpkin_block_from_tag;
use pumpkin_util::text::TextComponent;

struct ShulkerBoxScreenFactory(Arc<dyn Inventory>);

impl ScreenHandlerFactory for ShulkerBoxScreenFactory {
    fn create_screen_handler(
        &self,
        sync_id: u8,
        player_inventory: &Arc<PlayerInventory>,
        player: &dyn InventoryPlayer,
    ) -> Option<SharedScreenHandler> {
        let handler = create_generic_9x3(sync_id, player_inventory, self.0.clone(), player);
        let screen_handler_arc = Arc::new(Mutex::new(handler));

        Some(screen_handler_arc as SharedScreenHandler)
    }

    fn get_display_name(&self) -> TextComponent {
        pumpkin_macros::translate_cross!(
            translation::java::CONTAINER_SHULKERBOX,
            translation::bedrock::CONTAINER_SHULKERBOX
        )
    }
}

#[pumpkin_block_from_tag("minecraft:shulker_boxes")]
pub struct ShulkerBoxBlock;

type EndRodLikeProperties = pumpkin_data::block_properties::EndRodLikeProperties;

impl BlockBehaviour for ShulkerBoxBlock {
    fn on_place(&self, args: OnPlaceArgs<'_>) -> BlockStateId {
        let mut props = EndRodLikeProperties::default(args.block);
        props.facing = args.direction.to_facing().opposite();
        props.to_state_id(args.block)
    }

    fn on_synced_block_event(&self, args: OnSyncedBlockEventArgs<'_>) -> bool {
        // On the server, we don't need the Animation steps for now, because the client is responsible for that.
        args.r#type == Self::OPEN_ANIMATION_EVENT_TYPE
    }

    fn placed(&self, args: PlacedArgs<'_>) {
        {
            let barrel_block_entity = ShulkerBoxBlockEntity::new(*args.position);
            args.world.add_block_entity(Arc::new(barrel_block_entity));
        }
    }

    fn normal_use(&self, args: NormalUseArgs<'_>) -> BlockActionResult {
        if !Self::can_open(args.world, args.position) {
            return BlockActionResult::Success;
        }
        if let Some(factory) = self.get_screen_handler_factory(GetScreenHandlerFactoryArgs {
            server: args.server,
            world: args.world,
            block: args.block,
            position: args.position,
            player: args.player,
        }) {
            args.player.increment_stat(
                pumpkin_data::statistic::StatisticCategory::Custom,
                pumpkin_data::statistic::CustomStatistic::OpenShulkerBox as i32,
                1,
            );
            args.player
                .open_handled_screen(factory.as_ref(), Some(*args.position));
        }

        BlockActionResult::Success
    }

    fn get_screen_handler_factory(
        &self,
        args: GetScreenHandlerFactoryArgs<'_>,
    ) -> Option<Box<dyn ScreenHandlerFactory>> {
        let block_entity = args.world.get_block_entity(args.position)?;
        let inventory = block_entity.get_inventory()?;
        Some(Box::new(ShulkerBoxScreenFactory(inventory)))
    }

    fn get_comparator_output(&self, args: GetComparatorOutputArgs<'_>) -> Option<u8> {
        crate::block::container_comparator_output(&args)
    }
}

impl ShulkerBoxBlock {
    pub const OPEN_ANIMATION_EVENT_TYPE: u8 = 1;
    /// Vanilla deflates the lid bounding box by this much.
    const LID_EPSILON: f64 = 1.0E-6;

    /// A closed box only opens if the half block in front of its lid is free of collisions.
    /// The server does not track the lid animation, so "closed" means nobody is viewing it.
    // TODO: ShulkerBoxBlockEntity.getAnimationStatus (a still-closing lid counts as open).
    fn can_open(world: &Arc<World>, position: &BlockPos) -> bool {
        if let Some(block_entity) = world.get_block_entity(position)
            && let Some(shulker) = block_entity
                .as_any()
                .downcast_ref::<ShulkerBoxBlockEntity>()
            && shulker.viewers.current.load(Ordering::Relaxed) > 0
        {
            return true;
        }
        let state = world.get_block_state(position);
        let facing = EndRodLikeProperties::from_state_id(state.id).facing;
        let offset = facing.to_block_direction().to_offset();
        let min = position.0.to_f64();
        let mut lid = BoundingBox::new(min, min.add_raw(1.0, 1.0, 1.0));
        let offsets = [
            f64::from(offset.x),
            f64::from(offset.y),
            f64::from(offset.z),
        ];
        // Slab of half a block thickness on the lid side, deflated by a hair like vanilla.
        for (axis, o) in offsets.into_iter().enumerate() {
            let (lo, hi) = if o > 0.0 {
                (1.0, 1.5)
            } else if o < 0.0 {
                (-0.5, 0.0)
            } else {
                (0.0, 1.0)
            };
            let base = [min.x, min.y, min.z][axis];
            let (lo, hi) = (base + lo + Self::LID_EPSILON, base + hi - Self::LID_EPSILON);
            match axis {
                0 => {
                    lid.min.x = lo;
                    lid.max.x = hi;
                }
                1 => {
                    lid.min.y = lo;
                    lid.max.y = hi;
                }
                _ => {
                    lid.min.z = lo;
                    lid.max.z = hi;
                }
            }
        }
        world.is_space_empty(lid)
    }
}
