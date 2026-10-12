use std::sync::Arc;

use crate::block::blocks::plant::big_dripleaf::{
    can_grow_into, is_water_source, place as place_leaf,
};
use crate::block::{
    BlockBehaviour, BonemealArgs, GetStateForNeighborUpdateArgs, OnScheduledTickArgs,
};
use crate::world::World;
use pumpkin_data::block_properties::{HorizontalFacing, LadderLikeProperties};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_data::{Block, BlockDirection, BlockStateId};
use pumpkin_macros::pumpkin_block;
use pumpkin_util::math::position::BlockPos;
use pumpkin_world::tick::TickPriority;
use pumpkin_world::world::{BlockAccessor, BlockFlags};

#[pumpkin_block("minecraft:big_dripleaf_stem")]
pub struct BigDripleafStemBlock;

pub type BigDripleafStemLikeProperties = LadderLikeProperties;

impl BlockBehaviour for BigDripleafStemBlock {
    fn get_state_for_neighbor_update(
        &self,
        args: GetStateForNeighborUpdateArgs<'_>,
    ) -> BlockStateId {
        if matches!(args.direction, BlockDirection::Down | BlockDirection::Up)
            && !can_survive(args.world, args.position)
        {
            args.world
                .schedule_block_tick(args.block, *args.position, 1, TickPriority::Normal);
        }
        args.state_id
    }

    fn on_scheduled_tick(&self, args: OnScheduledTickArgs<'_>) {
        if !can_survive(args.world.as_ref(), args.position) {
            args.world
                .break_block(args.position, None, BlockFlags::NOTIFY_ALL);
        }
    }

    fn is_valid_bonemeal_target(&self, args: BonemealArgs<'_>) -> bool {
        get_top_connected_dripleaf(args.world, args.position)
            .is_some_and(|head_pos| can_grow_into(args.world, &head_pos.up()))
    }

    fn perform_bonemeal(&self, args: BonemealArgs<'_>) {
        if let Some(head_pos) = get_top_connected_dripleaf(args.world, args.position) {
            let place_head_pos = head_pos.up();
            let facing = BigDripleafStemLikeProperties::from_state_id(args.state_id).facing;
            place_stem(
                args.world,
                &head_pos,
                is_water_source(args.world, &head_pos),
                facing,
            );
            place_leaf(
                args.world,
                &place_head_pos,
                is_water_source(args.world, &place_head_pos),
                facing,
            );
        }
    }
}

/// `BigDripleafStemBlock.canSurvive`
fn can_survive(world: &dyn BlockAccessor, pos: &BlockPos) -> bool {
    let below = world.get_block(&pos.down());
    let above = world.get_block(&pos.up());
    (below == &Block::BIG_DRIPLEAF_STEM
        || below.has_tag(&tag::Block::MINECRAFT_SUPPORTS_BIG_DRIPLEAF))
        && (above == &Block::BIG_DRIPLEAF_STEM || above == &Block::BIG_DRIPLEAF)
}

/// `BlockUtil.getTopConnectedBlock` going up through stems and ending at a big dripleaf.
fn get_top_connected_dripleaf(world: &World, pos: &BlockPos) -> Option<BlockPos> {
    let mut current = *pos;
    loop {
        let block = world.get_block(&current);
        if block == &Block::BIG_DRIPLEAF {
            return Some(current);
        }
        if block != &Block::BIG_DRIPLEAF_STEM {
            return None;
        }
        current = current.up();
    }
}

/// `BigDripleafStemBlock.place`
pub fn place_stem(world: &Arc<World>, pos: &BlockPos, waterlogged: bool, facing: HorizontalFacing) {
    let mut props = BigDripleafStemLikeProperties::default(&Block::BIG_DRIPLEAF_STEM);
    props.waterlogged = waterlogged;
    props.facing = facing;
    world.set_block_state(
        pos,
        props.to_state_id(&Block::BIG_DRIPLEAF_STEM),
        BlockFlags::NOTIFY_ALL,
    );
}
