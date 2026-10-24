use std::sync::Arc;

use crate::block::blocks::weathering_copper::{WeatherState, get_weather_state};
use crate::block::entities::BlockEntity;
use crate::block::entities::chest::ChestBlockEntity;
use pumpkin_data::BlockStateId;
use pumpkin_data::HorizontalFacingExt;
use pumpkin_data::block_properties::{ChestLikeProperties, ChestType, HorizontalFacing};
use pumpkin_data::entity::EntityPose;
use pumpkin_data::fluid::Fluid;
use pumpkin_data::loot_table::get_loot_table;
use pumpkin_data::sound::Sound;
use pumpkin_data::tag::Taggable;
use pumpkin_data::{Block, BlockDirection, translation};
use pumpkin_inventory::Inventory;
use pumpkin_inventory::double::DoubleInventory;
use pumpkin_inventory::generic_container_screen_handler::{create_generic_9x3, create_generic_9x6};
use pumpkin_inventory::player::player_inventory::PlayerInventory;
use pumpkin_inventory::screen_handler::{
    InventoryPlayer, ScreenHandlerFactory, SharedScreenHandler,
};
use pumpkin_macros::{pumpkin_block, pumpkin_block_from_tag};
use pumpkin_util::GameMode;
use pumpkin_util::math::boundingbox::BoundingBox;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_util::text::TextComponent;
use pumpkin_world::tick::TickPriority;
use pumpkin_world::world::BlockFlags;
use std::sync::Mutex;

use crate::block::{
    BlockBehaviour, BrokenArgs, EmitsRedstonePowerArgs, GetComparatorOutputArgs,
    GetRedstonePowerArgs, GetScreenHandlerFactoryArgs, GetStateForNeighborUpdateArgs,
    NormalUseArgs, OnPlaceArgs, OnSyncedBlockEventArgs, PathComputationType, PlacedArgs,
    RandomTickArgs, registry::BlockActionResult,
};
use crate::entity::EntityBase;
use crate::entity::passive::cat::CatEntity;
use crate::entity::passive::tamable::TamableAnimal;
use crate::entity::player::Player;
use crate::world::World;
use crate::world::loot::{LootContextParameters, fill_chest_inventory_handle};
use pumpkin_data::BlockState;

struct ChestScreenFactory {
    inventory: Arc<dyn Inventory>,
    loot_pending: bool,
}

impl ScreenHandlerFactory for ChestScreenFactory {
    fn create_screen_handler(
        &self,
        sync_id: u8,
        player_inventory: &Arc<PlayerInventory>,
        player: &dyn InventoryPlayer,
    ) -> Option<SharedScreenHandler> {
        if self.loot_pending && player.is_spectator() {
            return None;
        }

        let concrete_handler = if self.inventory.size() > 27 {
            create_generic_9x6(sync_id, player_inventory, self.inventory.clone(), player)
        } else {
            create_generic_9x3(sync_id, player_inventory, self.inventory.clone(), player)
        };

        let concrete_arc = Arc::new(Mutex::new(concrete_handler));

        Some(concrete_arc as SharedScreenHandler)
    }

    fn get_display_name(&self) -> TextComponent {
        if self.inventory.size() > 27 {
            pumpkin_macros::translate_cross!(
                translation::java::CONTAINER_CHESTDOUBLE,
                translation::bedrock::CONTAINER_CHESTDOUBLE
            )
        } else {
            pumpkin_macros::translate_cross!(
                translation::java::CONTAINER_CHEST,
                translation::bedrock::CONTAINER_CHEST
            )
        }
    }
}

// Shared chest behavior implementations
const LID_ANIMATION_EVENT_TYPE: u8 = 1;

fn on_place_chest_impl(args: &OnPlaceArgs<'_>) -> BlockStateId {
    let mut chest_props = ChestLikeProperties::default(args.block);
    chest_props.waterlogged = args.replacing.water_source();

    let (r#type, facing) = compute_chest_props(
        args.world,
        args.player,
        args.block,
        args.position,
        args.direction,
    );
    chest_props.facing = facing;
    chest_props.r#type = r#type;

    let block = least_oxidized_connected_block(
        args.world,
        Block::from_state_id(args.block.default_state.id),
        args.position,
        chest_props,
    );
    chest_props.to_state_id(block)
}

fn placed_chest_impl<E: BlockEntity + 'static>(
    args: &PlacedArgs<'_>,
    create_entity: impl FnOnce(BlockPos) -> E,
) {
    if args.world.get_block_entity(args.position).is_none() {
        let chest = create_entity(*args.position);
        args.world.add_block_entity(Arc::new(chest));
    }

    let chest_props = ChestLikeProperties::from_state_id(args.state_id);
    let connected_towards = match chest_props.r#type {
        ChestType::Single => return,
        ChestType::Left => chest_props.facing.rotate_clockwise(),
        ChestType::Right => chest_props.facing.rotate_counter_clockwise(),
    };

    // The partner takes over the placed block, so a copper pair always shares one weathering stage.
    let placed_block = Block::from_state_id(args.state_id);
    if let Some((_, mut neighbor_props)) = get_chest_properties_if_can_connect(
        args.world,
        placed_block,
        args.position,
        chest_props.facing,
        connected_towards,
        ChestType::Single,
    ) {
        neighbor_props.r#type = chest_props.r#type.opposite();

        args.world.set_block_state(
            &args.position.offset(connected_towards.to_offset()),
            neighbor_props.to_state_id(placed_block),
            BlockFlags::NOTIFY_LISTENERS,
        );
    }
}

/// Computes the comparator output for a chest, combining both halves for double chests.
fn get_chest_comparator_output(args: &GetComparatorOutputArgs<'_>) -> Option<u8> {
    let state = args.world.get_block_state_id(args.position);
    let first_chest = args.world.get_block_entity(args.position);
    let first_inventory = first_chest.and_then(BlockEntity::get_inventory)?;

    let chest_props = ChestLikeProperties::from_state_id(state);
    let connected_towards = match chest_props.r#type {
        ChestType::Single => None,
        ChestType::Left => Some(chest_props.facing.rotate_clockwise()),
        ChestType::Right => Some(chest_props.facing.rotate_counter_clockwise()),
    };

    // Vanilla passes `ignoreBeingBlocked = false`, so a blocked half reads zero.
    if is_chest_blocked(args.world, args.position) {
        return Some(0);
    }

    if let Some(direction) = connected_towards
        && is_chest_blocked(args.world, &args.position.offset(direction.to_offset()))
    {
        return Some(0);
    }

    if let Some(direction) = connected_towards
        && let Some(second_inventory) = args
            .world
            .get_block_entity(&args.position.offset(direction.to_offset()))
            .and_then(BlockEntity::get_inventory)
    {
        let double_inventory = if matches!(chest_props.r#type, ChestType::Right) {
            DoubleInventory::new(first_inventory, second_inventory)
        } else {
            DoubleInventory::new(second_inventory, first_inventory)
        };
        Some(crate::block::calculate_comparator_output(
            double_inventory.as_ref(),
        ))
    } else {
        Some(crate::block::calculate_comparator_output(
            first_inventory.as_ref(),
        ))
    }
}

/// Returns a screen handler factory for opening a chest or double chest inventory.
///
/// Unpacks deferred loot tables on first open (for non-spectator players) and combines
/// connected inventories for double chests if neither chest half is obstructed.
fn get_chest_screen_handler_factory(
    args: GetScreenHandlerFactoryArgs<'_>,
) -> Option<Box<dyn ScreenHandlerFactory>> {
    let state = args.world.get_block_state_id(args.position);
    let first_chest = args.world.get_block_entity(args.position);

    let player_is_spectator = args.player.gamemode.load() == GameMode::Spectator;

    let chest_props = ChestLikeProperties::from_state_id(state);
    let connected_towards = match chest_props.r#type {
        ChestType::Single => None,
        ChestType::Left => Some(chest_props.facing.rotate_clockwise()),
        ChestType::Right => Some(chest_props.facing.rotate_counter_clockwise()),
    };

    let unpack = |entity: &Arc<dyn BlockEntity>| {
        if let Some((loot_key, seed)) = entity.take_loot_table()
            && let Some(table) = get_loot_table(&loot_key)
            && let Some(inv) = entity.clone().get_inventory()
        {
            fill_chest_inventory_handle(
                &inv,
                &crate::world::loot::LootTableHandle::Static(table),
                seed,
                &LootContextParameters {
                    world: Some(args.world),
                    position: Some(args.position.to_f64()),
                    this_entity: Some(args.player.as_ref()),
                    luck: args
                        .player
                        .living_entity
                        .get_attribute_value(&pumpkin_data::attributes::Attributes::LUCK)
                        as f32,
                    ..Default::default()
                },
            );
            inv.mark_dirty();
        }
    };

    // Unpack deferred loot table on first open (non-spectator only).
    if !player_is_spectator && let Some(ref entity) = first_chest {
        unpack(entity);
    }

    let first_loot_pending = first_chest
        .as_ref()
        .is_some_and(|entity| entity.has_loot_table());
    let first_inventory = first_chest.and_then(BlockEntity::get_inventory)?;

    if is_chest_blocked(args.world, args.position) {
        return None;
    }

    if let Some(direction) = connected_towards {
        let neighbor_pos = args.position.offset(direction.to_offset());
        if is_chest_blocked(args.world, &neighbor_pos) {
            return None;
        }
    }

    // Both halves of a double chest are unpacked at once, like vanilla's CompoundContainer.
    if !player_is_spectator
        && let Some(direction) = connected_towards
        && let Some(second) = args
            .world
            .get_block_entity(&args.position.offset(direction.to_offset()))
    {
        unpack(&second);
    }

    let loot_pending = first_loot_pending
        || connected_towards.is_some_and(|direction| {
            args.world
                .get_block_entity(&args.position.offset(direction.to_offset()))
                .is_some_and(|second| second.has_loot_table())
        });

    let inventory = if let Some(direction) = connected_towards
        && let Some(second_inventory) = args
            .world
            .get_block_entity(&args.position.offset(direction.to_offset()))
            .and_then(BlockEntity::get_inventory)
    {
        // Vanilla: chestType == ChestType.RIGHT ? DoubleBlockProperties.Type.FIRST : DoubleBlockProperties.Type.SECOND;
        if matches!(chest_props.r#type, ChestType::Right) {
            DoubleInventory::new(first_inventory, second_inventory)
        } else {
            DoubleInventory::new(second_inventory, first_inventory)
        }
    } else {
        first_inventory
    };

    Some(Box::new(ChestScreenFactory {
        inventory,
        loot_pending,
    }))
}

fn normal_use_chest_impl(args: &NormalUseArgs<'_>) -> BlockActionResult {
    let stat = if args.block.id == Block::TRAPPED_CHEST.id {
        pumpkin_data::statistic::CustomStatistic::TriggerTrappedChest
    } else {
        pumpkin_data::statistic::CustomStatistic::OpenChest
    };
    args.player.increment_stat(
        pumpkin_data::statistic::StatisticCategory::Custom,
        stat as i32,
        1,
    );

    if let Some(factory) = get_chest_screen_handler_factory(GetScreenHandlerFactoryArgs {
        server: args.server,
        world: args.world,
        block: args.block,
        position: args.position,
        player: args.player,
    }) {
        args.player
            .open_handled_screen(factory.as_ref(), Some(*args.position));
    }

    BlockActionResult::Success
}

fn broken_chest_impl(args: &BrokenArgs<'_>) {
    let chest_props = ChestLikeProperties::from_state_id(args.state.id);
    let connected_towards = match chest_props.r#type {
        ChestType::Single => return,
        ChestType::Left => chest_props.facing.rotate_clockwise(),
        ChestType::Right => chest_props.facing.rotate_counter_clockwise(),
    };

    if let Some((neighbor_block, mut neighbor_props)) = get_chest_properties_if_can_connect(
        args.world,
        args.block,
        args.position,
        chest_props.facing,
        connected_towards,
        chest_props.r#type.opposite(),
    ) {
        neighbor_props.r#type = ChestType::Single;

        args.world.set_block_state(
            &args.position.offset(connected_towards.to_offset()),
            neighbor_props.to_state_id(neighbor_block),
            BlockFlags::NOTIFY_LISTENERS,
        );
    }
}

/// Vanilla `updateShape`: keeps the water ticking and lets a copper chest adopt the block of its
/// connected partner (so waxing or weathering one half changes both).
fn chest_state_for_neighbor_update(args: &GetStateForNeighborUpdateArgs<'_>) -> BlockStateId {
    let props = ChestLikeProperties::from_state_id(args.state_id);
    if props.waterlogged {
        args.world.schedule_fluid_tick(
            &Fluid::WATER,
            *args.position,
            Fluid::WATER.flow_speed as u8,
            TickPriority::Normal,
        );
    }

    if is_copper_chest(args.block) && props.r#type != ChestType::Single {
        let connected_dir = match props.r#type {
            ChestType::Left => props.facing.rotate_clockwise(),
            _ => props.facing.rotate_counter_clockwise(),
        };
        let neighbor = Block::from_state_id(args.neighbor_state_id);
        if args.direction == connected_dir.to_block_direction()
            && is_copper_chest(neighbor)
            && neighbor != args.block
        {
            return props.to_state_id(neighbor);
        }
    }
    args.state_id
}

#[pumpkin_block_from_tag("c:chests/wooden")]
pub struct ChestBlock;

impl BlockBehaviour for ChestBlock {
    fn on_place(&self, args: OnPlaceArgs<'_>) -> BlockStateId {
        on_place_chest_impl(&args)
    }

    fn on_synced_block_event(&self, args: OnSyncedBlockEventArgs<'_>) -> bool {
        args.r#type == LID_ANIMATION_EVENT_TYPE
    }

    fn placed(&self, args: PlacedArgs<'_>) {
        placed_chest_impl(&args, ChestBlockEntity::new);
    }

    fn normal_use(&self, args: NormalUseArgs<'_>) -> BlockActionResult {
        normal_use_chest_impl(&args)
    }

    fn get_screen_handler_factory(
        &self,
        args: GetScreenHandlerFactoryArgs<'_>,
    ) -> Option<Box<dyn ScreenHandlerFactory>> {
        get_chest_screen_handler_factory(args)
    }

    fn get_state_for_neighbor_update(
        &self,
        args: GetStateForNeighborUpdateArgs<'_>,
    ) -> BlockStateId {
        chest_state_for_neighbor_update(&args)
    }

    fn broken(&self, args: BrokenArgs<'_>) {
        broken_chest_impl(&args);
    }

    fn get_comparator_output(&self, args: GetComparatorOutputArgs<'_>) -> Option<u8> {
        get_chest_comparator_output(&args)
    }

    fn is_pathfindable(&self, _state: &BlockState, _computation_type: PathComputationType) -> bool {
        false
    }
}

/// Copper chests have the same behavior as wooden chests but also oxidize over time.
#[pumpkin_block_from_tag("minecraft:copper_chests")]
pub struct CopperChestBlock;

impl
    crate::block::blocks::weathering_copper::ChangeOverTimeBlock<
        crate::block::blocks::weathering_copper::WeatherState,
    > for CopperChestBlock
{
    fn get_age(
        &self,
        block: &Block,
    ) -> Option<crate::block::blocks::weathering_copper::WeatherState> {
        crate::block::blocks::weathering_copper::get_weather_state(block)
    }

    fn get_chance_modifier(
        &self,
        age: crate::block::blocks::weathering_copper::WeatherState,
    ) -> f32 {
        crate::block::blocks::weathering_copper::get_chance_modifier(age)
    }

    fn get_next(&self, block: &Block) -> Option<&'static Block> {
        crate::block::blocks::weathering_copper::get_next(block)
    }

    fn get_previous(&self, block: &Block) -> Option<&'static Block> {
        crate::block::blocks::weathering_copper::get_previous(block)
    }

    fn get_first(&self, block: &Block) -> Option<&'static Block> {
        crate::block::blocks::weathering_copper::get_first(block)
    }
}

impl crate::block::blocks::weathering_copper::WeatheringCopper for CopperChestBlock {}

impl BlockBehaviour for CopperChestBlock {
    fn on_place(&self, args: OnPlaceArgs<'_>) -> BlockStateId {
        on_place_chest_impl(&args)
    }

    fn on_synced_block_event(&self, args: OnSyncedBlockEventArgs<'_>) -> bool {
        args.r#type == LID_ANIMATION_EVENT_TYPE
    }

    fn placed(&self, args: PlacedArgs<'_>) {
        placed_chest_impl(&args, ChestBlockEntity::new);
    }

    fn normal_use(&self, args: NormalUseArgs<'_>) -> BlockActionResult {
        normal_use_chest_impl(&args)
    }

    fn get_screen_handler_factory(
        &self,
        args: GetScreenHandlerFactoryArgs<'_>,
    ) -> Option<Box<dyn ScreenHandlerFactory>> {
        get_chest_screen_handler_factory(args)
    }

    fn get_state_for_neighbor_update(
        &self,
        args: GetStateForNeighborUpdateArgs<'_>,
    ) -> BlockStateId {
        chest_state_for_neighbor_update(&args)
    }

    fn broken(&self, args: BrokenArgs<'_>) {
        broken_chest_impl(&args);
    }

    fn random_tick(&self, args: RandomTickArgs<'_>) {
        let current_state_id = args.world.get_block_state_id(args.position);
        let chest_props = ChestLikeProperties::from_state_id(current_state_id);

        // Only oxidize LEFT or SINGLE chests (not RIGHT) to prevent double oxidation
        if chest_props.r#type == ChestType::Right {
            return;
        }

        // Only oxidize if no players are viewing the chest
        if let Some(block_entity) = args.world.get_block_entity(args.position)
            && let Some(chest_entity) = block_entity.as_any().downcast_ref::<ChestBlockEntity>()
            && chest_entity.get_viewer_count() > 0
        {
            return;
        }

        crate::block::blocks::weathering_copper::change_over_time(
            args.world,
            args.position,
            args.block,
        );
    }

    fn get_comparator_output(&self, args: GetComparatorOutputArgs<'_>) -> Option<u8> {
        get_chest_comparator_output(&args)
    }

    fn is_pathfindable(&self, _state: &BlockState, _computation_type: PathComputationType) -> bool {
        false
    }
}

/// Trapped chests have the same behavior as wooden chests but also emit redstone power based on viewer count.
#[pumpkin_block("minecraft:trapped_chest")]
pub struct TrappedChestBlock;

impl BlockBehaviour for TrappedChestBlock {
    fn on_place(&self, args: OnPlaceArgs<'_>) -> BlockStateId {
        on_place_chest_impl(&args)
    }

    fn on_synced_block_event(&self, args: OnSyncedBlockEventArgs<'_>) -> bool {
        args.r#type == LID_ANIMATION_EVENT_TYPE
    }

    fn placed(&self, args: PlacedArgs<'_>) {
        use crate::block::entities::trapped_chest::TrappedChestBlockEntity;
        placed_chest_impl(&args, TrappedChestBlockEntity::new);
    }

    fn normal_use(&self, args: NormalUseArgs<'_>) -> BlockActionResult {
        normal_use_chest_impl(&args)
    }

    fn get_screen_handler_factory(
        &self,
        args: GetScreenHandlerFactoryArgs<'_>,
    ) -> Option<Box<dyn ScreenHandlerFactory>> {
        get_chest_screen_handler_factory(args)
    }

    fn get_state_for_neighbor_update(
        &self,
        args: GetStateForNeighborUpdateArgs<'_>,
    ) -> BlockStateId {
        chest_state_for_neighbor_update(&args)
    }

    fn broken(&self, args: BrokenArgs<'_>) {
        broken_chest_impl(&args);
    }

    fn emits_redstone_power(&self, _args: EmitsRedstonePowerArgs<'_>) -> bool {
        true
    }

    fn get_weak_redstone_power(&self, args: GetRedstonePowerArgs<'_>) -> u8 {
        use crate::block::entities::trapped_chest::TrappedChestBlockEntity;

        // Get viewer count from this chest
        let viewer_count = if let Some(block_entity) = args.world.get_block_entity(args.position)
            && let Some(trapped_chest) = block_entity
                .as_any()
                .downcast_ref::<TrappedChestBlockEntity>()
        {
            trapped_chest.get_viewer_count()
        } else {
            0
        };

        viewer_count.min(15) as u8
    }

    fn get_strong_redstone_power(&self, args: GetRedstonePowerArgs<'_>) -> u8 {
        // Strong power emitted to the block beneath the trapped chest
        // The block below queries with direction Up (from below looking up at the chest)
        if args.direction == BlockDirection::Up {
            self.get_weak_redstone_power(args)
        } else {
            0
        }
    }

    fn get_comparator_output(&self, args: GetComparatorOutputArgs<'_>) -> Option<u8> {
        get_chest_comparator_output(&args)
    }

    fn is_pathfindable(&self, _state: &BlockState, _computation_type: PathComputationType) -> bool {
        false
    }
}

fn compute_chest_props(
    world: &World,
    player: &Player,
    block: &Block,
    block_pos: &BlockPos,
    face: BlockDirection,
) -> (ChestType, HorizontalFacing) {
    let player_facing = player.get_entity().get_horizontal_facing();
    let chest_facing = player_facing.opposite();

    if player.get_entity().pose.load() == EntityPose::Crouching {
        let Some(face) = face.to_horizontal_facing() else {
            return (ChestType::Single, chest_facing);
        };

        let (clicked_block, clicked_block_state) =
            world.get_block_and_state_id(&block_pos.offset(face.to_offset()));

        if chest_can_connect_to(block, clicked_block) {
            let clicked_props = ChestLikeProperties::from_state_id(clicked_block_state);

            if clicked_props.r#type != ChestType::Single {
                return (ChestType::Single, chest_facing);
            }

            if clicked_props.facing.rotate_clockwise() == face {
                return (ChestType::Left, clicked_props.facing);
            } else if clicked_props.facing.rotate_counter_clockwise() == face {
                return (ChestType::Right, clicked_props.facing);
            }
        }

        return (ChestType::Single, chest_facing);
    }

    if get_chest_properties_if_can_connect(
        world,
        block,
        block_pos,
        chest_facing,
        chest_facing.rotate_clockwise(),
        ChestType::Single,
    )
    .is_some()
    {
        (ChestType::Left, chest_facing)
    } else if get_chest_properties_if_can_connect(
        world,
        block,
        block_pos,
        chest_facing,
        chest_facing.rotate_counter_clockwise(),
        ChestType::Single,
    )
    .is_some()
    {
        (ChestType::Right, chest_facing)
    } else {
        (ChestType::Single, chest_facing)
    }
}

/// Whether `neighbor` may form a double chest with a chest of kind `block`. Copper chests connect
/// across weathering stages and waxing, every other chest only with its own block.
fn chest_can_connect_to(block: &Block, neighbor: &Block) -> bool {
    neighbor == block || (is_copper_chest(block) && is_copper_chest(neighbor))
}

fn is_copper_chest(block: &Block) -> bool {
    block.has_tag(&pumpkin_data::tag::Block::MINECRAFT_COPPER_CHESTS)
}

const WAXED_COPPER_CHESTS: [(&Block, &Block); 4] = [
    (&Block::WAXED_COPPER_CHEST, &Block::COPPER_CHEST),
    (
        &Block::WAXED_EXPOSED_COPPER_CHEST,
        &Block::EXPOSED_COPPER_CHEST,
    ),
    (
        &Block::WAXED_WEATHERED_COPPER_CHEST,
        &Block::WEATHERED_COPPER_CHEST,
    ),
    (
        &Block::WAXED_OXIDIZED_COPPER_CHEST,
        &Block::OXIDIZED_COPPER_CHEST,
    ),
];

/// The unwaxed counterpart of a waxed copper chest, or the block itself otherwise.
fn unwaxed_copper_chest(block: &'static Block) -> &'static Block {
    WAXED_COPPER_CHESTS
        .iter()
        .find(|(waxed, _)| *waxed == block)
        .map_or(block, |(_, unwaxed)| unwaxed)
}

/// Weathering stage of a copper chest, waxed or not.
fn copper_chest_weather_state(block: &Block) -> Option<WeatherState> {
    get_weather_state(block).or_else(|| {
        WAXED_COPPER_CHESTS
            .iter()
            .find(|(waxed, _)| *waxed == block)
            .and_then(|(_, unwaxed)| get_weather_state(unwaxed))
    })
}

/// When a copper chest is placed as one half of a double chest, both halves adopt the least
/// oxidized block of the pair. If only one half is waxed, the wax is dropped.
fn least_oxidized_connected_block(
    world: &World,
    block: &'static Block,
    pos: &BlockPos,
    props: ChestLikeProperties,
) -> &'static Block {
    let connected_dir = match props.r#type {
        ChestType::Single => return block,
        ChestType::Left => props.facing.rotate_clockwise(),
        ChestType::Right => props.facing.rotate_counter_clockwise(),
    };
    let connected = world.get_block(&pos.offset(connected_dir.to_offset()));
    let (Some(own_state), Some(connected_state)) = (
        copper_chest_weather_state(block),
        copper_chest_weather_state(connected),
    ) else {
        return block;
    };

    let is_waxed = |b: &Block| get_weather_state(b).is_none();
    let (own, other) = if is_waxed(block) == is_waxed(connected) {
        (block, connected)
    } else {
        (unwaxed_copper_chest(block), unwaxed_copper_chest(connected))
    };
    if own_state <= connected_state {
        own
    } else {
        other
    }
}

fn get_chest_properties_if_can_connect(
    world: &World,
    block: &Block,
    block_pos: &BlockPos,
    facing: HorizontalFacing,
    direction: HorizontalFacing,
    wanted_type: ChestType,
) -> Option<(&'static Block, ChestLikeProperties)> {
    let (neighbor_block, neighbor_block_state) =
        world.get_block_and_state_id(&block_pos.offset(direction.to_offset()));

    if !chest_can_connect_to(block, neighbor_block) {
        return None;
    }

    let neighbor_props = ChestLikeProperties::from_state_id(neighbor_block_state);
    if neighbor_props.facing == facing && neighbor_props.r#type == wanted_type {
        return Some((neighbor_block, neighbor_props));
    }

    None
}

/// A chest cannot be opened while a redstone-conductive block or a sitting cat is on top of it.
fn is_chest_blocked(world: &World, block_pos: &BlockPos) -> bool {
    has_block_on_top(world, block_pos) || is_cat_sitting_on_chest(world, block_pos)
}

fn has_block_on_top(world: &World, block_pos: &BlockPos) -> bool {
    let above_pos = block_pos.up();
    let above_state = world.get_block_state(&above_pos);
    above_state.is_solid_block()
}

fn is_cat_sitting_on_chest(world: &World, block_pos: &BlockPos) -> bool {
    let x = f64::from(block_pos.0.x);
    let y = f64::from(block_pos.0.y);
    let z = f64::from(block_pos.0.z);
    let area = BoundingBox::new(
        Vector3::new(x, y + 1.0, z),
        Vector3::new(x + 1.0, y + 2.0, z + 1.0),
    );
    world.get_entities_at_box(&area).iter().any(|entity| {
        entity
            .cast_any()
            .downcast_ref::<CatEntity>()
            .is_some_and(TamableAnimal::is_in_sitting_pose)
    })
}

/// Sound a chest hinge makes. Copper chests use the sounds of their weathering stage.
#[must_use]
pub fn chest_hinge_sound(state_id: BlockStateId, open: bool) -> Sound {
    match (
        copper_chest_weather_state(Block::from_state_id(state_id)),
        open,
    ) {
        (None, true) => Sound::BlockChestOpen,
        (None, false) => Sound::BlockChestClose,
        (Some(WeatherState::Weathered), true) => Sound::BlockCopperChestWeatheredOpen,
        (Some(WeatherState::Weathered), false) => Sound::BlockCopperChestWeatheredClose,
        (Some(WeatherState::Oxidized), true) => Sound::BlockCopperChestOxidizedOpen,
        (Some(WeatherState::Oxidized), false) => Sound::BlockCopperChestOxidizedClose,
        (Some(_), true) => Sound::BlockCopperChestOpen,
        (Some(_), false) => Sound::BlockCopperChestClose,
    }
}

trait ChestTypeExt {
    fn opposite(&self) -> ChestType;
}

impl ChestTypeExt for ChestType {
    fn opposite(&self) -> Self {
        match self {
            Self::Single => Self::Single,
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }
}
