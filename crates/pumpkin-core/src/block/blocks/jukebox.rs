use std::sync::Arc;

use crate::block::entities::jukebox::JukeboxBlockEntity;
use crate::block::registry::BlockActionResult;
use crate::block::{
    BlockBehaviour, EmitsRedstonePowerArgs, GetComparatorOutputArgs, GetRedstonePowerArgs,
    NormalUseArgs, OnStateReplacedArgs, PlacedArgs, PlayerPlacedArgs, UseWithItemArgs,
};
use pumpkin_data::block_properties::JukeboxLikeProperties;
use pumpkin_data::data_component_impl::JukeboxPlayableImpl;
use pumpkin_data::game_event::GameEvent;
use pumpkin_macros::pumpkin_block;
use pumpkin_world::world::BlockFlags;

#[pumpkin_block("minecraft:jukebox")]
pub struct JukeboxBlock;

impl BlockBehaviour for JukeboxBlock {
    fn placed(&self, args: PlacedArgs<'_>) {
        let block_entity = JukeboxBlockEntity::new(*args.position);
        block_entity.bind_world(args.world);
        args.world.add_block_entity(Arc::new(block_entity));
    }

    /// `JukeboxBlock.setPlacedBy`: an item carrying `RecordItem` data places a loaded jukebox.
    fn player_placed(&self, args: PlayerPlacedArgs<'_>) {
        if let Some(block_entity) = args.world.get_block_entity(args.position)
            && let Some(jukebox) = block_entity.as_any().downcast_ref::<JukeboxBlockEntity>()
            && !jukebox.get_record().is_empty()
        {
            let state = JukeboxLikeProperties { has_record: true };
            args.world.set_block_state(
                args.position,
                state.to_state_id(args.block),
                BlockFlags::NOTIFY_LISTENERS,
            );
        }
    }

    /// `JukeboxBlock.useWithoutItem`
    fn normal_use(&self, args: NormalUseArgs<'_>) -> BlockActionResult {
        let state_id = args.world.get_block_state(args.position).id;
        if let Some(block_entity) = args.world.get_block_entity(args.position)
            && let Some(jukebox) = block_entity.as_any().downcast_ref::<JukeboxBlockEntity>()
            && (JukeboxLikeProperties::from_state_id(state_id).has_record
                || !jukebox.get_record().is_empty())
        {
            jukebox.pop_out_the_item(args.world);
            return BlockActionResult::Success;
        }
        BlockActionResult::Pass
    }

    /// `JukeboxBlock.useItemOn` -> `JukeboxPlayable.tryInsertIntoJukebox`
    fn use_with_item(&self, args: UseWithItemArgs<'_>) -> BlockActionResult {
        let world = args.world;
        let state_id = world.get_block_state(args.position).id;
        let has_record_state = JukeboxLikeProperties::from_state_id(state_id).has_record;
        if has_record_state {
            return BlockActionResult::PassToDefaultBlockAction;
        }
        if args
            .item_stack
            .get_data_component::<JukeboxPlayableImpl>()
            .is_none()
        {
            return BlockActionResult::PassToDefaultBlockAction;
        }

        let Some(block_entity) = world.get_block_entity(args.position) else {
            return BlockActionResult::PassToDefaultBlockAction;
        };
        let Some(jukebox) = block_entity.as_any().downcast_ref::<JukeboxBlockEntity>() else {
            return BlockActionResult::PassToDefaultBlockAction;
        };

        // A hopper may have filled the jukebox before the state caught up.
        if !jukebox.get_record().is_empty() {
            return BlockActionResult::PassToDefaultBlockAction;
        }

        let inserted = args
            .item_stack
            .split_unless_creative(args.player.gamemode.load(), 1);
        jukebox.set_the_item(world, inserted);
        world.emit_game_event(
            GameEvent::BlockChange.name(),
            args.position.to_centered_f64(),
        );

        args.player.increment_stat(
            pumpkin_data::statistic::StatisticCategory::Custom,
            pumpkin_data::statistic::CustomStatistic::PlayRecord as i32,
            1,
        );

        BlockActionResult::Success
    }

    /// `JukeboxBlock.affectNeighborsAfterRemoval` -> `Containers.updateNeighboursAfterDestroy`
    fn on_state_replaced(&self, args: OnStateReplacedArgs<'_>) {
        args.world
            .update_neighbour_for_output_signal(args.position, args.block);
    }

    fn emits_redstone_power(&self, _args: EmitsRedstonePowerArgs<'_>) -> bool {
        true
    }

    /// `JukeboxBlock.ownSignal`
    fn get_weak_redstone_power(&self, args: GetRedstonePowerArgs<'_>) -> u8 {
        if let Some(block_entity) = args.world.get_block_entity(args.position)
            && let Some(jukebox) = block_entity.as_any().downcast_ref::<JukeboxBlockEntity>()
            && jukebox.is_playing()
        {
            15
        } else {
            0
        }
    }

    /// `JukeboxBlock.getAnalogOutputSignal`
    fn get_comparator_output(&self, args: GetComparatorOutputArgs<'_>) -> Option<u8> {
        let output = args
            .world
            .get_block_entity(args.position)
            .and_then(|block_entity| {
                block_entity
                    .as_any()
                    .downcast_ref::<JukeboxBlockEntity>()
                    .map(JukeboxBlockEntity::get_comparator_output)
            });
        Some(output.unwrap_or(0))
    }
}
