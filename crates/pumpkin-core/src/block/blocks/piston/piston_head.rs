use pumpkin_data::block_properties::PistonType;
use pumpkin_data::{Block, BlockState, BlockStateId, FacingExt};
use pumpkin_macros::pumpkin_block;
use pumpkin_util::GameMode;
use pumpkin_util::math::position::BlockPos;
use pumpkin_world::world::BlockFlags;

use crate::block::blocks::piston::piston_extension::MovingPistonProps;
use crate::block::{
    BlockBehaviour, BrokenArgs, GetStateForNeighborUpdateArgs, OnNeighborUpdateArgs,
    PathComputationType,
};
use crate::world::World;

use super::piston::PistonProps;

pub(crate) type PistonHeadProperties = pumpkin_data::block_properties::PistonHeadLikeProperties;

#[pumpkin_block("minecraft:piston_head")]
pub struct PistonHeadBlock;

impl PistonHeadBlock {
    fn base_pos(head: PistonHeadProperties, pos: &BlockPos) -> BlockPos {
        pos.offset(head.facing.opposite().to_block_direction().to_offset())
    }

    /// An extended piston of the head's type, facing the same way.
    fn is_fitting_base(head: PistonHeadProperties, base_state: BlockStateId) -> bool {
        let base_block = if head.r#type == PistonType::Sticky {
            &Block::STICKY_PISTON
        } else {
            &Block::PISTON
        };
        if base_state.to_block() != base_block {
            return false;
        }
        let base = PistonProps::from_state_id(base_state);
        base.extended && base.facing == head.facing
    }

    fn can_survive(world: &World, head_state: BlockStateId, pos: &BlockPos) -> bool {
        let head = PistonHeadProperties::from_state_id(head_state);
        let base_state = world.get_block_state_id(&Self::base_pos(head, pos));
        Self::is_fitting_base(head, base_state)
            || (base_state.to_block() == &Block::MOVING_PISTON
                && MovingPistonProps::from_state_id(base_state).facing == head.facing)
    }
}

impl BlockBehaviour for PistonHeadBlock {
    fn broken(&self, args: BrokenArgs<'_>) {
        let head = PistonHeadProperties::from_state_id(args.state.id);
        let base_pos = Self::base_pos(head, args.position);
        if Self::is_fitting_base(head, args.world.get_block_state_id(&base_pos)) {
            let flags = if args.player.gamemode.load() == GameMode::Creative {
                BlockFlags::NOTIFY_ALL | BlockFlags::SKIP_DROPS
            } else {
                BlockFlags::NOTIFY_ALL
            };
            args.world.break_block(&base_pos, None, flags);
        }
    }

    fn on_neighbor_update(&self, args: OnNeighborUpdateArgs<'_>) {
        // Pass updates on to the base so it re-checks its power.
        let head_state = args.world.get_block_state_id(args.position);
        if Self::can_survive(args.world, head_state, args.position) {
            let head = PistonHeadProperties::from_state_id(head_state);
            args.world
                .update_neighbor(&Self::base_pos(head, args.position), args.source_block);
        }
    }

    fn get_state_for_neighbor_update(
        &self,
        args: GetStateForNeighborUpdateArgs<'_>,
    ) -> BlockStateId {
        let head = PistonHeadProperties::from_state_id(args.state_id);
        if args.direction.to_facing() == head.facing.opposite()
            && !Self::can_survive(args.world, args.state_id, args.position)
        {
            return Block::AIR.default_state.id;
        }
        args.state_id
    }

    fn is_pathfindable(&self, _state: &BlockState, _computation_type: PathComputationType) -> bool {
        false
    }
}
