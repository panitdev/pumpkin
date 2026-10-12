use std::sync::atomic::Ordering;
use std::sync::{Arc, Weak};

use pumpkin_data::data_component_impl::EquipmentSlot;
use pumpkin_data::effect::StatusEffect;
use pumpkin_data::entity::EntityType;
use pumpkin_data::item::Item;
use pumpkin_data::item_stack::ItemStack;
use pumpkin_data::potion::Effect;
use pumpkin_data::sound::{Sound, SoundCategory};
use pumpkin_data::tag::{self, Taggable};
use pumpkin_data::tracked_data;
use pumpkin_nbt::compound::NbtCompound;

use crate::entity::{
    Entity, EntityBase,
    ai::goal::{
        Controls, Goal, active_target::ActiveTargetGoal, avoid_entity::AvoidEntityGoal,
        bow_attack::BowAttackGoal, look_around::RandomLookAroundGoal,
        look_at_entity::LookAtEntityGoal, revenge::RevengeGoal, swim::SwimGoal,
        wander_around::WanderAroundGoal,
    },
    mob::{
        Mob, MobEntity, RangedAttackMob,
        equipment::RegionalDifficulty,
        evoker::IllagerSpell,
        patrol::{LongDistancePatrolGoal, PatrolData, PatrollingMonster},
        raider::{
            ObtainRaidLeaderBannerGoal, PathfindToRaidGoal, Raider, RaiderCelebrationGoal,
            RaiderData, RaiderMoveThroughVillageGoal,
        },
        spellcaster_illager::{
            SpellcasterCastingSpellGoal, SpellcasterIllager, SpellcasterIllagerData,
            SpellcasterUseSpell,
        },
    },
    projectile::arrow::{ArrowEntity, ArrowPickup},
};

const ARROW_SPEED: f64 = 1.6;
/// Vanilla `Difficulty.NORMAL.ordinal()`: `DifficultyInstance.isHarderThan` compares against it.
const NORMAL_DIFFICULTY: f32 = 2.0;

pub struct IllusionerEntity {
    pub mob_entity: MobEntity,
    pub raider_data: RaiderData,
    pub spellcaster_data: SpellcasterIllagerData,
}

impl IllusionerEntity {
    #[must_use]
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let illusioner = Self {
            mob_entity,
            raider_data: RaiderData::default(),
            spellcaster_data: SpellcasterIllagerData::new(),
        };
        let mob_arc = Arc::new(illusioner);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };
        let caster_weak: Weak<dyn SpellcasterIllager> = {
            let caster: Arc<dyn SpellcasterIllager> = mob_arc.clone();
            Arc::downgrade(&caster)
        };
        let ranged_weak: Weak<dyn RangedAttackMob> = {
            let ranged: Arc<dyn RangedAttackMob> = mob_arc.clone();
            Arc::downgrade(&ranged)
        };
        let self_weak: Weak<Self> = Arc::downgrade(&mob_arc);

        {
            let mut goal_selector = mob_arc
                .mob_entity
                .goals_selector
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            goal_selector.add_goal(0, Box::new(SwimGoal::default()));
            // Same-priority goals keep insertion order: vanilla runs `Raider.registerGoals`
            // before the Illusioner's own goals.
            goal_selector.add_goal(1, Box::new(ObtainRaidLeaderBannerGoal));
            goal_selector.add_goal(1, Box::new(SpellcasterCastingSpellGoal::new(caster_weak)));
            goal_selector.add_goal(3, Box::new(PathfindToRaidGoal::default()));
            goal_selector.add_goal(
                3,
                Box::new(AvoidEntityGoal::new(&EntityType::CREAKING, 8.0, 1.0, 1.2)),
            );
            goal_selector.add_goal(4, Box::new(LongDistancePatrolGoal::new(0.7, 0.595)));
            goal_selector.add_goal(4, Box::new(RaiderMoveThroughVillageGoal::new(1.05)));
            goal_selector.add_goal(
                4,
                Box::new(IllusionerMirrorSpellGoal::new(self_weak.clone())),
            );
            goal_selector.add_goal(5, Box::new(RaiderCelebrationGoal));
            goal_selector.add_goal(5, Box::new(IllusionerBlindnessSpellGoal::new(self_weak)));
            goal_selector.add_goal(6, Box::new(BowAttackGoal::new(ranged_weak, 0.5, 20, 15.0)));
            goal_selector.add_goal(8, Box::new(WanderAroundGoal::new(0.6)));
            goal_selector.add_goal(
                9,
                LookAtEntityGoal::with_default(mob_weak, &EntityType::PLAYER, 3.0),
            );
            goal_selector.add_goal(10, Box::new(RandomLookAroundGoal::default()));

            let mut target_selector = mob_arc
                .mob_entity
                .target_selector
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            target_selector.add_goal(
                1,
                Box::new(
                    RevengeGoal::new(true)
                        .ignoring(|entity_type| {
                            entity_type.has_tag(&tag::EntityType::MINECRAFT_RAIDERS)
                        })
                        .alerting_others(),
                ),
            );
            target_selector.add_goal(
                2,
                ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::PLAYER, true)
                    .unseen_memory_ticks(300),
            );
            target_selector.add_goal(
                3,
                ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::VILLAGER, false)
                    .unseen_memory_ticks(300),
            );
            target_selector.add_goal(
                3,
                ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::IRON_GOLEM, false)
                    .unseen_memory_ticks(300),
            );
        };

        mob_arc
    }

    /// The bow stack in the main hand, or the off hand if the main hand is empty.
    fn bow_stack(&self) -> ItemStack {
        let equipment = self
            .mob_entity
            .living_entity
            .entity_equipment
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let main_hand = equipment.get(&EquipmentSlot::MAIN_HAND);
        if main_hand.item.id == Item::BOW.id {
            main_hand
        } else {
            equipment.get(&EquipmentSlot::OFF_HAND)
        }
    }
}

impl Mob for IllusionerEntity {
    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn as_patrolling_monster(&self) -> Option<&dyn PatrollingMonster> {
        Some(self)
    }

    fn as_raider(&self) -> Option<&dyn Raider> {
        Some(self)
    }

    fn mob_tick(&self, _caller: &dyn EntityBase) {
        self.spellcaster_data.tick();
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        self.write_raider_nbt(nbt);
        nbt.put_int("SpellTicks", self.spellcaster_data.get_spell_casting_time());
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.read_raider_nbt(nbt);
        if let Some(ticks) = nbt.get_int("SpellTicks") {
            self.spellcaster_data.set_spell_casting_time(ticks);
        }
    }
}

impl SpellcasterIllager for IllusionerEntity {
    fn spellcaster_data(&self) -> &SpellcasterIllagerData {
        &self.spellcaster_data
    }

    fn set_is_casting_spell(&self, spell: IllagerSpell) {
        self.spellcaster_data.set_current_spell(spell);
        self.mob_entity.living_entity.entity.set_synced_data(
            tracked_data::illusioner::SPELL_CASTING_ID,
            spell as u8 as i8,
        );
    }
}

impl RangedAttackMob for IllusionerEntity {
    /// Vanilla `Illusioner.performRangedAttack`, the same arrow launch as `AbstractSkeleton`.
    fn perform_ranged_attack(&self, target: &Arc<dyn EntityBase>, power: f32) {
        let entity = &self.mob_entity.living_entity.entity;
        let world = entity.world.load();
        let world_full = entity.world.load_full();

        let bow_item = self.bow_stack();
        let projectile = ItemStack::new(1, &Item::ARROW);
        let arrow_entity = Entity::new(world_full.clone(), entity.pos.load(), &EntityType::ARROW);
        let arrow = ArrowEntity::new_shot_with_weapon(
            arrow_entity,
            entity,
            &projectile,
            &bow_item,
            ArrowPickup::Disallowed,
        );
        let difficulty = world.level_info.load().difficulty as i32;
        arrow.set_base_damage_from_mob(f64::from(power), difficulty);
        arrow.set_base_damage(crate::enchantment::EnchantmentHelper::modify_damage(
            &bow_item,
            arrow.get_base_damage(),
        ));
        arrow.punch_level.store(
            crate::enchantment::EnchantmentHelper::modify_knockback(&bow_item, 0.0) as u8,
            Ordering::Relaxed,
        );
        arrow.apply_on_projectile_spawned(&projectile);

        let mob_pos = entity.pos.load();
        let target_entity = target.get_entity();
        let target_pos = target_entity.pos.load();
        let dx = target_pos.x - mob_pos.x;
        let dy = (target_pos.y + f64::from(target_entity.entity_dimension.load().height) / 3.0)
            - arrow.entity.pos.load().y;
        let dz = target_pos.z - mob_pos.z;
        let horizontal_distance = dx.hypot(dz);

        // RangedAttackMob.rangedAttackUncertainty
        let divergence = f64::from(14 - difficulty * 4);
        arrow.set_velocity(
            dx,
            horizontal_distance.mul_add(0.2, dy),
            dz,
            ARROW_SPEED,
            divergence,
        );

        world.play_sound_fine(
            Sound::EntitySkeletonShoot,
            SoundCategory::Hostile,
            &mob_pos,
            1.0,
            1.0 / (rand::random::<f32>() * 0.4 + 0.8),
        );

        let arrow: Arc<dyn EntityBase> = Arc::new(arrow);
        if let Some(server) = world_full.server.upgrade() {
            let mut event =
                crate::plugin::api::events::entity::entity_shoot_bow::EntityShootBowEvent::new(
                    entity.entity_id,
                    "minecraft:bow".to_string(),
                    1.0,
                );
            server.plugin_manager.fire_blocking(&server, &mut event);
            if event.cancelled {
                return;
            }
        }
        world_full.spawn_entity(arrow);
    }
}

impl PatrollingMonster for IllusionerEntity {
    fn get_patrol_data(&self) -> &PatrolData {
        &self.raider_data.patrol_data
    }
}

impl Raider for IllusionerEntity {
    fn get_raider_data(&self) -> &RaiderData {
        &self.raider_data
    }

    fn get_celebrate_sound(&self) -> Sound {
        Sound::EntityIllusionerAmbient
    }
}

/// Vanilla `Illusioner.IllusionerMirrorSpellGoal`: cast invisibility on itself.
pub struct IllusionerMirrorSpellGoal {
    illusioner: Weak<IllusionerEntity>,
    use_spell: SpellcasterUseSpell,
}

impl IllusionerMirrorSpellGoal {
    const CASTING_TIME: i32 = 20;
    const CASTING_INTERVAL: i32 = 340;
    const INVISIBILITY_DURATION: i32 = 1200;

    #[must_use]
    pub const fn new(illusioner: Weak<IllusionerEntity>) -> Self {
        Self {
            illusioner,
            use_spell: SpellcasterUseSpell::new(),
        }
    }
}

impl Goal for IllusionerMirrorSpellGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(illusioner) = self.illusioner.upgrade() else {
            return false;
        };
        self.use_spell
            .can_start(mob, illusioner.spellcaster_data.is_casting_spell())
            && !illusioner
                .mob_entity
                .living_entity
                .has_effect(&StatusEffect::INVISIBILITY)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        self.use_spell.can_continue(mob)
    }

    fn start(&mut self, mob: &dyn Mob) {
        let Some(illusioner) = self.illusioner.upgrade() else {
            return;
        };
        self.use_spell.start(
            mob,
            &*illusioner,
            Self::CASTING_TIME,
            Self::CASTING_INTERVAL,
            IllagerSpell::Disappear,
            Some(Sound::EntityIllusionerPrepareMirror),
        );
    }

    fn tick(&mut self, _mob: &dyn Mob) {
        if !self.use_spell.tick() {
            return;
        }
        let Some(illusioner) = self.illusioner.upgrade() else {
            return;
        };
        illusioner
            .mob_entity
            .living_entity
            .entity
            .play_sound(Sound::EntityIllusionerCastSpell);
        illusioner.mob_entity.living_entity.add_effect(Effect {
            effect_type: &StatusEffect::INVISIBILITY,
            duration: Self::INVISIBILITY_DURATION,
            amplifier: 0,
            ambient: false,
            show_particles: true,
            show_icon: true,
            blend: false,
        });
    }

    fn controls(&self) -> Controls {
        Controls::empty()
    }
}

/// Vanilla `Illusioner.IllusionerBlindnessSpellGoal`: blind the current target, once per target,
/// only on harder than Normal difficulty.
pub struct IllusionerBlindnessSpellGoal {
    illusioner: Weak<IllusionerEntity>,
    use_spell: SpellcasterUseSpell,
    last_target_id: i32,
}

impl IllusionerBlindnessSpellGoal {
    const CASTING_TIME: i32 = 20;
    const CASTING_INTERVAL: i32 = 180;
    const BLINDNESS_DURATION: i32 = 400;

    #[must_use]
    pub const fn new(illusioner: Weak<IllusionerEntity>) -> Self {
        Self {
            illusioner,
            use_spell: SpellcasterUseSpell::new(),
            last_target_id: 0,
        }
    }

    fn is_hard_enough(illusioner: &IllusionerEntity) -> bool {
        let entity = &illusioner.mob_entity.living_entity.entity;
        let world = entity.world.load();
        RegionalDifficulty::at(&world, entity.pos.load()).effective_difficulty > NORMAL_DIFFICULTY
    }
}

impl Goal for IllusionerBlindnessSpellGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(illusioner) = self.illusioner.upgrade() else {
            return false;
        };
        if !self
            .use_spell
            .can_start(mob, illusioner.spellcaster_data.is_casting_spell())
        {
            return false;
        }
        let Some(target) = illusioner.mob_entity.get_target() else {
            return false;
        };
        target.get_entity().entity_id != self.last_target_id && Self::is_hard_enough(&illusioner)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        self.use_spell.can_continue(mob)
    }

    fn start(&mut self, mob: &dyn Mob) {
        let Some(illusioner) = self.illusioner.upgrade() else {
            return;
        };
        self.use_spell.start(
            mob,
            &*illusioner,
            Self::CASTING_TIME,
            Self::CASTING_INTERVAL,
            IllagerSpell::Blindness,
            Some(Sound::EntityIllusionerPrepareBlindness),
        );
        if let Some(target) = illusioner.mob_entity.get_target() {
            self.last_target_id = target.get_entity().entity_id;
        }
    }

    fn tick(&mut self, _mob: &dyn Mob) {
        if !self.use_spell.tick() {
            return;
        }
        let Some(illusioner) = self.illusioner.upgrade() else {
            return;
        };
        illusioner
            .mob_entity
            .living_entity
            .entity
            .play_sound(Sound::EntityIllusionerCastSpell);
        if let Some(target) = illusioner.mob_entity.get_target()
            && let Some(living) = target.get_living_entity()
        {
            living.add_effect(Effect {
                effect_type: &StatusEffect::BLINDNESS,
                duration: Self::BLINDNESS_DURATION,
                amplifier: 0,
                ambient: false,
                show_particles: true,
                show_icon: true,
                blend: false,
            });
        }
    }

    fn controls(&self) -> Controls {
        Controls::empty()
    }
}
