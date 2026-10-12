use crate::block::entities::BlockEntity;
use crate::entity::EntityBase;
use crate::world::World;
use crossbeam::atomic::AtomicCell;
use pumpkin_data::BlockDirection;
use pumpkin_data::effect::StatusEffect;
use pumpkin_data::potion::Effect;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag;
use pumpkin_data::tag::Taggable;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::boundingbox::BoundingBox;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use std::any::Any;
use std::sync::{Arc, Mutex};

const DURATION: i32 = 50;
const GLOW_DURATION: i32 = 60;
const MIN_TICKS_BETWEEN_SEARCHES: i64 = 60;
const MAX_RESONATION_TICKS: i32 = 40;
const TICKS_BEFORE_RESONATION: i32 = 5;
const SEARCH_RADIUS: f64 = 48.0;
const HEAR_BELL_RADIUS: f64 = 32.0;
const HIGHLIGHT_RAIDERS_RADIUS: f64 = 48.0;

/// Block event type sent when the bell is rung.
pub const EVENT_BELL_RING: u8 = 1;

pub struct BellBlockEntity {
    pub position: BlockPos,
    last_ring_timestamp: AtomicCell<i64>,
    pub ticks: AtomicCell<i32>,
    pub shaking: AtomicCell<bool>,
    pub click_direction: AtomicCell<Option<BlockDirection>>,
    nearby_entities: Mutex<Option<Vec<Arc<dyn EntityBase>>>>,
    resonating: AtomicCell<bool>,
    resonation_ticks: AtomicCell<i32>,
}

impl BellBlockEntity {
    pub const ID: &'static str = "minecraft:bell";

    #[must_use]
    pub const fn new(position: BlockPos) -> Self {
        Self {
            position,
            last_ring_timestamp: AtomicCell::new(0),
            ticks: AtomicCell::new(0),
            shaking: AtomicCell::new(false),
            click_direction: AtomicCell::new(None),
            nearby_entities: Mutex::new(None),
            resonating: AtomicCell::new(false),
            resonation_ticks: AtomicCell::new(0),
        }
    }

    /// Vanilla `BellBlockEntity.onHit`.
    pub fn on_hit(&self, world: &Arc<World>, click_direction: BlockDirection) {
        self.click_direction.store(Some(click_direction));
        if self.shaking.load() {
            self.ticks.store(0);
        } else {
            self.shaking.store(true);
        }
        world.add_synced_block_event(self.position, EVENT_BELL_RING, click_direction.to_index());
    }

    /// Vanilla `BellBlockEntity.triggerEvent`.
    pub fn trigger_event(&self, world: &Arc<World>, r#type: u8, data: u8) -> bool {
        if r#type != EVENT_BELL_RING {
            return false;
        }
        self.update_entities(world);
        self.resonation_ticks.store(0);
        self.click_direction.store(BlockDirection::from_index(data));
        self.ticks.store(0);
        self.shaking.store(true);
        true
    }

    fn update_entities(&self, world: &Arc<World>) {
        let game_time = world.get_world_age();
        let mut nearby = self
            .nearby_entities
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if game_time > self.last_ring_timestamp.load() + MIN_TICKS_BETWEEN_SEARCHES
            || nearby.is_none()
        {
            self.last_ring_timestamp.store(game_time);
            let aabb = BoundingBox::from_block(&self.position).expand_all(SEARCH_RADIUS);
            *nearby = Some(
                world
                    .get_entities_at_box(&aabb)
                    .into_iter()
                    .filter(|entity| entity.get_living_entity().is_some())
                    .collect(),
            );
        }
        // TODO: set HEARD_BELL_TIME on each living entity within HEAR_BELL_RADIUS once mobs run on the brain
    }

    fn is_raider_within(&self, entity: &Arc<dyn EntityBase>, radius: f64) -> bool {
        let base = entity.get_entity();
        base.is_alive()
            && self.closer_to_center_than(&base.pos.load(), radius)
            && base
                .entity_type
                .has_tag(&tag::EntityType::MINECRAFT_RAIDERS)
    }

    /// Vanilla `Vec3i.closerToCenterThan`.
    fn closer_to_center_than(&self, pos: &Vector3<f64>, distance: f64) -> bool {
        let center = self.position.to_centered_f64();
        center.squared_distance_to_vec(pos) < distance * distance
    }

    fn are_raiders_nearby(&self, nearby: &[Arc<dyn EntityBase>]) -> bool {
        nearby
            .iter()
            .any(|entity| self.is_raider_within(entity, HEAR_BELL_RADIUS))
    }

    fn make_raiders_glow(&self, nearby: &[Arc<dyn EntityBase>]) {
        for entity in nearby {
            if self.is_raider_within(entity, HIGHLIGHT_RAIDERS_RADIUS)
                && let Some(living) = entity.get_living_entity()
            {
                living.add_effect(Effect {
                    effect_type: &StatusEffect::GLOWING,
                    duration: GLOW_DURATION,
                    amplifier: 0,
                    ambient: false,
                    show_particles: true,
                    show_icon: true,
                    blend: false,
                });
            }
        }
    }
}

impl BlockEntity for BellBlockEntity {
    fn write_nbt(&self, _nbt: &mut NbtCompound) {}

    fn from_nbt(_nbt: &NbtCompound, position: BlockPos) -> Self
    where
        Self: Sized,
    {
        Self::new(position)
    }

    fn tick(&self, world: &Arc<World>) {
        if self.shaking.load() {
            self.ticks.fetch_add(1);
        }
        if self.ticks.load() >= DURATION {
            self.shaking.store(false);
            self.ticks.store(0);
        }

        let nearby = self
            .nearby_entities
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .unwrap_or_default();

        if self.ticks.load() >= TICKS_BEFORE_RESONATION
            && self.resonation_ticks.load() == 0
            && self.are_raiders_nearby(&nearby)
        {
            self.resonating.store(true);
            world.play_sound_fine(
                Sound::BlockBellResonate,
                SoundCategory::Blocks,
                &self.position.to_centered_f64(),
                1.0,
                1.0,
            );
        }

        if self.resonating.load() {
            if self.resonation_ticks.load() < MAX_RESONATION_TICKS {
                self.resonation_ticks.fetch_add(1);
            } else {
                self.make_raiders_glow(&nearby);
                self.resonating.store(false);
            }
        }
    }

    fn resource_location(&self) -> &'static str {
        Self::ID
    }

    fn get_position(&self) -> BlockPos {
        self.position
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
