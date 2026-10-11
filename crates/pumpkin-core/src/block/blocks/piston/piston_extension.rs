use pumpkin_data::{Block, BlockState, FacingExt};
use pumpkin_macros::pumpkin_block;
use pumpkin_world::world::BlockFlags;

use crate::block::registry::BlockActionResult;
use crate::block::{BlockBehaviour, BrokenArgs, NormalUseArgs, PathComputationType};

use super::piston::PistonProps;

pub(crate) type MovingPistonProps = pumpkin_data::block_properties::MovingPistonLikeProperties;

#[pumpkin_block("minecraft:moving_piston")]
pub struct PistonExtensionBlock;

impl BlockBehaviour for PistonExtensionBlock {
    fn broken(&self, args: BrokenArgs<'_>) {
        {
            let props = MovingPistonProps::from_state_id(args.state.id);
            let pos = args
                .position
                .offset(props.facing.opposite().to_block_direction().to_offset());
            let (new_block, new_state) = args.world.get_block_and_state_id(&pos);
            if &Block::PISTON == new_block || &Block::STICKY_PISTON == new_block {
                let props = PistonProps::from_state_id(new_state);
                if props.extended {
                    // Vanilla `removeBlock(pos, false)`: no drops, no effects.
                    args.world.set_block_state(
                        &pos,
                        Block::AIR.default_state.id,
                        BlockFlags::NOTIFY_ALL,
                    );
                }
            }
        }
    }

    /// A moving piston without its block entity is a leftover (e.g. from `/setblock`); using it
    /// clears it away.
    fn normal_use(&self, args: NormalUseArgs<'_>) -> BlockActionResult {
        if args.world.get_block_entity(args.position).is_none() {
            args.world.set_block_state(
                args.position,
                Block::AIR.default_state.id,
                BlockFlags::NOTIFY_ALL,
            );
            return BlockActionResult::Consume;
        }
        BlockActionResult::Pass
    }

    fn is_pathfindable(&self, _state: &BlockState, _computation_type: PathComputationType) -> bool {
        false
    }
}
