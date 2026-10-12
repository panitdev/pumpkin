use pumpkin_data::Block;
use pumpkin_data::BlockStateId;
use pumpkin_data::fluid::Fluid;
use pumpkin_data::tag::{self, Taggable};
use pumpkin_macros::pumpkin_block;
use pumpkin_util::math::position::BlockPos;
use pumpkin_world::world::BlockFlags;

use crate::block::{
    BlockBehaviour, CanPlaceAtArgs, GetStateForNeighborUpdateArgs, OnEntityCollisionArgs,
};
use crate::entity::vehicle::boat::BoatEntity;
use crate::world::World;

#[pumpkin_block("minecraft:lily_pad")]
pub struct LilyPadBlock;

impl BlockBehaviour for LilyPadBlock {
    fn on_entity_collision(&self, args: OnEntityCollisionArgs<'_>) {
        if args
            .entity
            .cast_any()
            .downcast_ref::<BoatEntity>()
            .is_some()
        {
            args.world
                .break_block(args.position, None, BlockFlags::NOTIFY_ALL);
        }
    }

    fn can_place_at(&self, args: CanPlaceAtArgs<'_>) -> bool {
        args.world
            .is_some_and(|world| can_place_at(world, args.position))
    }

    fn get_state_for_neighbor_update(
        &self,
        args: GetStateForNeighborUpdateArgs<'_>,
    ) -> BlockStateId {
        if !can_place_at(args.world, args.position) {
            return Block::AIR.default_state.id;
        }
        args.state_id
    }
}

fn can_place_at(world: &World, block_pos: &BlockPos) -> bool {
    let below = block_pos.down();
    let below_fluid = world.get_fluid(&below);
    let above_fluid = world.get_fluid(block_pos);
    (below_fluid.has_tag(&tag::Fluid::MINECRAFT_SUPPORTS_LILY_PAD)
        || world
            .get_block(&below)
            .has_tag(&tag::Block::MINECRAFT_SUPPORTS_LILY_PAD))
        && above_fluid.matches_type(&Fluid::EMPTY)
}
