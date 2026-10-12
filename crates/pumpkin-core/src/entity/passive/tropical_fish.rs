use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Weak};

use pumpkin_data::dye_color::DyeColor;
use pumpkin_data::sound::Sound;
use pumpkin_data::tracked_data;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_protocol::codec::var_int::VarInt;
use pumpkin_util::math::vector3::Vector3;
use rand::RngExt;

use crate::entity::{
    Entity, EntityBase,
    mob::{Mob, MobEntity, spawn::SpawnGroupData},
    passive::fish::{self, AbstractFish, AbstractSchoolingFish, FishState},
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
    schooling: FishState,
    variant: AtomicI32,
}

impl TropicalFishEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let tropical_fish = Self {
            mob_entity,
            schooling: FishState::default(),
            variant: AtomicI32::new(DEFAULT_VARIANT_PACKED),
        };
        let mob_arc = Arc::new(tropical_fish);
        let entity_weak: Weak<dyn EntityBase> = {
            let entity_arc: Arc<dyn EntityBase> = mob_arc.clone();
            Arc::downgrade(&entity_arc)
        };

        fish::register_fish(&mob_arc.mob_entity);
        fish::register_schooling_goals(&mob_arc.mob_entity, &entity_weak);

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
}

impl AbstractSchoolingFish for TropicalFishEntity {
    fn schooling(&self) -> &FishState {
        &self.schooling
    }
}

impl Mob for TropicalFishEntity {
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

        // Vanilla shares the rolled variant across a spawn group through
        // `TropicalFishGroupData`, which Pumpkin's `SpawnGroupData` cannot carry yet, so each
        // fish rolls its own.
        let mut rng = rand::rng();
        let variant = if rng.random::<f32>() < COMMON_VARIANT_CHANCE {
            COMMON_VARIANTS[rng.random_range(0..COMMON_VARIANTS.len())]
        } else {
            let pattern = Pattern::VALUES[rng.random_range(0..Pattern::VALUES.len())];
            let base_color = DyeColor::by_id(rng.random_range(0u8..16)).unwrap_or_default();
            let pattern_color = DyeColor::by_id(rng.random_range(0u8..16)).unwrap_or_default();
            (pattern, base_color, pattern_color)
        };
        self.set_variant(pack_variant(variant.0, variant.1, variant.2));

        group_data
    }

    fn mob_init_data_tracker(&self) {
        self.get_entity().set_synced_data(
            tracked_data::tropical_fish::DATA_ID_TYPE_VARIANT,
            VarInt(self.get_packed_variant()),
        );
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        nbt.put_int("Variant", self.get_packed_variant());
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
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
