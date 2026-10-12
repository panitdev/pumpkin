use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Weak};

use pumpkin_data::sound::Sound;
use pumpkin_data::tracked_data;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::codec::var_int::VarInt;
use pumpkin_util::math::boundingbox::BoundingBox;
use pumpkin_util::math::vector3::Vector3;
use rand::RngExt;

use crate::entity::{
    Entity, EntityBase,
    mob::{Mob, MobEntity, spawn::SpawnGroupData},
    passive::fish::{self, AbstractFish, AbstractSchoolingFish, FishState},
};

/// Vanilla `Salmon.Variant`: the weighted sizes a salmon spawns as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(i32)]
pub enum SalmonVariant {
    Small = 0,
    #[default]
    Medium = 1,
    Large = 2,
}

impl SalmonVariant {
    /// Vanilla `ByIdMap.continuous(..., CLAMP)`: out-of-range ids clamp to the ends.
    #[must_use]
    pub const fn from_id(id: i32) -> Self {
        match id {
            ..=0 => Self::Small,
            1 => Self::Medium,
            _ => Self::Large,
        }
    }

    #[must_use]
    pub const fn id(self) -> i32 {
        self as i32
    }

    #[must_use]
    pub const fn bounding_box_scale(self) -> f32 {
        match self {
            Self::Small => 0.5,
            Self::Medium => 1.0,
            Self::Large => 1.5,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Small => "small",
            Self::Medium => "medium",
            Self::Large => "large",
        }
    }

    #[must_use]
    pub fn from_name(name: &str) -> Self {
        match name {
            "small" => Self::Small,
            "large" => Self::Large,
            _ => Self::Medium,
        }
    }

    /// Vanilla `finalizeSpawn`: small 30, medium 50, large 15.
    #[must_use]
    pub fn random() -> Self {
        match rand::rng().random_range(0..95) {
            0..30 => Self::Small,
            30..80 => Self::Medium,
            _ => Self::Large,
        }
    }
}

pub struct SalmonEntity {
    pub mob_entity: MobEntity,
    schooling: FishState,
    variant: AtomicI32,
}

impl SalmonEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let salmon = Self {
            mob_entity,
            schooling: FishState::default(),
            variant: AtomicI32::new(SalmonVariant::default().id()),
        };
        let mob_arc = Arc::new(salmon);
        let entity_weak: Weak<dyn EntityBase> = {
            let entity_arc: Arc<dyn EntityBase> = mob_arc.clone();
            Arc::downgrade(&entity_arc)
        };

        fish::register_fish(&mob_arc.mob_entity);
        fish::register_schooling_goals(&mob_arc.mob_entity, &entity_weak);
        mob_arc.apply_dimensions();

        mob_arc
    }

    #[must_use]
    pub fn get_variant(&self) -> SalmonVariant {
        SalmonVariant::from_id(self.variant.load(Ordering::Relaxed))
    }

    pub fn set_variant(&self, variant: SalmonVariant) {
        self.variant.store(variant.id(), Ordering::Relaxed);
        self.get_entity()
            .set_synced_data(tracked_data::salmon::DATA_TYPE, VarInt(variant.id()));
        self.apply_dimensions();
    }

    /// Vanilla `Salmon.getDefaultDimensions`: the type's box scaled by the variant.
    fn apply_dimensions(&self) {
        let entity = self.get_entity();
        let base = Entity::type_dimensions(entity.entity_type);
        let scale = self.get_variant().bounding_box_scale();
        let dimension = pumpkin_util::math::boundingbox::EntityDimensions::new(
            base.width * scale,
            base.height * scale,
            base.eye_height * scale,
        );
        let pos = entity.pos.load();
        entity.entity_dimension.store(dimension);
        entity
            .bounding_box
            .store(BoundingBox::new_from_pos(pos.x, pos.y, pos.z, &dimension));
    }
}

impl AbstractFish for SalmonEntity {
    fn flop_sound(&self) -> Sound {
        Sound::EntitySalmonFlop
    }
}

impl AbstractSchoolingFish for SalmonEntity {
    fn schooling(&self) -> &FishState {
        &self.schooling
    }

    fn max_school_size(&self) -> i32 {
        5
    }
}

impl Mob for SalmonEntity {
    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn as_schooling_fish(&self) -> Option<&dyn AbstractSchoolingFish> {
        Some(self)
    }

    fn mob_tick(&self, _caller: &dyn EntityBase) {
        fish::fish_tick(self);
    }

    fn travel_in_water(&self, caller: &dyn EntityBase, movement_input: Vector3<f64>) -> bool {
        fish::fish_travel_in_water(self, caller, movement_input);
        true
    }

    fn finalize_spawn(
        &self,
        _world: &Arc<crate::world::World>,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        self.mob_entity.finalize_spawn_base();
        self.set_variant(SalmonVariant::random());
        group_data
    }

    fn mob_init_data_tracker(&self) {
        self.get_entity().set_synced_data(
            tracked_data::salmon::DATA_TYPE,
            VarInt(self.get_variant().id()),
        );
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        nbt.put_string("type", self.get_variant().name().to_string());
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        if let Some(name) = nbt.get_string("type") {
            self.set_variant(SalmonVariant::from_name(name));
        }
    }
}
