use std::sync::Arc;

use crate::block::{
    BlockBehaviour, CanPlaceAtArgs, GetStateForNeighborUpdateArgs, OnLandedUponArgs, OnPlaceArgs,
    OnScheduledTickArgs, PathComputationType, RandomTickArgs,
};
use crate::world::World;
use pumpkin_data::block_properties::FarmlandLikeProperties;
use pumpkin_data::fluid::Fluid;
use pumpkin_data::tag;
use pumpkin_data::tag::Taggable;
use pumpkin_data::{Block, BlockDirection, BlockState, BlockStateId};
use pumpkin_macros::pumpkin_block;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_world::tick::TickPriority;
use pumpkin_world::world::BlockAccessor;
use pumpkin_world::world::BlockFlags;

type FarmlandProperties = FarmlandLikeProperties;

#[pumpkin_block("minecraft:farmland")]
pub struct FarmlandBlock;

impl BlockBehaviour for FarmlandBlock {
    fn on_scheduled_tick(&self, args: OnScheduledTickArgs<'_>) {
        if !can_place_at(&**args.world, args.position) {
            turn_to_dirt(args.world, args.position);
        }
    }

    fn on_place(&self, args: OnPlaceArgs<'_>) -> BlockStateId {
        if !can_place_at(args.world, args.position) {
            return Block::DIRT.default_state.id;
        }
        args.block.default_state.id
    }

    fn get_state_for_neighbor_update(
        &self,
        args: GetStateForNeighborUpdateArgs<'_>,
    ) -> BlockStateId {
        if args.direction == BlockDirection::Up && !can_place_at(args.world, args.position) {
            args.world
                .schedule_block_tick(args.block, *args.position, 1, TickPriority::Normal);
        }
        args.state_id
    }

    fn can_place_at(&self, args: CanPlaceAtArgs<'_>) -> bool {
        can_place_at(args.block_accessor, args.position)
    }

    fn on_landed_upon(&self, args: OnLandedUponArgs<'_>) {
        if let Some(living) = args.entity.get_living_entity() {
            let entity = args.entity.get_entity();
            if rand::random::<f32>() < args.fall_distance - 0.5 {
                let can_trample = args.entity.get_player().map_or_else(
                    || args.world.level_info.load().game_rules.mob_griefing,
                    |player| !args.world.is_in_spawn_protection(player, args.position),
                );
                let dims = entity.entity_dimension.load();
                if can_trample && dims.width * dims.width * dims.height > 0.512 {
                    turn_to_dirt(args.world, args.position);
                }
            }
            living.handle_fall_damage(args.entity, args.fall_distance, 1.0);
        }
    }

    fn random_tick(&self, args: RandomTickArgs<'_>) {
        let state_id = args.world.get_block_state_id(args.position);
        let props = FarmlandProperties::from_state_id(state_id);
        let moisture = i32::from(props.moisture);
        if is_water_nearby(args.world, args.position)
            || args.world.is_raining_at(&args.position.up())
        {
            if moisture < 7 {
                set_moisture(&args, props, 7);
            }
        } else if moisture > 0 {
            set_moisture(&args, props, moisture - 1);
        } else if !should_maintain_farmland(args.world.as_ref(), args.position) {
            turn_to_dirt(args.world, args.position);
        }
    }

    fn is_pathfindable(&self, _state: &BlockState, _computation_type: PathComputationType) -> bool {
        false
    }
}

/// Fires the moisture event (plugins may veto or change the value) and applies the result.
fn set_moisture(args: &RandomTickArgs<'_>, mut props: FarmlandProperties, mut new_moisture: i32) {
    if let Some(server) = args.world.server.upgrade() {
        let mut event =
            crate::plugin::api::events::block::moisture_change::MoistureChangeEvent::new(
                *args.position,
                args.world.clone(),
                new_moisture,
            );
        server.plugin_manager.fire_blocking(&server, &mut event);
        if event.cancelled {
            return;
        }
        new_moisture = event.new_moisture;
    }
    props.moisture = new_moisture.clamp(0, 7) as u8;
    args.world.set_block_state(
        args.position,
        props.to_state_id(args.block),
        BlockFlags::NOTIFY_LISTENERS,
    );
}

fn should_maintain_farmland(world: &dyn BlockAccessor, block_pos: &BlockPos) -> bool {
    world
        .get_block(&block_pos.up())
        .has_tag(&tag::Block::MINECRAFT_MAINTAINS_FARMLAND)
}

fn can_place_at(world: &dyn BlockAccessor, block_pos: &BlockPos) -> bool {
    let state = world.get_block_state(&block_pos.up());
    !state.is_solid() || should_maintain_farmland(world, block_pos)
}

/// Replaces the farmland with dirt, lifting entities that were standing in the lost top slice.
fn turn_to_dirt(world: &Arc<World>, block_pos: &BlockPos) {
    crate::block::push_entities_up(world, block_pos);
    world.set_block_state(
        block_pos,
        Block::DIRT.default_state.id,
        BlockFlags::NOTIFY_ALL,
    );
}

fn is_water_nearby(world: &Arc<World>, block_pos: &BlockPos) -> bool {
    for dx in -4..=4 {
        for dy in 0..=1 {
            for dz in -4..=4 {
                let check_pos = block_pos.offset(Vector3 {
                    x: dx,
                    y: dy,
                    z: dz,
                });
                if Fluid::WATER.matches_type(world.get_fluid(&check_pos)) {
                    return true;
                }
            }
        }
    }
    false
}
