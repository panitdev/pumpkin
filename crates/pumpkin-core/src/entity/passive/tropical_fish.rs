use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};

use pumpkin_data::data_component_impl::{
    TropicalFishBaseColorImpl, TropicalFishPatternColorImpl, TropicalFishPatternImpl,
};
use pumpkin_data::dye_color::DyeColor;
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

/// Vanilla `TropicalFish.Pattern`: the fish's shape. `base` is the small (0) or large (1)
/// model, packed as `base | index << 8`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Pattern {
    Kob = 0,
    Sunstreak = 256,
    Snooper = 512,
    Dasher = 768,
    Brinely = 1024,
    Spotty = 1280,
    Flopper = 1,
    Stripey = 257,
    Glitter = 513,
    Blockfish = 769,
    Betty = 1025,
    Clayfish = 1281,
}

impl Pattern {
    /// Declaration order, for vanilla `Util.getRandom(Pattern.values(), random)`.
    pub const VALUES: [Self; 12] = [
        Self::Kob,
        Self::Sunstreak,
        Self::Snooper,
        Self::Dasher,
        Self::Brinely,
        Self::Spotty,
        Self::Flopper,
        Self::Stripey,
        Self::Glitter,
        Self::Blockfish,
        Self::Betty,
        Self::Clayfish,
    ];

    #[must_use]
    pub const fn packed_id(self) -> i32 {
        self as i32
    }

    /// The serialized name, as in the `tropical_fish/pattern` component.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Kob => "kob",
            Self::Sunstreak => "sunstreak",
            Self::Snooper => "snooper",
            Self::Dasher => "dasher",
            Self::Brinely => "brinely",
            Self::Spotty => "spotty",
            Self::Flopper => "flopper",
            Self::Stripey => "stripey",
            Self::Glitter => "glitter",
            Self::Blockfish => "blockfish",
            Self::Betty => "betty",
            Self::Clayfish => "clayfish",
        }
    }

    #[must_use]
    pub fn by_name(name: &str) -> Option<Self> {
        Self::VALUES
            .into_iter()
            .find(|pattern| pattern.name() == name)
    }

    /// Vanilla `ByIdMap.sparse(..., KOB)`: an unknown id falls back to `KOB`.
    #[must_use]
    pub const fn by_id(packed_id: i32) -> Self {
        match packed_id {
            256 => Self::Sunstreak,
            512 => Self::Snooper,
            768 => Self::Dasher,
            1024 => Self::Brinely,
            1280 => Self::Spotty,
            1 => Self::Flopper,
            257 => Self::Stripey,
            513 => Self::Glitter,
            769 => Self::Blockfish,
            1025 => Self::Betty,
            1281 => Self::Clayfish,
            _ => Self::Kob,
        }
    }
}

/// Vanilla `TropicalFish.Variant`, packed as `pattern | base << 16 | patternColor << 24`.
type Variant = (Pattern, DyeColor, DyeColor);

/// Vanilla `TropicalFish.DEFAULT_VARIANT`: `KOB`, white on white.
const DEFAULT_VARIANT_PACKED: i32 = pack_variant(Pattern::Kob, DyeColor::White, DyeColor::White);
/// Vanilla `TropicalFish.finalizeSpawn` rolls a common variant 90% of the time.
const COMMON_VARIANT_CHANCE: f32 = 0.9;

/// Vanilla `TropicalFish.COMMON_VARIANTS`.
const COMMON_VARIANTS: [Variant; 22] = [
    (Pattern::Stripey, DyeColor::Orange, DyeColor::Gray),
    (Pattern::Flopper, DyeColor::Gray, DyeColor::Gray),
    (Pattern::Flopper, DyeColor::Gray, DyeColor::Blue),
    (Pattern::Clayfish, DyeColor::White, DyeColor::Gray),
    (Pattern::Sunstreak, DyeColor::Blue, DyeColor::Gray),
    (Pattern::Kob, DyeColor::Orange, DyeColor::White),
    (Pattern::Spotty, DyeColor::Pink, DyeColor::LightBlue),
    (Pattern::Blockfish, DyeColor::Purple, DyeColor::Yellow),
    (Pattern::Clayfish, DyeColor::White, DyeColor::Red),
    (Pattern::Spotty, DyeColor::White, DyeColor::Yellow),
    (Pattern::Glitter, DyeColor::White, DyeColor::Gray),
    (Pattern::Clayfish, DyeColor::White, DyeColor::Orange),
    (Pattern::Dasher, DyeColor::Cyan, DyeColor::Pink),
    (Pattern::Brinely, DyeColor::Lime, DyeColor::LightBlue),
    (Pattern::Betty, DyeColor::Red, DyeColor::White),
    (Pattern::Snooper, DyeColor::Gray, DyeColor::Red),
    (Pattern::Blockfish, DyeColor::Red, DyeColor::White),
    (Pattern::Flopper, DyeColor::White, DyeColor::Yellow),
    (Pattern::Kob, DyeColor::Red, DyeColor::White),
    (Pattern::Sunstreak, DyeColor::Gray, DyeColor::White),
    (Pattern::Dasher, DyeColor::Cyan, DyeColor::Yellow),
    (Pattern::Flopper, DyeColor::Yellow, DyeColor::Yellow),
];

/// Represents a Tropical Fish, a schooling passive aquatic mob.
///
/// Wiki: <https://minecraft.wiki/w/Tropical_Fish>
pub struct TropicalFishEntity {
    pub mob_entity: MobEntity,
    fish_data: FishData,
    schooling_data: SchoolingFishData,
    variant: AtomicI32,
}

impl TropicalFishEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mut mob_entity = MobEntity::new(entity);
        abstract_fish::initialize_fish_movement(&mut mob_entity);
        let mob_arc = Arc::new_cyclic(|weak: &std::sync::Weak<Self>| Self {
            mob_entity,
            fish_data: FishData::default(),
            schooling_data: SchoolingFishData::new(weak.clone()),
            variant: AtomicI32::new(DEFAULT_VARIANT_PACKED),
        });
        abstract_fish::register_fish_goals(&mob_arc.mob_entity);
        mob_arc
            .mob_entity
            .goals_selector
            .add_goal(5, Box::new(FollowFlockLeaderGoal::default()));

        mob_arc
    }

    fn get_packed_variant(&self) -> i32 {
        self.variant.load(Ordering::Relaxed)
    }

    #[must_use]
    pub fn get_pattern(&self) -> Pattern {
        get_pattern(self.get_packed_variant())
    }

    #[must_use]
    pub fn get_base_color(&self) -> DyeColor {
        get_base_color(self.get_packed_variant())
    }

    #[must_use]
    pub fn get_pattern_color(&self) -> DyeColor {
        get_pattern_color(self.get_packed_variant())
    }

    /// Vanilla `TropicalFish.applyImplicitComponent` for the pattern and color components.
    pub fn set_pattern(&self, pattern: Pattern) {
        self.set_variant(pack_variant(
            pattern,
            self.get_base_color(),
            self.get_pattern_color(),
        ));
    }

    pub fn set_base_color(&self, color: DyeColor) {
        self.set_variant(pack_variant(
            self.get_pattern(),
            color,
            self.get_pattern_color(),
        ));
    }

    pub fn set_pattern_color(&self, color: DyeColor) {
        self.set_variant(pack_variant(
            self.get_pattern(),
            self.get_base_color(),
            color,
        ));
    }

    fn set_variant(&self, packed_variant: i32) {
        self.variant.store(packed_variant, Ordering::Relaxed);
        self.get_entity().set_synced_data(
            tracked_data::tropical_fish::DATA_ID_TYPE_VARIANT,
            VarInt(packed_variant),
        );
    }
}

/// Vanilla `TropicalFish.packVariant`.
const fn pack_variant(pattern: Pattern, base_color: DyeColor, pattern_color: DyeColor) -> i32 {
    (pattern.packed_id() & 0xFFFF)
        | ((base_color.id() as i32) << 16)
        | ((pattern_color.id() as i32) << 24)
}

/// Vanilla `TropicalFish.getPattern`.
const fn get_pattern(packed_variant: i32) -> Pattern {
    Pattern::by_id(packed_variant & 0xFFFF)
}

/// Vanilla `TropicalFish.getBaseColor`, whose out-of-range colors fall back to white.
fn get_base_color(packed_variant: i32) -> DyeColor {
    DyeColor::by_id(((packed_variant >> 16) & 0xFF) as u8).unwrap_or(DyeColor::White)
}

/// Vanilla `TropicalFish.getPatternColor`.
fn get_pattern_color(packed_variant: i32) -> DyeColor {
    DyeColor::by_id(((packed_variant >> 24) & 0xFF) as u8).unwrap_or(DyeColor::White)
}

impl AbstractFish for TropicalFishEntity {
    fn flop_sound(&self) -> Sound {
        Sound::EntityTropicalFishFlop
    }
    fn fish_data(&self) -> &FishData {
        &self.fish_data
    }
    fn ambient_sound(&self) -> Option<Sound> {
        Some(Sound::EntityTropicalFishAmbient)
    }
}

impl AbstractSchoolingFish for TropicalFishEntity {
    fn schooling_data(&self) -> &SchoolingFishData {
        &self.schooling_data
    }
}

abstract_fish_impls!(TropicalFishEntity);

impl Bucketable for TropicalFishEntity {
    fn is_from_bucket(&self) -> bool {
        self.fish_data.from_bucket.load(Ordering::Relaxed)
    }
    fn set_from_bucket(&self, from_bucket: bool) {
        abstract_fish::set_fish_from_bucket(self, from_bucket);
    }
    fn bucket_item(&self) -> &'static pumpkin_data::item::Item {
        &pumpkin_data::item::Item::TROPICAL_FISH_BUCKET
    }
    fn pickup_sound(&self) -> Sound {
        Sound::ItemBucketFillFish
    }

    /// Vanilla `TropicalFish.saveToBucketTag`.
    fn save_to_bucket_tag(&self, bucket: &mut ItemStack) {
        crate::entity::bucketable::save_default_data_to_bucket_tag(self, bucket);
        bucket.set_data_component(TropicalFishPatternImpl {
            value: self.get_pattern().name().into(),
        });
        bucket.set_data_component(TropicalFishBaseColorImpl {
            value: self.get_base_color().name().into(),
        });
        bucket.set_data_component(TropicalFishPatternColorImpl {
            value: self.get_pattern_color().name().into(),
        });
    }
}

impl Mob for TropicalFishEntity {
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
        let group_data = self.finalize_school_spawn(group_data);
        // Vanilla `TropicalFishGroupData`: the school shares the variant its leader rolled.
        if let Some(SpawnGroupData::TropicalFishSchool(_, packed_variant)) = group_data {
            self.set_variant(packed_variant);
            return group_data;
        }

        let mut rng = rand::rng();
        if rng.random::<f32>() < COMMON_VARIANT_CHANCE {
            let (pattern, base_color, pattern_color) =
                COMMON_VARIANTS[rng.random_range(0..COMMON_VARIANTS.len())];
            let packed_variant = pack_variant(pattern, base_color, pattern_color);
            self.set_variant(packed_variant);
            return Some(SpawnGroupData::TropicalFishSchool(
                self.schooling_data.self_reference.clone(),
                packed_variant,
            ));
        }
        // TODO: vanilla also clears `isSchool` here, which `isMaxGroupSizeReached` reads to stop
        // the natural spawner from adding more fish to the group.
        let pattern = Pattern::VALUES[rng.random_range(0..Pattern::VALUES.len())];
        let base_color = DyeColor::by_id(rng.random_range(0u8..16)).unwrap_or_default();
        let pattern_color = DyeColor::by_id(rng.random_range(0u8..16)).unwrap_or_default();
        self.set_variant(pack_variant(pattern, base_color, pattern_color));
        group_data
    }

    fn mob_init_data_tracker(&self) {
        self.set_from_bucket(self.is_from_bucket());
        self.get_entity().set_synced_data(
            tracked_data::tropical_fish::DATA_ID_TYPE_VARIANT,
            VarInt(self.get_packed_variant()),
        );
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        self.fish_write_nbt(nbt);
        nbt.put_int("Variant", self.get_packed_variant());
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.fish_read_nbt(nbt);
        if let Some(packed_variant) = nbt.get_int("Variant") {
            self.set_variant(packed_variant);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_and_unpacks_variant() {
        // Vanilla `packVariant(STRIPEY, ORANGE, GRAY)`: 257 | 1 << 16 | 7 << 24.
        let packed = pack_variant(Pattern::Stripey, DyeColor::Orange, DyeColor::Gray);
        assert_eq!(packed, 117_506_305);
        assert_eq!(get_pattern(packed), Pattern::Stripey);
        assert_eq!(get_base_color(packed), DyeColor::Orange);
        assert_eq!(get_pattern_color(packed), DyeColor::Gray);
    }

    #[test]
    fn unknown_pattern_id_falls_back_to_kob() {
        assert_eq!(Pattern::by_id(0x1234), Pattern::Kob);
    }

    #[test]
    fn out_of_range_color_is_white() {
        // Vanilla `ByIdMap.continuous(..., ZERO)`.
        let packed = (200 << 16) | (250 << 24);
        assert_eq!(get_base_color(packed), DyeColor::White);
        assert_eq!(get_pattern_color(packed), DyeColor::White);
    }
}
