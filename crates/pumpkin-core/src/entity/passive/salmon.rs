use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};

use pumpkin_data::data_component_impl::SalmonSizeImpl;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::Sound;
use pumpkin_data::tracked_data;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::codec::var_int::VarInt;
use rand::RngExt;

use super::abstract_fish::{self, AbstractFish, FishData, abstract_fish_impls, abstract_fish_mob};
use super::abstract_schooling_fish::{AbstractSchoolingFish, SchoolingFishData};
use crate::entity::ai::goal::follow_flock_leader::FollowFlockLeaderGoal;
use crate::entity::bucketable::Bucketable;
use crate::entity::{
    Entity, EntityBase,
    mob::{Mob, MobEntity, spawn::SpawnGroupData},
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
    fish_data: FishData,
    schooling_data: SchoolingFishData,
    variant: AtomicI32,
}

impl SalmonEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mut mob_entity = MobEntity::new(entity);
        abstract_fish::initialize_fish_movement(&mut mob_entity);
        let mob_arc = Arc::new_cyclic(|weak: &std::sync::Weak<Self>| Self {
            mob_entity,
            fish_data: FishData::default(),
            schooling_data: SchoolingFishData::new(weak.clone()),
            variant: AtomicI32::new(SalmonVariant::default().id()),
        });
        abstract_fish::register_fish_goals(&mob_arc.mob_entity);
        mob_arc
            .mob_entity
            .goals_selector
            .add_goal(5, Box::new(FollowFlockLeaderGoal::default()));
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
        abstract_fish::apply_scaled_dimensions(
            self.get_entity(),
            self.get_variant().bounding_box_scale(),
        );
    }
}

impl Mob for SalmonEntity {
    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    abstract_fish_mob!();

    fn as_schooling_fish(&self) -> Option<&dyn AbstractSchoolingFish> {
        Some(self)
    }

    fn post_tick(&self) {
        self.schooling_tick();
    }

    fn finalize_spawn(
        &self,
        _world: &Arc<crate::world::World>,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        self.set_variant(SalmonVariant::random());
        self.finalize_school_spawn(group_data)
    }

    /// Vanilla `Salmon.applyImplicitComponent(SALMON_SIZE)`.
    fn mob_set_variant_name(&self, name: &str) {
        self.set_variant(SalmonVariant::from_name(
            name.strip_prefix("minecraft:").unwrap_or(name),
        ));
    }

    fn mob_init_data_tracker(&self) {
        self.set_from_bucket(self.is_from_bucket());
        self.get_entity().set_synced_data(
            tracked_data::salmon::DATA_TYPE,
            VarInt(self.get_variant().id()),
        );
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        self.fish_write_nbt(nbt);
        nbt.put_string("type", self.get_variant().name().to_string());
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.fish_read_nbt(nbt);
        if let Some(name) = nbt.get_string("type") {
            self.set_variant(SalmonVariant::from_name(name));
        }
    }
}

impl AbstractFish for SalmonEntity {
    fn flop_sound(&self) -> Sound {
        Sound::EntitySalmonFlop
    }
    fn fish_data(&self) -> &FishData {
        &self.fish_data
    }
    fn ambient_sound(&self) -> Option<Sound> {
        Some(Sound::EntitySalmonAmbient)
    }
}

impl AbstractSchoolingFish for SalmonEntity {
    fn schooling_data(&self) -> &SchoolingFishData {
        &self.schooling_data
    }
}

abstract_fish_impls!(SalmonEntity);

impl Bucketable for SalmonEntity {
    fn is_from_bucket(&self) -> bool {
        self.fish_data.from_bucket.load(Ordering::Relaxed)
    }
    fn set_from_bucket(&self, from_bucket: bool) {
        abstract_fish::set_fish_from_bucket(self, from_bucket);
    }
    fn bucket_item(&self) -> &'static pumpkin_data::item::Item {
        &pumpkin_data::item::Item::SALMON_BUCKET
    }
    fn pickup_sound(&self) -> Sound {
        Sound::ItemBucketFillFish
    }

    /// Vanilla `Salmon.saveToBucketTag`.
    fn save_to_bucket_tag(&self, bucket: &mut ItemStack) {
        crate::entity::bucketable::save_default_data_to_bucket_tag(self, bucket);
        bucket.set_data_component(SalmonSizeImpl {
            value: self.get_variant().name().into(),
        });
    }
}
