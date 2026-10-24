use std::sync::Arc;

use pumpkin_data::block_properties::{Axis, PaleOakWoodLikeProperties};
use pumpkin_data::data_component_impl::EnchantmentsImpl;
use pumpkin_data::entity::EntityType;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::tag::{Enchantment as EnchantmentTag, Taggable};
use pumpkin_data::{Block, BlockState, BlockStateId, Rotation};
use pumpkin_macros::pumpkin_block;
use pumpkin_util::GameMode;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use uuid::Uuid;

use crate::block::BlockBehaviour;
use crate::block::BrokenArgs;
use crate::block::OnPlaceArgs;
use crate::block::SpawnAfterBreakArgs;
use crate::entity::r#type::from_type;
use crate::world::World;

#[pumpkin_block(
    "minecraft:infested_stone",
    "minecraft:infested_cobblestone",
    "minecraft:infested_stone_bricks",
    "minecraft:infested_mossy_stone_bricks",
    "minecraft:infested_cracked_stone_bricks",
    "minecraft:infested_chiseled_stone_bricks",
    "minecraft:infested_deepslate"
)]
pub struct InfestedBlock;

impl InfestedBlock {
    /// Vanilla `spawnInfestation`: a silverfish at the block centre.
    fn spawn_infestation(world: &Arc<World>, position: &BlockPos) {
        let pos = position.0;
        let spawn_pos = Vector3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y),
            f64::from(pos.z) + 0.5,
        );
        let silverfish = from_type(&EntityType::SILVERFISH, spawn_pos, world, Uuid::new_v4());
        world.spawn_entity(silverfish);
    }
}

impl BlockBehaviour for InfestedBlock {
    fn on_place(&self, args: OnPlaceArgs<'_>) -> BlockStateId {
        if args.block == &Block::INFESTED_DEEPSLATE {
            let mut props = PaleOakWoodLikeProperties::default(args.block);
            props.axis = args.direction.to_axis();
            props.to_state_id(args.block)
        } else {
            args.block.default_state.id
        }
    }

    fn rotate(
        &self,
        block: &Block,
        state_id: BlockStateId,
        rotation: Rotation,
    ) -> &'static BlockState {
        if block == &Block::INFESTED_DEEPSLATE {
            let mut props = PaleOakWoodLikeProperties::from_state_id(state_id);
            if matches!(
                rotation,
                Rotation::Clockwise90 | Rotation::CounterClockwise90
            ) {
                props.axis = match props.axis {
                    Axis::X => Axis::Z,
                    Axis::Z => Axis::X,
                    Axis::Y => Axis::Y,
                };
            }
            BlockState::from_id(props.to_state_id(block))
        } else {
            BlockState::from_id(state_id)
        }
    }

    fn broken(&self, args: BrokenArgs<'_>) {
        // Vanilla `spawnAfterBreak` runs from `playerDestroy`, which is skipped in creative
        // (`preventsBlockDrops`) and when the tool cannot harvest the block.
        if args.player.gamemode.load() == GameMode::Creative
            || !args
                .player
                .can_harvest(args.state, Block::from_state_id(args.state.id))
            || !args.world.level_info.load().game_rules.block_drops
        {
            return;
        }

        let tool = args.player.inventory().held_item();
        if prevents_infested_spawns(&tool) {
            return;
        }

        Self::spawn_infestation(args.world, args.position);
    }

    fn spawn_after_break(&self, args: SpawnAfterBreakArgs<'_>) {
        // Vanilla `onExplosionHit` calls `spawnAfterBreak` with an empty tool, so explosions
        // always spawn the silverfish (when block drops are enabled).
        if args.world.level_info.load().game_rules.block_drops {
            Self::spawn_infestation(args.world, args.position);
        }
    }
}

/// Vanilla `EnchantmentHelper.hasTag(tool, EnchantmentTags.PREVENTS_INFESTED_SPAWNS)`.
fn prevents_infested_spawns(tool: &ItemStack) -> bool {
    tool.get_data_component::<EnchantmentsImpl>()
        .is_some_and(|enchantments| {
            enchantments.enchantment.iter().any(|(enchantment, _)| {
                enchantment.has_tag(&EnchantmentTag::MINECRAFT_PREVENTS_INFESTED_SPAWNS)
            })
        })
}
