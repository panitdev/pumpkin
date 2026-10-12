use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use crate::entity::experience_orb::ExperienceOrbEntity;
use crate::entity::item::ItemEntity;
use crate::entity::passive::villager::{apply_potion, enchant_with_levels};
use crate::entity::projectile::{ProjectileHit, is_projectile};
use crate::world::World;
use crate::world::loot::{LootContextParameters, get_loot_table};
use crate::{
    entity::{Entity, EntityBase, living::LivingEntity, player::Player},
    server::Server,
};
use crossbeam::atomic::AtomicCell;
use pumpkin_data::Block;
use pumpkin_data::attributes::Attributes;
use pumpkin_data::chunk::Biome;
use pumpkin_data::entity::{EntityStatus, EntityType};
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_util::math::boundingbox::BoundingBox;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;
use rand::RngExt;

/// `FishingHook.OpenWaterType`
#[derive(Clone, Copy, PartialEq, Eq)]
enum OpenWaterType {
    AboveWater,
    InsideWater,
    Invalid,
}

/// Top level entries of the `gameplay/fishing` loot table: sub table, weight, quality.
const FISHING_JUNK: (&str, i32, f32) = ("gameplay/fishing/junk", 10, -2.0);
const FISHING_TREASURE: (&str, i32, f32) = ("gameplay/fishing/treasure", 5, 2.0);
const FISHING_FISH: (&str, i32, f32) = ("gameplay/fishing/fish", 85, -1.0);
/// `levels` of the `enchant_with_levels` function on the treasure entries.
const TREASURE_ENCHANT_LEVELS: i32 = 30;
/// Upper bound of the `set_damage` function on the treasure and junk entries.
const TREASURE_MAX_DAMAGE_FRACTION: f32 = 0.25;
const JUNK_MAX_DAMAGE_FRACTION: f32 = 0.9;
const MAX_OUT_OF_WATER_TIME: i32 = 10;

pub struct FishingBobberEntity {
    pub entity: Entity,
    pub owner_id: i32,
    pub hooked_entity_id: AtomicI32,
    pub in_ground: AtomicBool,
    pub has_hit: AtomicBool,
    nibble: AtomicI32,
    time_until_lured: AtomicI32,
    time_until_hooked: AtomicI32,
    fish_angle: AtomicCell<f32>,
    open_water: AtomicBool,
    out_of_water_time: AtomicI32,
    luck: i32,
    lure_speed: i32,
}

impl FishingBobberEntity {
    const WATER_INERTIA: f64 = 0.8;
    const AIR_INERTIA: f64 = 0.92;
    const GRAVITY: f64 = 0.03;

    pub fn new(entity: Entity, owner: &Player, luck: i32, lure_speed: i32) -> Self {
        // FishingHook(Player, Level, int, int): starts in front of the eyes, launched along the look vector.
        let owner_entity = &owner.living_entity.entity;
        let yaw = owner_entity.yaw.load();
        let pitch = owner_entity.pitch.load();
        let y_cos = (-yaw.to_radians() - std::f32::consts::PI).cos();
        let y_sin = (-yaw.to_radians() - std::f32::consts::PI).sin();
        let x_cos = -(-pitch.to_radians()).cos();
        let x_sin = (-pitch.to_radians()).sin();
        let owner_pos = owner_entity.pos.load();
        entity.pos.store(Vector3::new(
            owner_pos.x - f64::from(y_sin) * 0.3,
            owner_pos.y + owner_entity.get_eye_height(),
            owner_pos.z - f64::from(y_cos) * 0.3,
        ));
        let direction = Vector3::new(
            f64::from(-y_sin),
            f64::from((-(x_sin / x_cos)).clamp(-5.0, 5.0)),
            f64::from(-y_cos),
        );
        let length = direction.length();
        let mut rng = rand::rng();
        let mut scale =
            || 0.6 / length + 0.5 + 0.010_336_5 * (rng.random::<f64>() - rng.random::<f64>());
        let velocity = direction.multiply(scale(), scale(), scale());
        entity.velocity.store(velocity);
        entity
            .yaw
            .store(velocity.x.atan2(velocity.z).to_degrees() as f32);
        entity
            .pitch
            .store(velocity.y.atan2(velocity.x.hypot(velocity.z)).to_degrees() as f32);

        Self {
            entity,
            owner_id: owner_entity.entity_id,
            hooked_entity_id: AtomicI32::new(0),
            in_ground: AtomicBool::new(false),
            has_hit: AtomicBool::new(false),
            nibble: AtomicI32::new(0),
            time_until_lured: AtomicI32::new(0),
            time_until_hooked: AtomicI32::new(0),
            fish_angle: AtomicCell::new(0.0),
            open_water: AtomicBool::new(true),
            out_of_water_time: AtomicI32::new(0),
            luck: luck.max(0),
            lure_speed: lure_speed.max(0),
        }
    }

    /// `FishingHook.pullEntity`
    fn pull_entity(&self, target: &Entity, owner: &Player) {
        let owner_pos = owner.living_entity.entity.pos.load();
        let delta = (owner_pos - self.entity.pos.load()).multiply(0.1, 0.1, 0.1);
        target.add_velocity(delta);
    }

    /// `FishingHook.retrieve`; returns the damage the rod takes.
    pub fn retrieve(&self, player: &Player, rod: &ItemStack) -> i32 {
        let world = self.entity.world.load();
        let hooked_id = self.hooked_entity_id.load(Ordering::Relaxed);
        let mut damage = 0;

        if hooked_id != 0
            && let Some(hooked) = world.get_entity_by_id(hooked_id)
        {
            self.pull_entity(hooked.get_entity(), player);
            world.send_entity_status(&self.entity, EntityStatus::FishingRodReelIn, None);
            damage = if hooked.cast_any().is::<ItemEntity>() {
                3
            } else {
                5
            };
        } else if self.nibble.load(Ordering::Relaxed) > 0 {
            let luck = self.luck as f32
                + player.living_entity.get_attribute_value(&Attributes::LUCK) as f32;
            let pos = self.entity.pos.load();
            let open_water = self.open_water.load(Ordering::Relaxed);
            let owner_pos = player.living_entity.entity.pos.load();
            for stack in Self::fishing_loot(&world, rod, luck, open_water, pos) {
                let is_fish = stack.item.has_tag(&tag::Item::MINECRAFT_FISHES);
                let item_name = stack.item.registry_key;
                let (xa, ya, za) = (
                    owner_pos.x - pos.x,
                    owner_pos.y - pos.y,
                    owner_pos.z - pos.z,
                );
                let velocity = Vector3::new(
                    xa * 0.1,
                    ya * 0.1 + (xa * xa + ya * ya + za * za).sqrt().sqrt() * 0.08,
                    za * 0.1,
                );
                let item_entity = Entity::new(world.clone(), pos, &EntityType::ITEM);
                world.spawn_entity(Arc::new(ItemEntity::new_with_velocity(
                    item_entity,
                    stack,
                    velocity,
                    ItemEntity::DEFAULT_PICKUP_DELAY,
                )));
                let orb = Entity::new(
                    world.clone(),
                    Vector3::new(owner_pos.x, owner_pos.y + 0.5, owner_pos.z + 0.5),
                    &EntityType::EXPERIENCE_ORB,
                );
                world.spawn_entity(Arc::new(ExperienceOrbEntity::new(
                    orb,
                    rand::rng().random_range(1..=6),
                )));
                if is_fish {
                    player.increment_stat(
                        pumpkin_data::statistic::StatisticCategory::Custom,
                        pumpkin_data::statistic::CustomStatistic::FishCaught as i32,
                        1,
                    );
                    player.trigger_advancement(
                        crate::entity::player::advancement::trigger::AdvancementTrigger::FishedItem {
                            item_id: format!("minecraft:{item_name}"),
                        },
                    );
                }
            }
            damage = 1;
        }

        if self.entity.on_ground.load(Ordering::Relaxed) {
            damage = 2;
        }
        damage
    }

    /// Rolls the `gameplay/fishing` loot table.
    ///
    /// The generated static table flattens the nested tables and drops quality, conditions and item
    /// functions, so the top level choice and the functions of the sub tables are applied here.
    fn fishing_loot(
        world: &Arc<World>,
        _rod: &ItemStack,
        luck: f32,
        open_water: bool,
        pos: Vector3<f64>,
    ) -> Vec<ItemStack> {
        let mut rng = rand::rng();
        // LootPoolSingletonContainer.getWeight
        let weight_of = |(_, weight, quality): (&str, i32, f32)| {
            ((weight as f32 + quality * luck).floor() as i32).max(0)
        };
        let mut entries = vec![(FISHING_JUNK, weight_of(FISHING_JUNK))];
        if open_water {
            entries.push((FISHING_TREASURE, weight_of(FISHING_TREASURE)));
        }
        entries.push((FISHING_FISH, weight_of(FISHING_FISH)));
        let total: i32 = entries.iter().map(|(_, weight)| weight).sum();
        if total <= 0 {
            return Vec::new();
        }
        let mut choice = rng.random_range(0..total);
        let Some(((table_key, _, _), _)) = entries.iter().find(|(_, weight)| {
            choice -= weight;
            choice < 0
        }) else {
            return Vec::new();
        };
        let Some(table) = get_loot_table(table_key) else {
            return Vec::new();
        };

        let biome = world.get_biome(&BlockPos::floored(pos.x, pos.y, pos.z));
        let in_jungle = [
            Biome::JUNGLE.id,
            Biome::SPARSE_JUNGLE.id,
            Biome::BAMBOO_JUNGLE.id,
        ]
        .contains(&biome.id);
        let params = LootContextParameters::default();
        let mut items = loop {
            let items = table.generate_loot_with_context(rng.random::<i64>(), &params);
            // The bamboo entry has a jungle `location_check` the generated table lost; rerolling
            // keeps the distribution of excluding the entry.
            if in_jungle || items.iter().all(|stack| stack.item.id != Item::BAMBOO.id) {
                break items;
            }
        };
        let is_treasure = *table_key == FISHING_TREASURE.0;
        for stack in &mut items {
            Self::apply_loot_functions(stack, is_treasure, &mut rng);
        }
        items
    }

    /// The `set_damage`, `set_potion` and `enchant_with_levels` functions of the fishing sub tables.
    fn apply_loot_functions(stack: &mut ItemStack, is_treasure: bool, rng: &mut impl rand::Rng) {
        let item_id = stack.item.id;
        if is_treasure {
            if item_id == Item::BOW.id || item_id == Item::FISHING_ROD.id {
                Self::set_damage(stack, TREASURE_MAX_DAMAGE_FRACTION, rng);
            }
            if [Item::BOW.id, Item::FISHING_ROD.id, Item::BOOK.id].contains(&item_id) {
                let enchanted = enchant_with_levels(
                    rng,
                    stack.clone(),
                    TREASURE_ENCHANT_LEVELS,
                    pumpkin_data::tag::Enchantment::MINECRAFT_ON_RANDOM_LOOT.1,
                );
                if let Some(enchanted) = enchanted {
                    *stack = enchanted;
                } else if item_id == Item::BOOK.id {
                    // EnchantmentHelper.enchantItem turns the book into an enchanted book even with no enchantment.
                    *stack = ItemStack::new(1, &Item::ENCHANTED_BOOK);
                }
            }
        } else if item_id == Item::LEATHER_BOOTS.id || item_id == Item::FISHING_ROD.id {
            Self::set_damage(stack, JUNK_MAX_DAMAGE_FRACTION, rng);
        } else if item_id == Item::POTION.id {
            apply_potion(stack, "minecraft:water");
        }
    }

    /// `SetItemDamageFunction` with a uniform provider.
    fn set_damage(stack: &mut ItemStack, max_fraction: f32, rng: &mut impl rand::Rng) {
        let Some(max_damage) = stack.get_max_damage() else {
            return;
        };
        let remaining = 1.0 - rng.random_range(0.0..max_fraction);
        stack.set_damage((remaining * max_damage as f32).floor() as i32);
    }

    /// `FishingHook.catchingFish`
    /// Returns true on the tick a fish starts biting.
    #[expect(clippy::too_many_lines)]
    fn catching_fish(&self, block_pos: BlockPos) -> bool {
        let world = self.entity.world.load();
        let mut rng = rand::rng();
        let pos = self.entity.pos.load();
        let width = self.entity.width();
        let mut fishing_speed = 1;
        let above = block_pos.up();
        if rng.random::<f32>() < 0.25 && world.is_raining_at(&above) {
            fishing_speed += 1;
        }
        if rng.random::<f32>() < 0.5 && !world.can_see_sky(&above) {
            fishing_speed -= 1;
        }

        let nibble = self.nibble.load(Ordering::Relaxed);
        let time_until_hooked = self.time_until_hooked.load(Ordering::Relaxed);
        let time_until_lured = self.time_until_lured.load(Ordering::Relaxed);
        if nibble > 0 {
            self.nibble.store(nibble - 1, Ordering::Relaxed);
            if nibble - 1 <= 0 {
                self.time_until_lured.store(0, Ordering::Relaxed);
                self.time_until_hooked.store(0, Ordering::Relaxed);
                self.set_biting(false);
            }
        } else if time_until_hooked > 0 {
            let time_until_hooked = time_until_hooked - fishing_speed;
            self.time_until_hooked
                .store(time_until_hooked, Ordering::Relaxed);
            if time_until_hooked > 0 {
                // random.triangle(0.0, 9.188)
                let fish_angle =
                    self.fish_angle.load() + 9.188 * (rng.random::<f32>() - rng.random::<f32>());
                self.fish_angle.store(fish_angle);
                let angle = fish_angle.to_radians();
                let (angle_sin, angle_cos) = angle.sin_cos();
                let fish_x = pos.x + f64::from(angle_sin * time_until_hooked as f32 * 0.1);
                let fish_y = pos.y.floor() + 1.0;
                let fish_z = pos.z + f64::from(angle_cos * time_until_hooked as f32 * 0.1);
                let splash_pos = BlockPos::floored(fish_x, fish_y - 1.0, fish_z);
                if world.get_block(&splash_pos) == &Block::WATER {
                    if rng.random::<f32>() < 0.15 {
                        world.spawn_particle(
                            Vector3::new(fish_x, fish_y - 0.1, fish_z),
                            Vector3::new(angle_sin, 0.1, angle_cos),
                            0.0,
                            1,
                            pumpkin_data::particle::Particle::Bubble,
                        );
                    }
                    let particle_x_movement = angle_sin * 0.04;
                    let particle_z_movement = angle_cos * 0.04;
                    world.spawn_particle(
                        Vector3::new(fish_x, fish_y, fish_z),
                        Vector3::new(particle_z_movement, 0.01, -particle_x_movement),
                        1.0,
                        0,
                        pumpkin_data::particle::Particle::Fishing,
                    );
                    world.spawn_particle(
                        Vector3::new(fish_x, fish_y, fish_z),
                        Vector3::new(-particle_z_movement, 0.01, particle_x_movement),
                        1.0,
                        0,
                        pumpkin_data::particle::Particle::Fishing,
                    );
                }
            } else {
                world.play_sound_fine(
                    Sound::EntityFishingBobberSplash,
                    SoundCategory::Neutral,
                    &pos,
                    0.25,
                    1.0 + (rng.random::<f32>() - rng.random::<f32>()) * 0.4,
                );
                let y = pos.y + 0.5;
                let count = (1.0 + width * 20.0) as i32;
                world.spawn_particle(
                    Vector3::new(pos.x, y, pos.z),
                    Vector3::new(width, 0.0, width),
                    0.2,
                    count,
                    pumpkin_data::particle::Particle::Bubble,
                );
                world.spawn_particle(
                    Vector3::new(pos.x, y, pos.z),
                    Vector3::new(width, 0.0, width),
                    0.2,
                    count,
                    pumpkin_data::particle::Particle::Fishing,
                );
                self.nibble
                    .store(rng.random_range(20..=40), Ordering::Relaxed);
                self.set_biting(true);
                return true;
            }
        } else if time_until_lured > 0 {
            let time_until_lured = time_until_lured - fishing_speed;
            self.time_until_lured
                .store(time_until_lured, Ordering::Relaxed);
            let mut tease_chance = 0.15;
            if time_until_lured < 20 {
                tease_chance += (20 - time_until_lured) as f32 * 0.05;
            } else if time_until_lured < 40 {
                tease_chance += (40 - time_until_lured) as f32 * 0.02;
            } else if time_until_lured < 60 {
                tease_chance += (60 - time_until_lured) as f32 * 0.01;
            }

            if rng.random::<f32>() < tease_chance {
                let angle = rng.random_range(0.0..360.0f32).to_radians();
                let dist = rng.random_range(25.0..60.0f32);
                let fish_x = pos.x + f64::from(angle.sin() * dist) * 0.1;
                let fish_y = pos.y.floor() + 1.0;
                let fish_z = pos.z + f64::from(angle.cos() * dist) * 0.1;
                let splash_pos = BlockPos::floored(fish_x, fish_y - 1.0, fish_z);
                if world.get_block(&splash_pos) == &Block::WATER {
                    world.spawn_particle(
                        Vector3::new(fish_x, fish_y, fish_z),
                        Vector3::new(0.1, 0.0, 0.1),
                        0.0,
                        2 + rng.random_range(0..2),
                        pumpkin_data::particle::Particle::Splash,
                    );
                }
            }

            if time_until_lured <= 0 {
                self.fish_angle.store(rng.random_range(0.0..360.0));
                self.time_until_hooked
                    .store(rng.random_range(20..=80), Ordering::Relaxed);
            }
        } else {
            self.time_until_lured.store(
                rng.random_range(100..=600) - self.lure_speed,
                Ordering::Relaxed,
            );
        }
        false
    }

    fn set_biting(&self, biting: bool) {
        self.entity.set_synced_data(
            pumpkin_data::tracked_data::fishing_bobber::DATA_BITING,
            biting,
        );
    }

    /// `FishingHook.calculateOpenWater`
    fn calculate_open_water(&self, block_pos: BlockPos) -> bool {
        let mut previous_layer = OpenWaterType::Invalid;
        for y in -1..=2 {
            let layer = self.get_open_water_type_for_area(
                block_pos.offset(Vector3::new(-2, y, -2)),
                block_pos.offset(Vector3::new(2, y, 2)),
            );
            match layer {
                OpenWaterType::AboveWater if previous_layer == OpenWaterType::Invalid => {
                    return false;
                }
                OpenWaterType::InsideWater if previous_layer == OpenWaterType::AboveWater => {
                    return false;
                }
                OpenWaterType::Invalid => return false,
                _ => {}
            }
            previous_layer = layer;
        }
        true
    }

    fn get_open_water_type_for_area(&self, from: BlockPos, to: BlockPos) -> OpenWaterType {
        BlockPos::between_closed(from, to)
            .map(|pos| self.get_open_water_type_for_block(pos))
            .reduce(|a, b| if a == b { a } else { OpenWaterType::Invalid })
            .unwrap_or(OpenWaterType::Invalid)
    }

    fn get_open_water_type_for_block(&self, pos: BlockPos) -> OpenWaterType {
        let world = self.entity.world.load();
        let (block, state) = world.get_block_and_state(&pos);
        if state.is_air() || block == &Block::LILY_PAD {
            return OpenWaterType::AboveWater;
        }
        let (fluid, fluid_state) = world.get_fluid_and_fluid_state(&pos);
        if fluid.has_tag(&tag::Fluid::MINECRAFT_WATER)
            && fluid_state.is_source
            && state.get_block_collision_shapes().next().is_none()
        {
            OpenWaterType::InsideWater
        } else {
            OpenWaterType::Invalid
        }
    }

    #[expect(clippy::too_many_lines)]
    pub fn process_tick(&self, caller: &dyn EntityBase) {
        let entity = self.get_entity();
        let world = entity.world.load();

        if self.in_ground.load(Ordering::Relaxed) {
            return;
        }

        let hooked_id = self.hooked_entity_id.load(Ordering::Relaxed);
        if hooked_id != 0 {
            if let Some(hooked) = world.get_entity_by_id(hooked_id) {
                if hooked.get_entity().removed.load(Ordering::Relaxed) {
                    self.hooked_entity_id.store(0, Ordering::Relaxed);
                } else {
                    let mut hooked_pos = hooked.get_entity().pos.load();
                    hooked_pos.y += hooked.get_entity().get_eye_height() * 0.8;
                    entity.set_pos(hooked_pos);
                    return;
                }
            } else {
                self.hooked_entity_id.store(0, Ordering::Relaxed);
            }
        }

        let mut velocity = entity.velocity.load();
        let start_pos = entity.pos.load();

        if entity.touching_water.load(Ordering::Relaxed) {
            velocity.y += 0.02; // Buoyancy

            let block_pos = entity.block_pos.load();
            if self.nibble.load(Ordering::Relaxed) <= 0
                && self.time_until_hooked.load(Ordering::Relaxed) <= 0
            {
                self.open_water.store(true, Ordering::Relaxed);
            } else {
                let still_open = self.open_water.load(Ordering::Relaxed)
                    && self.out_of_water_time.load(Ordering::Relaxed) < MAX_OUT_OF_WATER_TIME
                    && self.calculate_open_water(block_pos);
                self.open_water.store(still_open, Ordering::Relaxed);
            }
            self.out_of_water_time.store(
                (self.out_of_water_time.load(Ordering::Relaxed) - 1).max(0),
                Ordering::Relaxed,
            );
            if self.nibble.load(Ordering::Relaxed) > 0 {
                let mut rng = rand::rng();
                velocity.y +=
                    -0.1 * f64::from(rng.random::<f32>()) * f64::from(rng.random::<f32>());
            }
            if self.catching_fish(block_pos) {
                // onSyncedDataUpdated: the bobber dips when a fish bites.
                velocity.y = -0.4 * rand::rng().random_range(0.6..1.0);
            }
        } else {
            self.out_of_water_time.store(
                (self.out_of_water_time.load(Ordering::Relaxed) + 1).min(MAX_OUT_OF_WATER_TIME),
                Ordering::Relaxed,
            );
            velocity.y -= Self::GRAVITY;
        }

        let inertia = if entity.touching_water.load(Ordering::Relaxed) {
            Self::WATER_INERTIA
        } else {
            Self::AIR_INERTIA
        };
        velocity = velocity.multiply(inertia, inertia, inertia);
        entity.velocity.store(velocity);

        let new_pos = start_pos.add(&velocity);

        let search_box = BoundingBox::new(
            Vector3::new(
                start_pos.x.min(new_pos.x),
                start_pos.y.min(new_pos.y),
                start_pos.z.min(new_pos.z),
            ),
            Vector3::new(
                start_pos.x.max(new_pos.x),
                start_pos.y.max(new_pos.y),
                start_pos.z.max(new_pos.z),
            ),
        )
        .expand(0.3, 0.3, 0.3);

        // Basic block collision to stop bobber
        let (block_cols, _) = world.get_block_collisions(search_box, caller);
        if !block_cols.is_empty() {
            self.in_ground.store(true, Ordering::Relaxed);
            entity.velocity.store(Vector3::new(0.0, 0.0, 0.0));
            return;
        }

        entity.set_pos(new_pos);

        let candidates = world.get_entities_at_box(&search_box);
        for cand in candidates {
            if cand.get_entity().entity_id == self.owner_id
                || cand.get_entity().entity_id == entity.entity_id
            {
                continue;
            }

            if is_projectile(cand.get_entity().entity_type) {
                continue;
            }

            let ebb = cand.get_entity().bounding_box.load().expand(0.3, 0.3, 0.3);
            if ebb.intersects(&search_box) {
                self.hooked_entity_id
                    .store(cand.get_entity().entity_id, Ordering::Relaxed);
                entity.set_synced_data(
                    pumpkin_data::tracked_data::fishing_bobber::HOOKED_ENTITY,
                    cand.get_entity().entity_id + 1,
                );
                return;
            }
        }
    }
}

impl EntityBase for FishingBobberEntity {
    fn get_owner_id(&self) -> Option<i32> {
        Some(self.owner_id)
    }

    fn get_entity(&self) -> &Entity {
        &self.entity
    }

    fn get_living_entity(&self) -> Option<&LivingEntity> {
        None
    }

    fn cast_any(&self) -> &dyn std::any::Any {
        self
    }
    fn on_hit(&self, _hit: ProjectileHit) {
        self.has_hit.store(true, Ordering::Relaxed);
    }

    fn tick(&self, caller: &dyn EntityBase, _server: &Server) {
        self.process_tick(caller);
    }
}
