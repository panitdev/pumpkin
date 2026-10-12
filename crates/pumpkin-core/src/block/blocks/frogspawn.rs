use std::sync::Arc;
use std::sync::atomic::Ordering;

use pumpkin_data::Block;
use pumpkin_data::BlockStateId;
use pumpkin_data::entity::EntityType;
use pumpkin_data::fluid::Fluid;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::{Block as BlockTag, Fluid as FluidTag, Taggable};
use pumpkin_macros::pumpkin_block;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_world::tick::TickPriority;
use pumpkin_world::world::BlockFlags;
use uuid::Uuid;

use crate::block::{
    BlockBehaviour, CanPlaceAtArgs, GetStateForNeighborUpdateArgs, OnEntityCollisionArgs,
    OnScheduledTickArgs, PlacedArgs,
};
use crate::entity::r#type::from_type;
use crate::world::World;

#[pumpkin_block("minecraft:frogspawn")]
pub struct FrogspawnBlock;

impl FrogspawnBlock {
    const MIN_TADPOLES_SPAWN: u32 = 2;
    const MAX_TADPOLES_SPAWN: u32 = 5;
    const DEFAULT_MIN_HATCH_TICK_DELAY: u32 = 3600;
    const DEFAULT_MAX_HATCH_TICK_DELAY: u32 = 12000;

    /// Vanilla `FrogspawnBlock.mayPlaceOn`, called with the block below.
    fn may_place_on(world: &World, pos: &BlockPos) -> bool {
        let fluid = world.get_fluid(pos);
        let block = world.get_block(pos);
        let fluid_above = world.get_fluid(&pos.up());
        (fluid.has_tag(&FluidTag::MINECRAFT_SUPPORTS_FROGSPAWN)
            || block.has_tag(&BlockTag::MINECRAFT_SUPPORTS_FROGSPAWN))
            && fluid_above.id == Fluid::EMPTY.id
    }

    fn can_survive(world: &World, pos: &BlockPos) -> bool {
        Self::may_place_on(world, &pos.down())
    }

    fn get_hatch_delay() -> u32 {
        rand::random_range(Self::DEFAULT_MIN_HATCH_TICK_DELAY..Self::DEFAULT_MAX_HATCH_TICK_DELAY)
    }

    fn hatch_frogspawn(world: &Arc<World>, pos: &BlockPos) {
        world.break_block(pos, None, BlockFlags::SKIP_DROPS);
        world.play_sound(
            Sound::BlockFrogspawnHatch,
            SoundCategory::Blocks,
            &pos.to_f64(),
        );
        Self::spawn_tadpoles(world, pos);
    }

    fn spawn_tadpoles(world: &Arc<World>, pos: &BlockPos) {
        let amount = rand::random_range(Self::MIN_TADPOLES_SPAWN..=Self::MAX_TADPOLES_SPAWN);
        for _ in 0..amount {
            let x = f64::from(pos.0.x) + Self::random_tadpole_position_offset();
            let z = f64::from(pos.0.z) + Self::random_tadpole_position_offset();
            let y = f64::from(pos.0.y) - 0.5;
            let yaw = rand::random_range(1..=360) as f32;
            let tadpole = from_type(
                &EntityType::TADPOLE,
                Vector3::new(x, y, z),
                world,
                Uuid::new_v4(),
            );
            tadpole.get_entity().set_rotation(yaw, 0.0);
            if let Some(mob) = tadpole.get_mob() {
                mob.get_mob_entity()
                    .persistence_required
                    .store(true, Ordering::Relaxed);
            }
            world.spawn_entity(tadpole);
        }
    }

    fn random_tadpole_position_offset() -> f64 {
        rand::random::<f64>().clamp(0.2, 0.799_999_997_019_767_8)
    }
}

impl BlockBehaviour for FrogspawnBlock {
    fn can_place_at(&self, args: CanPlaceAtArgs<'_>) -> bool {
        args.world
            .is_none_or(|world| Self::can_survive(world, args.position))
    }

    fn get_state_for_neighbor_update(
        &self,
        args: GetStateForNeighborUpdateArgs<'_>,
    ) -> BlockStateId {
        if Self::can_survive(args.world, args.position) {
            args.state_id
        } else {
            Block::AIR.default_state.id
        }
    }

    fn placed(&self, args: PlacedArgs<'_>) {
        args.world.schedule_block_tick_long(
            args.block,
            *args.position,
            Self::get_hatch_delay(),
            TickPriority::Normal,
        );
    }

    fn on_scheduled_tick(&self, args: OnScheduledTickArgs<'_>) {
        if Self::can_survive(args.world, args.position) {
            Self::hatch_frogspawn(args.world, args.position);
        } else {
            args.world
                .break_block(args.position, None, BlockFlags::SKIP_DROPS);
        }
    }

    fn on_entity_collision(&self, args: OnEntityCollisionArgs<'_>) {
        if args.entity.get_entity().entity_type.id == EntityType::FALLING_BLOCK.id {
            args.world
                .break_block(args.position, None, BlockFlags::SKIP_DROPS);
        }
    }
}
