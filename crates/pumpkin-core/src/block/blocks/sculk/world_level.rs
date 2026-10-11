use std::sync::Arc;

use pumpkin_data::{
    BlockDirection, BlockId, BlockState, BlockStateId, fluid::Fluid, sound::Sound,
    sound::SoundCategory,
};
use pumpkin_util::math::position::BlockPos;
use pumpkin_world::generation::feature::features::sculk::SculkLevel;
use pumpkin_world::world::BlockFlags;

use crate::world::World;

/// Lets the shared sculk spreader read and write a live world. Positions in unloaded chunks
/// read as `None`, so cursors treat them as solid and never walk into them.
pub struct WorldSculkLevel<'a> {
    world: &'a Arc<World>,
}

impl<'a> WorldSculkLevel<'a> {
    #[must_use]
    pub const fn new(world: &'a Arc<World>) -> Self {
        Self { world }
    }

    fn state(&self, pos: BlockPos) -> Option<&'static BlockState> {
        self.world
            .get_block_state_id_if_loaded(&pos)
            .map(BlockState::from_id)
    }
}

impl SculkLevel for WorldSculkLevel<'_> {
    fn sculk_get(&self, pos: BlockPos) -> Option<BlockStateId> {
        self.world.get_block_state_id_if_loaded(&pos)
    }

    fn sculk_set(&mut self, pos: BlockPos, state: &'static BlockState) {
        if !self.world.is_loaded(&pos) {
            return;
        }
        self.world
            .set_block_state(&pos, state.id, BlockFlags::NOTIFY_ALL);
        let sound = match state.id.to_block_id() {
            BlockId::SCULK | BlockId::SCULK_VEIN => Some(Sound::BlockSculkSpread),
            BlockId::SCULK_SENSOR => Some(Sound::BlockSculkSensorPlace),
            BlockId::SCULK_SHRIEKER => Some(Sound::BlockSculkShriekerPlace),
            _ => None,
        };
        if let Some(sound) = sound {
            self.world
                .play_sound(sound, SoundCategory::Blocks, &pos.to_centered_f64());
        }
    }

    fn sculk_is_air(&self, pos: BlockPos) -> bool {
        self.state(pos).is_some_and(BlockState::is_air)
    }

    fn sculk_is_water_source(&self, pos: BlockPos) -> bool {
        if !self.world.is_loaded(&pos) {
            return false;
        }
        let (fluid, fluid_state) = self.world.get_fluid_and_fluid_state(&pos);
        fluid == &Fluid::WATER && fluid_state.is_source
    }

    fn sculk_is_water(&self, pos: BlockPos) -> bool {
        self.world.is_loaded(&pos) && self.world.get_fluid(&pos) == &Fluid::WATER
    }

    fn sculk_is_face_sturdy(&self, pos: BlockPos, face: BlockDirection) -> bool {
        self.state(pos).is_some_and(|s| s.is_side_solid(face))
    }

    fn sculk_is_full_cube(&self, pos: BlockPos) -> bool {
        self.state(pos).is_some_and(BlockState::is_full_cube)
    }
}
