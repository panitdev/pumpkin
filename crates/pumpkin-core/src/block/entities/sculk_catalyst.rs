use super::BlockEntity;
use crate::block::blocks::sculk::world_level::WorldSculkLevel;
use crate::world::World;
use pumpkin_data::{
    Block, BlockDirection, block_properties::SculkCatalystLikeProperties, particle::Particle,
    sound::Sound, sound::SoundCategory,
};
use pumpkin_nbt::{compound::NbtCompound, tag::NbtTag};
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};
use pumpkin_util::random::{RandomGenerator, xoroshiro128::Xoroshiro};
use pumpkin_world::generation::feature::features::sculk::spreader::{
    ChargeCursor, MAX_CHARGE, SculkSpreader,
};
use pumpkin_world::tick::TickPriority;
use pumpkin_world::world::BlockFlags;
use rand::RngExt;
use std::sync::{Arc, Mutex, PoisonError};

/// How far from the catalyst a dying mob still feeds it.
const LISTENER_RADIUS: f64 = 8.0;
/// Ticks the bloom state lasts before the block settles back.
const PULSE_TICKS: u8 = 8;

pub struct SculkCatalystBlockEntity {
    pub position: BlockPos,
    pub spreader: Mutex<SculkSpreader>,
}

const fn direction_name(direction: BlockDirection) -> &'static str {
    match direction {
        BlockDirection::Down => "down",
        BlockDirection::Up => "up",
        BlockDirection::North => "north",
        BlockDirection::South => "south",
        BlockDirection::West => "west",
        BlockDirection::East => "east",
    }
}

fn direction_from_name(name: &str) -> Option<BlockDirection> {
    BlockDirection::all()
        .into_iter()
        .find(|direction| direction_name(*direction) == name)
}

fn cursor_to_nbt(cursor: &ChargeCursor) -> NbtCompound {
    let mut nbt = NbtCompound::new();
    nbt.put(
        "pos",
        NbtTag::IntArray(vec![cursor.pos.0.x, cursor.pos.0.y, cursor.pos.0.z]),
    );
    nbt.put_int("charge", i32::from(cursor.charge));
    nbt.put_int("decay_delay", i32::from(cursor.decay_delay));
    nbt.put_int("update_delay", i32::from(cursor.update_delay));
    if cursor.faces.is_some() {
        let facings = cursor
            .facing_directions()
            .map(|direction| NbtTag::String(direction_name(direction).into()))
            .collect();
        nbt.put_list("facings", facings);
    }
    nbt
}

fn cursor_from_nbt(nbt: &NbtCompound) -> Option<ChargeCursor> {
    let pos = nbt.get_int_array("pos")?;
    let [x, y, z] = pos else {
        return None;
    };
    let mut cursor = ChargeCursor::new(
        BlockPos::new(*x, *y, *z),
        nbt.get_int("charge")
            .unwrap_or(0)
            .clamp(0, MAX_CHARGE.into()) as u16,
    );
    cursor.decay_delay = nbt.get_int("decay_delay").unwrap_or(1).clamp(0, 1) as u8;
    cursor.update_delay = nbt.get_int("update_delay").unwrap_or(0).clamp(0, 255) as u8;
    if let Some(facings) = nbt.get_list("facings") {
        let mut bits = 0u8;
        for direction in facings
            .iter()
            .filter_map(|tag| tag.extract_string())
            .filter_map(direction_from_name)
        {
            bits |= 1 << direction.to_index();
        }
        cursor.faces = Some(bits);
    }
    Some(cursor)
}

impl BlockEntity for SculkCatalystBlockEntity {
    fn resource_location(&self) -> &'static str {
        Self::ID
    }

    fn get_position(&self) -> BlockPos {
        self.position
    }

    fn from_nbt(nbt: &pumpkin_nbt::compound::NbtCompound, position: BlockPos) -> Self
    where
        Self: Sized,
    {
        let entity = Self::new(position);
        if let Some(cursors) = nbt.get_list("cursors") {
            let mut spreader = entity
                .spreader
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            for cursor in cursors
                .iter()
                .filter_map(|tag| tag.extract_compound())
                .filter_map(cursor_from_nbt)
            {
                spreader.add_cursor(cursor);
            }
        }
        entity
    }

    fn write_nbt(&self, nbt: &mut NbtCompound) {
        let spreader = self.spreader.lock().unwrap_or_else(PoisonError::into_inner);
        let cursors = spreader
            .cursors()
            .iter()
            .map(|cursor| NbtTag::Compound(cursor_to_nbt(cursor)))
            .collect();
        nbt.put_list("cursors", cursors);
    }

    fn chunk_data_nbt(&self) -> Option<NbtCompound> {
        // The client does not need the spreader state.
        Some(NbtCompound::new())
    }

    fn tick(&self, world: &Arc<World>) {
        let mut spreader = self.spreader.lock().unwrap_or_else(PoisonError::into_inner);
        if spreader.cursors().is_empty() {
            return;
        }
        let mut level = WorldSculkLevel::new(world);
        let mut random = RandomGenerator::Xoroshiro(Xoroshiro::from_seed(rand::rng().random()));
        spreader.update_cursors(&mut level, self.position, &mut random, true);
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl SculkCatalystBlockEntity {
    pub const ID: &'static str = "minecraft:sculk_catalyst";
    #[must_use]
    pub const fn new(position: BlockPos) -> Self {
        Self {
            position,
            spreader: Mutex::new(SculkSpreader::new_level_spreader()),
        }
    }

    /// Finds the catalyst closest to `pos` that is within listening range, like the nearest
    /// `BY_DISTANCE` game event listener would be.
    #[must_use]
    pub fn find_near(world: &World, pos: Vector3<f64>) -> Option<Arc<dyn BlockEntity>> {
        let radius = LISTENER_RADIUS as i32;
        let min_chunk_x = (pos.x.floor() as i32 - radius) >> 4;
        let max_chunk_x = (pos.x.floor() as i32 + radius) >> 4;
        let min_chunk_z = (pos.z.floor() as i32 - radius) >> 4;
        let max_chunk_z = (pos.z.floor() as i32 + radius) >> 4;

        let mut best: Option<(f64, Arc<dyn BlockEntity>)> = None;
        for chunk_x in min_chunk_x..=max_chunk_x {
            for chunk_z in min_chunk_z..=max_chunk_z {
                let Some(block_entities) = world.block_entities.get(&(chunk_x, chunk_z).into())
                else {
                    continue;
                };
                for block_entity in block_entities.values() {
                    if block_entity.as_any().downcast_ref::<Self>().is_none() {
                        continue;
                    }
                    let center = block_entity.get_position().to_centered_f64();
                    let distance = center.squared_distance_to_vec(&pos);
                    if distance <= LISTENER_RADIUS * LISTENER_RADIUS
                        && best.as_ref().is_none_or(|(d, _)| distance < *d)
                    {
                        best = Some((distance, block_entity.clone()));
                    }
                }
            }
        }
        best.map(|(_, block_entity)| block_entity)
    }

    /// A mob died in range: turn its experience into sculk charge and make the block bloom.
    pub fn on_entity_die(&self, world: &Arc<World>, death_pos: Vector3<f64>, experience: u32) {
        if experience > 0 {
            let start = BlockPos::floored(death_pos.x, death_pos.y + 0.5, death_pos.z);
            self.spreader
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .add_cursors(start, experience.min(i32::MAX as u32) as i32);
        }
        self.bloom(world);
    }

    fn bloom(&self, world: &Arc<World>) {
        let (block, state) = world.get_block_and_state(&self.position);
        if block != &Block::SCULK_CATALYST {
            return;
        }
        let mut props = SculkCatalystLikeProperties::from_state_id(state.id);
        props.bloom = true;
        world.set_block_state(
            &self.position,
            props.to_state_id(block),
            BlockFlags::NOTIFY_ALL,
        );
        world.schedule_block_tick(block, self.position, PULSE_TICKS, TickPriority::Normal);

        let center = self.position.to_centered_f64();
        world.spawn_particles(
            Particle::SculkSoul,
            Vector3::new(center.x, f64::from(self.position.0.y) + 1.15, center.z),
            2,
            Vector3::new(0.2, 0.0, 0.2),
            0.0,
        );
        world.play_sound_fine(
            Sound::BlockSculkCatalystBloom,
            SoundCategory::Blocks,
            &center,
            2.0,
            0.6 + rand::random::<f32>() * 0.4,
        );
    }
}
