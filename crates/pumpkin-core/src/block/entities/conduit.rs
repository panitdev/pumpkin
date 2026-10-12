use super::BlockEntity;
use crate::world::World;
use pumpkin_data::damage::DamageType;
use pumpkin_data::effect::StatusEffect;
use pumpkin_data::entity::MobCategory;
use pumpkin_data::fluid::Fluid;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::boundingbox::BoundingBox;
use pumpkin_util::math::position::BlockPos;
use rand::RngExt;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use uuid::Uuid;

/// Frame blocks needed for the conduit to activate.
const MIN_ACTIVE_SIZE: usize = 16;
/// Frame blocks needed for the conduit to attack hostile mobs.
const MIN_KILL_SIZE: usize = 42;
const KILL_RANGE: f64 = 8.0;
const EFFECT_DURATION: i32 = 260;

pub struct ConduitBlockEntity {
    pub position: BlockPos,
    pub active: AtomicBool,
    pub hunting: AtomicBool,
    pub target: Mutex<Option<Uuid>>,
    effect_blocks: Mutex<Vec<BlockPos>>,
    next_ambient_sound: AtomicI64,
}

impl BlockEntity for ConduitBlockEntity {
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
        let entity = Self::new(position);
        *entity
            .target
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = nbt.get_uuid("Target");
        entity
    }

    fn write_nbt(&self, nbt: &mut NbtCompound) {
        if let Some(target) = *self
            .target
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
        {
            nbt.put_uuid("Target", target);
        }
    }

    fn tick(&self, world: &Arc<World>) {
        let game_time = world.get_world_age();
        if game_time % 40 == 0 {
            let active = self.update_shape(world);
            if active != self.active.load(Ordering::Relaxed) {
                let sound = if active {
                    Sound::BlockConduitActivate
                } else {
                    Sound::BlockConduitDeactivate
                };
                world.play_sound(sound, SoundCategory::Blocks, &self.center());
            }
            self.active.store(active, Ordering::Relaxed);
            let size = self.frame_size();
            self.hunting.store(size >= MIN_KILL_SIZE, Ordering::Relaxed);
            if active {
                self.apply_effects(world, size);
                self.update_and_attack_target(world, size >= MIN_KILL_SIZE);
            }
        }

        if self.active.load(Ordering::Relaxed) {
            if game_time % 80 == 0 {
                world.play_sound(
                    Sound::BlockConduitAmbient,
                    SoundCategory::Blocks,
                    &self.center(),
                );
            }
            if game_time > self.next_ambient_sound.load(Ordering::Relaxed) {
                self.next_ambient_sound.store(
                    game_time + 60 + rand::rng().random_range(0..40),
                    Ordering::Relaxed,
                );
                world.play_sound(
                    Sound::BlockConduitAmbientShort,
                    SoundCategory::Blocks,
                    &self.center(),
                );
            }
        }
    }

    fn chunk_data_nbt(&self) -> Option<NbtCompound> {
        let mut nbt = NbtCompound::new();
        if let Ok(target) = self.target.try_lock()
            && let Some(target) = *target
        {
            nbt.put_uuid("Target", target);
        }
        Some(nbt)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl ConduitBlockEntity {
    pub const ID: &'static str = "minecraft:conduit";
    #[must_use]
    pub const fn new(position: BlockPos) -> Self {
        Self {
            position,
            active: AtomicBool::new(false),
            hunting: AtomicBool::new(false),
            target: Mutex::new(None),
            effect_blocks: Mutex::new(Vec::new()),
            next_ambient_sound: AtomicI64::new(0),
        }
    }

    fn center(&self) -> pumpkin_util::math::vector3::Vector3<f64> {
        self.position.to_centered_f64()
    }

    fn frame_size(&self) -> usize {
        self.effect_blocks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }

    fn is_water_at(world: &World, pos: &BlockPos) -> bool {
        let (fluid, _) = world.get_fluid_and_fluid_state(pos);
        fluid.id == Fluid::WATER.id || fluid.id == Fluid::FLOWING_WATER.id
    }

    /// Rescans the 5x5x5 frame. Needs the surrounding 3x3x3 to be water; returns whether enough
    /// frame blocks were found to activate.
    fn update_shape(&self, world: &World) -> bool {
        let mut blocks = self
            .effect_blocks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        blocks.clear();
        let origin = self.position.0;

        for ox in -1..=1 {
            for oy in -1..=1 {
                for oz in -1..=1 {
                    let pos = BlockPos::new(origin.x + ox, origin.y + oy, origin.z + oz);
                    if !Self::is_water_at(world, &pos) {
                        return false;
                    }
                }
            }
        }

        // Only the edges and face centers of the outer shell count, not its corners.
        for ox in -2i32..=2 {
            for oy in -2i32..=2 {
                for oz in -2i32..=2 {
                    let (ax, ay, az) = (ox.abs(), oy.abs(), oz.abs());
                    let outside_core = ax > 1 || ay > 1 || az > 1;
                    let on_line = (ox == 0 && (ay == 2 || az == 2))
                        || (oy == 0 && (ax == 2 || az == 2))
                        || (oz == 0 && (ax == 2 || ay == 2));
                    if outside_core && on_line {
                        let pos = BlockPos::new(origin.x + ox, origin.y + oy, origin.z + oz);
                        if world
                            .get_block(&pos)
                            .has_tag(&tag::Block::MINECRAFT_CONDUIT_EFFECT_BLOCK)
                        {
                            blocks.push(pos);
                        }
                    }
                }
            }
        }

        blocks.len() >= MIN_ACTIVE_SIZE
    }

    fn is_in_water_or_rain(world: &World, entity: &crate::entity::Entity) -> bool {
        entity.touching_water.load(Ordering::Relaxed)
            || world.is_raining_at(&entity.block_pos.load())
            || world.is_raining_at(&entity.bounding_box.load().max_block_pos())
    }

    fn apply_effects(&self, world: &Arc<World>, frame_size: usize) {
        let range = (frame_size / 7 * 16) as i64;
        let pos = self.position.0;
        for player in world.players.load().iter() {
            let entity = &player.living_entity.entity;
            let player_pos = entity.block_pos.load().0;
            let (dx, dy, dz) = (
                i64::from(player_pos.x - pos.x),
                i64::from(player_pos.y - pos.y),
                i64::from(player_pos.z - pos.z),
            );
            if dx * dx + dy * dy + dz * dz < range * range
                && Self::is_in_water_or_rain(world, entity)
            {
                player.add_effect(pumpkin_data::potion::Effect {
                    effect_type: &StatusEffect::CONDUIT_POWER,
                    duration: EFFECT_DURATION,
                    amplifier: 0,
                    ambient: true,
                    show_particles: true,
                    show_icon: true,
                    blend: false,
                });
            }
        }
    }

    fn update_and_attack_target(&self, world: &Arc<World>, hunting: bool) {
        let mut guard = self
            .target
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let old = *guard;
        let new = self.update_destroy_target(world, old, hunting);
        let target_entity = new.and_then(|id| world.get_entity_by_uuid(id));
        if let Some(entity) = target_entity {
            let pos = entity.get_entity().pos.load();
            world.play_sound(Sound::BlockConduitAttackTarget, SoundCategory::Blocks, &pos);
            entity.damage(&*entity, 4.0, DamageType::MAGIC);
        }
        if new != old {
            *guard = new;
            // Vanilla `sendBlockUpdated`: clients draw the eye tracking `Target`. The lock must be
            // released first, `chunk_data_nbt` only `try_lock`s it.
            drop(guard);
            if let Some(block_entity) = world.get_block_entity(&self.position) {
                world.update_block_entity(&block_entity);
            }
        }
    }

    fn update_destroy_target(
        &self,
        world: &Arc<World>,
        target: Option<Uuid>,
        hunting: bool,
    ) -> Option<Uuid> {
        if !hunting {
            return None;
        }
        let Some(target) = target else {
            return self.select_new_target(world);
        };
        let entity = world.get_entity_by_uuid(target)?;
        let base = entity.get_entity();
        let target_pos = base.block_pos.load().0;
        let pos = self.position.0;
        let (dx, dy, dz) = (
            f64::from(target_pos.x - pos.x),
            f64::from(target_pos.y - pos.y),
            f64::from(target_pos.z - pos.z),
        );
        (base.is_alive() && dx * dx + dy * dy + dz * dz < KILL_RANGE * KILL_RANGE).then_some(target)
    }

    fn select_new_target(&self, world: &Arc<World>) -> Option<Uuid> {
        let area =
            BoundingBox::from_block(&self.position).expand(KILL_RANGE, KILL_RANGE, KILL_RANGE);
        // TODO: vanilla matches the `Enemy` interface; the monster mob category is the closest
        // equivalent available (it also covers e.g. piglins, which are not `Enemy` in vanilla).
        let candidates: Vec<_> = world
            .get_entities_at_box(&area)
            .into_iter()
            .filter(|entity| {
                let base = entity.get_entity();
                entity.get_living_entity().is_some()
                    && base.is_alive()
                    && base.entity_type.category == &MobCategory::MONSTER
                    && Self::is_in_water_or_rain(world, base)
            })
            .collect();
        if candidates.is_empty() {
            return None;
        }
        let index = rand::rng().random_range(0..candidates.len());
        Some(candidates[index].get_entity().entity_uuid)
    }
}
