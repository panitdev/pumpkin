use std::any::Any;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};

use pumpkin_data::Block;
use pumpkin_data::block_properties::JukeboxLikeProperties;
use pumpkin_data::data_component_impl::{BlockEntityDataImpl, JukeboxPlayableImpl};
use pumpkin_data::entity::EntityType;
use pumpkin_data::game_event::GameEvent;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::jukebox_song::JukeboxSong;
use pumpkin_data::particle::Particle;
use pumpkin_data::world::WorldEvent;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_nbt::tag::NbtTag;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use pumpkin_world::world::BlockFlags;
use rand::{RngExt, rng};

use crate::block::entities::BlockEntity;
use crate::entity::Entity;
use crate::entity::item::ItemEntity;
use crate::world::World;
use pumpkin_inventory::{Clearable, Inventory};

/// `JukeboxSongPlayer.PLAY_EVENT_INTERVAL_TICKS`
const PLAY_EVENT_INTERVAL_TICKS: u64 = 20;
/// `JukeboxSong.SONG_END_PADDING_TICKS`
const SONG_END_PADDING_TICKS: u64 = 20;

/// `ServerLevel.sendParticles` range
const PARTICLE_VIEW_DISTANCE: f64 = 32.0;

const RECORD_ITEM_NBT_KEY: &str = "RecordItem";
const TICKS_SINCE_SONG_STARTED_NBT_KEY: &str = "ticks_since_song_started";

/// `JukeboxSong.hasFinished`
const fn has_finished(song: JukeboxSong, ticks_elapsed: u64) -> bool {
    ticks_elapsed >= song.length_in_ticks() + SONG_END_PADDING_TICKS
}

/// State of vanilla's `JukeboxSongPlayer`.
#[derive(Clone, Copy, Default)]
struct SongPlayer {
    song: Option<JukeboxSong>,
    ticks_since_song_started: u64,
    /// Length requested through the plugin API; replaces the registry length when set.
    length_override: Option<u64>,
}

impl SongPlayer {
    fn has_finished(&self, song: JukeboxSong) -> bool {
        self.length_override.map_or_else(
            || has_finished(song, self.ticks_since_song_started),
            |length| self.ticks_since_song_started >= length,
        )
    }
}

/// Matches vanilla's `JukeboxBlockEntity`
pub struct JukeboxBlockEntity {
    position: BlockPos,
    record_stack: Mutex<ItemStack>,
    song_player: Mutex<SongPlayer>,
    /// World this block entity was last ticked or placed in; inventory and plugin calls have no
    /// world argument.
    world: Mutex<Weak<World>>,
    /// The item was changed through the inventory before a world was known; the next tick runs
    /// the world side of `setTheItem` for it.
    item_changed_externally: AtomicBool,
    dirty: AtomicBool,
    comparator_dirty: AtomicBool,
}

impl BlockEntity for JukeboxBlockEntity {
    fn resource_location(&self) -> &'static str {
        Self::ID
    }

    fn get_position(&self) -> BlockPos {
        self.position
    }

    fn from_nbt(nbt: &NbtCompound, position: BlockPos) -> Self
    where
        Self: Sized,
    {
        let record_stack = nbt
            .get_compound(RECORD_ITEM_NBT_KEY)
            .and_then(ItemStack::read_item_stack)
            .unwrap_or_else(|| ItemStack::EMPTY.clone());

        let mut song_player = SongPlayer::default();
        if let Some(ticks) = nbt.get_long(TICKS_SINCE_SONG_STARTED_NBT_KEY)
            && let Some(song) = Self::song_from_stack(&record_stack)
        {
            let ticks = ticks.max(0) as u64;
            if !has_finished(song, ticks) {
                song_player = SongPlayer {
                    song: Some(song),
                    ticks_since_song_started: ticks,
                    length_override: None,
                };
            }
        }

        let entity = Self::new(position);
        *entity.lock_record() = record_stack;
        *entity.lock_song_player() = song_player;
        entity
    }

    fn write_nbt(&self, nbt: &mut NbtCompound) {
        let record = self.lock_record();
        if !record.is_empty() {
            let mut record_nbt = NbtCompound::new();
            record.write_item_stack(&mut record_nbt);
            nbt.put(RECORD_ITEM_NBT_KEY, record_nbt);
        }
        drop(record);

        let song_player = *self.lock_song_player();
        if song_player.song.is_some() {
            nbt.put_long(
                TICKS_SINCE_SONG_STARTED_NBT_KEY,
                song_player.ticks_since_song_started as i64,
            );
        }
    }

    /// `JukeboxBlockEntity.tick` -> `JukeboxSongPlayer.tick`
    fn tick(&self, world: &Arc<World>) {
        self.bind_world(world);
        if self.item_changed_externally.swap(false, Ordering::Relaxed) {
            self.on_item_set(world);
        }

        let mut song_player = self.lock_song_player();
        let Some(song) = song_player.song else {
            return;
        };
        if song_player.has_finished(song) {
            drop(song_player);
            self.stop(world);
            return;
        }
        let emit_play_event = song_player
            .ticks_since_song_started
            .is_multiple_of(PLAY_EVENT_INTERVAL_TICKS);
        song_player.ticks_since_song_started += 1;
        drop(song_player);

        if emit_play_event {
            world.emit_game_event(
                GameEvent::JukeboxPlay.name(),
                self.position.to_centered_f64(),
            );
            self.spawn_music_particles(world);
        }
    }

    fn is_comparator_dirty(&self) -> bool {
        self.comparator_dirty.load(Ordering::Relaxed)
    }

    fn clear_comparator_dirty(&self) {
        self.comparator_dirty.store(false, Ordering::Relaxed);
    }

    fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::Relaxed)
    }

    fn clear_dirty(&self) {
        self.dirty.store(false, Ordering::Relaxed);
    }

    fn chunk_data_nbt(&self) -> Option<NbtCompound> {
        let mut nbt = NbtCompound::new();
        let record = self.lock_record();
        if !record.is_empty() {
            let mut record_nbt = NbtCompound::new();
            record.write_item_stack(&mut record_nbt);
            nbt.put(RECORD_ITEM_NBT_KEY, NbtTag::Compound(record_nbt));
        }
        Some(nbt)
    }

    /// `JukeboxBlockEntity.preRemoveSideEffects` + `setRemoved`
    fn on_block_replaced(self: Arc<Self>, world: &Arc<World>, _position: &BlockPos) {
        self.pop_out_the_item(world);
        world.emit_game_event(
            GameEvent::JukeboxStopPlay.name(),
            self.position.to_centered_f64(),
        );
        world.sync_world_event(WorldEvent::SoundStopJukeboxSong, self.position, 0);
    }

    /// Block entity data carried by the item (`RecordItem`), read before `setPlacedBy`.
    fn apply_item_components(&self, stack: &ItemStack) {
        if let Some(data) = stack.get_data_component::<BlockEntityDataImpl>()
            && let Some(record) = data
                .nbt
                .get_compound(RECORD_ITEM_NBT_KEY)
                .and_then(ItemStack::read_item_stack)
        {
            *self.lock_record() = record;
            self.mark_changed();
        }
    }

    fn bedrock_block_actor_data(
        &self,
        _state_id: pumpkin_data::BlockStateId,
    ) -> Option<NbtCompound> {
        let ticks = self.lock_song_player().ticks_since_song_started;
        Some(crate::block::entities::bedrock::build_jukebox_actor(
            self.position,
            Some(&self.lock_record()),
            ticks,
        ))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn get_inventory(self: Arc<Self>) -> Option<Arc<dyn Inventory>> {
        Some(self)
    }
}

impl JukeboxBlockEntity {
    pub const ID: &'static str = "minecraft:jukebox";

    #[must_use]
    pub fn new(position: BlockPos) -> Self {
        Self {
            position,
            record_stack: Mutex::new(ItemStack::EMPTY.clone()),
            song_player: Mutex::new(SongPlayer::default()),
            world: Mutex::new(Weak::new()),
            item_changed_externally: AtomicBool::new(false),
            dirty: AtomicBool::new(false),
            comparator_dirty: AtomicBool::new(false),
        }
    }

    fn lock_record(&self) -> MutexGuard<'_, ItemStack> {
        self.record_stack
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn lock_song_player(&self) -> MutexGuard<'_, SongPlayer> {
        self.song_player
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// `JukeboxSong.fromStack`
    #[must_use]
    pub fn song_from_stack(stack: &ItemStack) -> Option<JukeboxSong> {
        let playable = stack.get_data_component::<JukeboxPlayableImpl>()?;
        let name = playable.song.rsplit(':').next()?;
        JukeboxSong::from_name(name)
    }

    #[must_use]
    pub fn get_record(&self) -> ItemStack {
        self.lock_record().clone()
    }

    /// `JukeboxSongPlayer.isPlaying`
    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.lock_song_player().song.is_some()
    }

    pub fn bind_world(&self, world: &Arc<World>) {
        *self.world.lock().unwrap_or_else(PoisonError::into_inner) = Arc::downgrade(world);
    }

    fn bound_world(&self) -> Option<Arc<World>> {
        self.world
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .upgrade()
    }

    /// Plugin API: stops the current song.
    pub fn stop_playing(&self) {
        if let Some(world) = self.bound_world() {
            self.stop(&world);
        } else {
            *self.lock_song_player() = SongPlayer::default();
        }
    }

    /// Plugin API: plays the stored record's song for `length_in_ticks`. Does nothing when the
    /// stored item is not a playable disc.
    pub fn start_playing(&self, length_in_ticks: u64) {
        let Some(song) = Self::song_from_stack(&self.get_record()) else {
            return;
        };
        if let Some(world) = self.bound_world() {
            self.play_with_length(&world, song, Some(length_in_ticks));
        } else {
            *self.lock_song_player() = SongPlayer {
                song: Some(song),
                ticks_since_song_started: 0,
                length_override: Some(length_in_ticks),
            };
        }
    }

    /// Runs the world side of an inventory change, or defers it to the next tick.
    fn on_inventory_changed(&self) {
        self.mark_changed();
        if let Some(world) = self.bound_world() {
            self.on_item_set(&world);
        } else {
            self.item_changed_externally.store(true, Ordering::Relaxed);
        }
    }

    /// `JukeboxBlockEntity.getComparatorOutput`
    #[must_use]
    pub fn get_comparator_output(&self) -> u8 {
        Self::song_from_stack(&self.lock_record()).map_or(0, |song| song.comparator_output())
    }

    /// `JukeboxBlockEntity.setTheItem`
    pub fn set_the_item(&self, world: &Arc<World>, stack: ItemStack) {
        let previous = std::mem::replace(&mut *self.lock_record(), stack);
        if !previous.is_empty() {
            let pos = self.position.to_centered_f64();
            world.scatter_stack(pos.x, pos.y, pos.z, previous);
        }
        self.item_changed_externally.store(false, Ordering::Relaxed);
        self.mark_changed();
        self.on_item_set(world);
    }

    /// World side of `setTheItem`: block state, game event and the song player.
    fn on_item_set(&self, world: &Arc<World>) {
        let stack = self.get_record();
        let item_was_inserted = !stack.is_empty();
        self.notify_item_changed_in_jukebox(world, item_was_inserted);
        match Self::song_from_stack(&stack) {
            Some(song) if item_was_inserted => self.play(world, song),
            _ => self.stop(world),
        }
    }

    /// `JukeboxBlockEntity.notifyItemChangedInJukebox`
    fn notify_item_changed_in_jukebox(&self, world: &Arc<World>, was_inserted: bool) {
        let block = world.get_block(&self.position);
        if block != &Block::JUKEBOX {
            return;
        }
        let new_state = JukeboxLikeProperties {
            has_record: was_inserted,
        };
        world.set_block_state(
            &self.position,
            new_state.to_state_id(block),
            BlockFlags::NOTIFY_LISTENERS,
        );
        world.emit_game_event(
            GameEvent::BlockChange.name(),
            self.position.to_centered_f64(),
        );
    }

    /// `JukeboxBlockEntity.popOutTheItem`
    pub fn pop_out_the_item(&self, world: &Arc<World>) {
        let item = std::mem::replace(&mut *self.lock_record(), ItemStack::EMPTY.clone());
        if item.is_empty() {
            return;
        }
        self.item_changed_externally.store(false, Ordering::Relaxed);
        self.mark_changed();
        self.on_item_set(world);

        // Vec3.atLowerCornerWithOffset(pos, 0.5, 1.01, 0.5).offsetRandomXZ(random, 0.7F)
        let mut random = rng();
        let spawn_pos = Vector3::new(
            f64::from(self.position.0.x) + 0.5 + random.random_range(-0.35..0.35),
            f64::from(self.position.0.y) + 1.01,
            f64::from(self.position.0.z) + 0.5 + random.random_range(-0.35..0.35),
        );
        let entity = Entity::new(world.clone(), spawn_pos, &EntityType::ITEM);
        let item_entity = Arc::new(ItemEntity::new(entity, item));
        world.spawn_entity(item_entity);
        self.on_song_changed(world);
    }

    /// `JukeboxSongPlayer.play`
    fn play(&self, world: &Arc<World>, song: JukeboxSong) {
        self.play_with_length(world, song, None);
    }

    fn play_with_length(&self, world: &Arc<World>, song: JukeboxSong, length: Option<u64>) {
        *self.lock_song_player() = SongPlayer {
            song: Some(song),
            ticks_since_song_started: 0,
            length_override: length,
        };
        world.sync_world_event(
            WorldEvent::SoundPlayJukeboxSong,
            self.position,
            song.get_id() as i32,
        );
        self.on_song_changed(world);
    }

    /// `JukeboxSongPlayer.stop`
    fn stop(&self, world: &Arc<World>) {
        let mut song_player = self.lock_song_player();
        if song_player.song.is_none() {
            return;
        }
        *song_player = SongPlayer::default();
        drop(song_player);
        world.emit_game_event(
            GameEvent::JukeboxStopPlay.name(),
            self.position.to_centered_f64(),
        );
        world.sync_world_event(WorldEvent::SoundStopJukeboxSong, self.position, 0);
        self.on_song_changed(world);
    }

    /// `JukeboxBlockEntity.onSongChanged`
    fn on_song_changed(&self, world: &Arc<World>) {
        world.update_neighbors_at(&self.position, &Block::JUKEBOX, None);
        self.mark_changed();
    }

    /// `JukeboxSongPlayer.spawnMusicParticles`
    fn spawn_music_particles(&self, world: &Arc<World>) {
        let pos = Vector3::new(
            f64::from(self.position.0.x) + 0.5,
            f64::from(self.position.0.y) + f64::from(1.2f32),
            f64::from(self.position.0.z) + 0.5,
        );
        let random_color = f32::from(rng().random_range(0..4u8)) / 24.0;
        // ServerLevel.sendParticles only reaches players within 32 blocks.
        for player in world.get_nearby_players(pos, PARTICLE_VIEW_DISTANCE) {
            player.spawn_particles(
                Particle::Note,
                pos,
                0,
                Vector3::new(random_color, 0.0, 0.0),
                1.0,
            );
        }
    }

    fn mark_changed(&self) {
        self.dirty.store(true, Ordering::Relaxed);
        self.comparator_dirty.store(true, Ordering::Relaxed);
    }
}

/// Single-slot inventory (vanilla `ContainerSingleItem`).
impl Inventory for JukeboxBlockEntity {
    fn size(&self) -> usize {
        1
    }

    fn is_empty(&self) -> bool {
        self.lock_record().is_empty()
    }

    fn get_stack(&self, _slot: usize) -> ItemStack {
        self.get_record()
    }

    fn remove_stack(&self, _slot: usize) -> ItemStack {
        let taken = std::mem::replace(&mut *self.lock_record(), ItemStack::EMPTY.clone());
        if !taken.is_empty() {
            self.on_inventory_changed();
        }
        taken
    }

    fn remove_stack_specific(&self, _slot: usize, _amount: u8) -> ItemStack {
        self.remove_stack(0)
    }

    fn set_stack(&self, _slot: usize, stack: ItemStack) {
        *self.lock_record() = stack;
        self.on_inventory_changed();
    }

    fn get_max_count_per_stack(&self) -> u8 {
        1
    }

    fn mark_dirty(&self) {
        self.mark_changed();
    }

    /// `JukeboxBlockEntity.canPlaceItem`
    fn is_valid_slot_for(&self, slot: usize, stack: &ItemStack) -> bool {
        stack.get_data_component::<JukeboxPlayableImpl>().is_some()
            && self.get_stack(slot).is_empty()
    }

    /// `JukeboxBlockEntity.canTakeItem`
    fn can_transfer_to(
        &self,
        hopper_inventory: &dyn Inventory,
        _slot: usize,
        _stack: &ItemStack,
    ) -> bool {
        (0..hopper_inventory.size()).any(|slot| hopper_inventory.get_stack(slot).is_empty())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl Clearable for JukeboxBlockEntity {
    fn clear(&self) {
        *self.lock_record() = ItemStack::EMPTY.clone();
        self.on_inventory_changed();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{Entity, ai::test_support::world, passive::allay::AllayEntity};
    use pumpkin_data::entity::EntityType;
    use pumpkin_util::math::{vector2::Vector2, vector3::Vector3};

    #[tokio::test]
    async fn removing_the_record_through_inventory_stops_allay_dancing()
    -> Result<(), Box<dyn std::error::Error>> {
        let folder = tempfile::tempdir()?;
        let world = world(&folder);
        world.level.loaded_chunks.insert(
            Vector2::new(0, 0),
            pumpkin_world::chunk::ChunkData::empty_sync(0, 0),
        );
        let allay = AllayEntity::new(Entity::new(
            world.clone(),
            Vector3::new(1.5, 64.0, 0.5),
            &EntityType::ALLAY,
        ));
        world.add_entity_silent(allay.clone());
        let jukebox = JukeboxBlockEntity::new(BlockPos::new(0, 64, 0));
        jukebox.set_stack(
            0,
            ItemStack::new(1, &pumpkin_data::item::Item::MUSIC_DISC_CAT),
        );
        jukebox.tick(&world);
        assert!(allay.is_dancing());
        jukebox.remove_stack(0);
        assert!(!allay.is_dancing());
        world.level.shutdown().await;
        Ok(())
    }
}
