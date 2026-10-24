use std::sync::Mutex;
use std::sync::{Arc, Weak};
use uuid::Uuid;

use pumpkin_data::entity::EntityType;
use pumpkin_data::sound::Sound;
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::position::BlockPos;
use pumpkin_util::math::vector3::Vector3;

use crate::entity::{
    Entity, EntityBase,
    ai::goal::{
        Goal, active_target::ActiveTargetGoal, look_around::RandomLookAroundGoal,
        look_at_entity::LookAtEntityGoal, swim::SwimGoal, wander_around::WanderAroundGoal,
    },
    mob::{
        Mob, MobEntity,
        patrol::{PatrolData, PatrollingMonster},
        raider::{
            ObtainRaidLeaderBannerGoal, PathfindToRaidGoal, Raider, RaiderCelebrationGoal,
            RaiderData, RaiderMoveThroughVillageGoal,
        },
        spellcaster_illager::{
            SpellcasterCastingSpellGoal, SpellcasterIllager, SpellcasterIllagerData,
            SpellcasterUseSpell,
        },
        vex::VexEntity,
    },
    projectile::evoker_fangs::EvokerFangsEntity,
    r#type::from_type,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum IllagerSpell {
    None = 0,
    SummonVex = 1,
    Fangs = 2,
    Wololo = 3,
    Disappear = 4,
    Blindness = 5,
}

impl IllagerSpell {
    #[must_use]
    pub const fn from_u8(val: u8) -> Self {
        match val {
            1 => Self::SummonVex,
            2 => Self::Fangs,
            3 => Self::Wololo,
            4 => Self::Disappear,
            5 => Self::Blindness,
            _ => Self::None,
        }
    }
}

pub struct EvokerEntity {
    pub mob_entity: MobEntity,
    pub raider_data: RaiderData,
    pub spellcaster_data: SpellcasterIllagerData,
    wololo_target_id: Arc<Mutex<Option<i32>>>,
}

impl EvokerEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let evoker = Self {
            mob_entity,
            raider_data: RaiderData::default(),
            spellcaster_data: SpellcasterIllagerData::new(),
            wololo_target_id: Arc::new(Mutex::new(None)),
        };
        let mob_arc = Arc::new(evoker);
        let mob_weak: Weak<Self> = Arc::downgrade(&mob_arc);
        let mob_interface_weak: Weak<dyn Mob> = {
            let mob_dyn: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_dyn)
        };
        let caster_weak: Weak<dyn SpellcasterIllager> = {
            let caster: Arc<dyn SpellcasterIllager> = mob_arc.clone();
            Arc::downgrade(&caster)
        };

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;

            goal_selector.add_goal(0, Box::new(SwimGoal::default()));
            goal_selector.add_goal(1, Box::new(SpellcasterCastingSpellGoal::new(caster_weak)));
            goal_selector.add_goal(2, Box::new(ObtainRaidLeaderBannerGoal));
            goal_selector.add_goal(3, Box::new(RaiderMoveThroughVillageGoal::new(1.05)));
            goal_selector.add_goal(3, Box::new(PathfindToRaidGoal::default()));
            goal_selector.add_goal(4, Box::new(EvokerSummonSpellGoal::new(mob_weak.clone())));
            goal_selector.add_goal(5, Box::new(EvokerAttackSpellGoal::new(mob_weak.clone())));
            goal_selector.add_goal(6, Box::new(EvokerWololoSpellGoal::new(mob_weak)));
            goal_selector.add_goal(7, Box::new(RaiderCelebrationGoal));
            goal_selector.add_goal(8, Box::new(WanderAroundGoal::new(0.6)));
            goal_selector.add_goal(
                9,
                LookAtEntityGoal::with_default(mob_interface_weak, &EntityType::PLAYER, 3.0),
            );
            goal_selector.add_goal(10, Box::new(RandomLookAroundGoal::default()));

            let target_selector = &mob_arc.mob_entity.target_selector;
            target_selector.add_goal(
                1,
                ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::PLAYER, true),
            );
            target_selector.add_goal(
                2,
                ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::VILLAGER, true),
            );
            target_selector.add_goal(
                3,
                ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::IRON_GOLEM, true),
            );
        };

        mob_arc
    }

    pub fn is_casting_spell(&self) -> bool {
        self.spellcaster_data.is_casting_spell()
    }
}

impl SpellcasterIllager for EvokerEntity {
    fn spellcaster_data(&self) -> &SpellcasterIllagerData {
        &self.spellcaster_data
    }

    fn set_is_casting_spell(&self, spell: IllagerSpell) {
        self.spellcaster_data.set_current_spell(spell);
        self.mob_entity.living_entity.entity.set_synced_data(
            pumpkin_data::tracked_data::evoker::SPELL_CASTING_ID,
            spell as u8 as i8,
        );
    }
}

impl Mob for EvokerEntity {
    fn as_patrolling_monster(&self) -> Option<&dyn PatrollingMonster> {
        Some(self)
    }

    fn as_raider(&self) -> Option<&dyn Raider> {
        Some(self)
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

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn mob_tick(&self, _caller: &dyn EntityBase) {
        self.spellcaster_data.tick();
    }
}

impl PatrollingMonster for EvokerEntity {
    fn get_patrol_data(&self) -> &PatrolData {
        &self.raider_data.patrol_data
    }
}

impl Raider for EvokerEntity {
    fn get_raider_data(&self) -> &RaiderData {
        &self.raider_data
    }

    fn get_celebrate_sound(&self) -> Sound {
        Sound::EntityEvokerCelebrate
    }
}

pub struct EvokerAttackSpellGoal {
    evoker: Weak<EvokerEntity>,
    use_spell: SpellcasterUseSpell,
}

impl EvokerAttackSpellGoal {
    const CASTING_TIME: i32 = 40;
    const CASTING_INTERVAL: i32 = 100;

    #[must_use]
    pub const fn new(evoker: Weak<EvokerEntity>) -> Self {
        Self {
            evoker,
            use_spell: SpellcasterUseSpell::new(),
        }
    }
}

impl Goal for EvokerAttackSpellGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(evoker) = self.evoker.upgrade() else {
            return false;
        };
        self.use_spell.can_start(mob, evoker.is_casting_spell())
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        self.use_spell.can_continue(mob)
    }

    fn start(&mut self, mob: &dyn Mob) {
        let Some(evoker) = self.evoker.upgrade() else {
            return;
        };
        self.use_spell.start(
            mob,
            &*evoker,
            Self::CASTING_TIME,
            Self::CASTING_INTERVAL,
            IllagerSpell::Fangs,
            Some(Sound::EntityEvokerPrepareAttack),
        );
    }

    fn tick(&mut self, _mob: &dyn Mob) {
        if !self.use_spell.tick() {
            return;
        }
        let Some(evoker) = self.evoker.upgrade() else {
            return;
        };
        let target = evoker.mob_entity.get_target();
        let Some(target) = target else {
            return;
        };

        let evoker_ent = &evoker.mob_entity.living_entity.entity;
        evoker_ent.play_sound(Sound::EntityEvokerCastSpell);

        let evoker_pos = evoker_ent.pos.load();
        let target_pos = target.get_entity().pos.load();

        let dx = target_pos.x - evoker_pos.x;
        let dz = target_pos.z - evoker_pos.z;
        let angle_towards_target = (dz.atan2(dx)) as f32;

        let min_y = evoker_pos.y.min(target_pos.y);
        let dist_sq = evoker_pos.squared_distance_to_vec(&target_pos);
        let world = evoker_ent.world.load();
        let evoker_id = evoker_ent.entity_id;

        if dist_sq < 81.0 {
            // Close range: concentric rings around Evoker
            for i in 0..5 {
                let angle = angle_towards_target + (i as f32) * std::f32::consts::PI * 0.4;
                let spawn_x = evoker_pos.x + (angle.cos() as f64) * 1.5;
                let spawn_z = evoker_pos.z + (angle.sin() as f64) * 1.5;
                let pos = Vector3::new(spawn_x, min_y, spawn_z);

                let entity = Entity::from_uuid(
                    Uuid::new_v4(),
                    world.clone(),
                    pos,
                    &EntityType::EVOKER_FANGS,
                );
                let fangs = Arc::new(EvokerFangsEntity::new(entity, 0, angle, Some(evoker_id)));
                world.spawn_entity(fangs);
            }

            for i in 0..8 {
                let angle = angle_towards_target
                    + (i as f32) * std::f32::consts::PI * 2.0 / 8.0
                    + 1.256_637_1;
                let spawn_x = evoker_pos.x + (angle.cos() as f64) * 2.5;
                let spawn_z = evoker_pos.z + (angle.sin() as f64) * 2.5;
                let pos = Vector3::new(spawn_x, min_y, spawn_z);

                let entity = Entity::from_uuid(
                    Uuid::new_v4(),
                    world.clone(),
                    pos,
                    &EntityType::EVOKER_FANGS,
                );
                let fangs = Arc::new(EvokerFangsEntity::new(entity, 3, angle, Some(evoker_id)));
                world.spawn_entity(fangs);
            }
        } else {
            // Long range: line of fangs towards target
            for i in 0..16 {
                let reach = 1.25 * ((i + 1) as f64);
                let spawn_x = evoker_pos.x + (angle_towards_target.cos() as f64) * reach;
                let spawn_z = evoker_pos.z + (angle_towards_target.sin() as f64) * reach;
                let pos = Vector3::new(spawn_x, min_y, spawn_z);

                let entity = Entity::from_uuid(
                    Uuid::new_v4(),
                    world.clone(),
                    pos,
                    &EntityType::EVOKER_FANGS,
                );
                let fangs = Arc::new(EvokerFangsEntity::new(
                    entity,
                    i as u32,
                    angle_towards_target,
                    Some(evoker_id),
                ));
                world.spawn_entity(fangs);
            }
        }
    }
}

pub struct EvokerSummonSpellGoal {
    evoker: Weak<EvokerEntity>,
    use_spell: SpellcasterUseSpell,
}

impl EvokerSummonSpellGoal {
    const CASTING_TIME: i32 = 100;
    const CASTING_INTERVAL: i32 = 340;

    #[must_use]
    pub const fn new(evoker: Weak<EvokerEntity>) -> Self {
        Self {
            evoker,
            use_spell: SpellcasterUseSpell::new(),
        }
    }
}

impl Goal for EvokerSummonSpellGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(evoker) = self.evoker.upgrade() else {
            return false;
        };
        if !self.use_spell.can_start(mob, evoker.is_casting_spell()) {
            return false;
        }

        // Count nearby Vexes
        let entity = &evoker.mob_entity.living_entity.entity;
        let bb = entity.bounding_box.load().expand(16.0, 16.0, 16.0);
        let world = entity.world.load();
        let nearby = world.get_entities_at_box(&bb);
        let vex_count = nearby
            .iter()
            .filter(|e| *e.get_entity().entity_type == EntityType::VEX)
            .count();

        let max_allowed = (rand::random::<u8>() % 8 + 1) as usize;
        vex_count < max_allowed
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        self.use_spell.can_continue(mob)
    }

    fn start(&mut self, mob: &dyn Mob) {
        let Some(evoker) = self.evoker.upgrade() else {
            return;
        };
        self.use_spell.start(
            mob,
            &*evoker,
            Self::CASTING_TIME,
            Self::CASTING_INTERVAL,
            IllagerSpell::SummonVex,
            Some(Sound::EntityEvokerPrepareSummon),
        );
    }

    fn tick(&mut self, _mob: &dyn Mob) {
        if !self.use_spell.tick() {
            return;
        }
        let Some(evoker) = self.evoker.upgrade() else {
            return;
        };
        let evoker_ent = &evoker.mob_entity.living_entity.entity;
        evoker_ent.play_sound(Sound::EntityEvokerCastSpell);

        let world = evoker_ent.world.load();
        let evoker_block = evoker_ent.block_pos.load();
        let evoker_uuid = evoker_ent.entity_uuid;

        for _ in 0..3 {
            let bound_origin = BlockPos::new(
                evoker_block.0.x + rand::random_range(0..5) - 2,
                evoker_block.0.y + 1,
                evoker_block.0.z + rand::random_range(0..5) - 2,
            );
            let spawn_pos = Vector3::new(
                f64::from(bound_origin.0.x) + 0.5,
                f64::from(bound_origin.0.y),
                f64::from(bound_origin.0.z) + 0.5,
            );

            let vex = from_type(&EntityType::VEX, spawn_pos, &world, Uuid::new_v4());
            if let Some(vex_entity) = vex.cast_any().downcast_ref::<VexEntity>() {
                vex_entity.set_owner(Some(evoker_uuid));
                vex_entity.set_bound_origin(Some(bound_origin));
                vex_entity.set_limited_life(20 * (30 + rand::random_range(0..90)));
            }
            world.spawn_entity(vex);
        }
    }
}

pub struct EvokerWololoSpellGoal {
    evoker: Weak<EvokerEntity>,
    use_spell: SpellcasterUseSpell,
}

impl EvokerWololoSpellGoal {
    /// Vanilla `EvokerWololoSpellGoal.getCastWarmupTime`.
    const CAST_WARMUP_TIME: i32 = 40;
    const CASTING_TIME: i32 = 60;
    const CASTING_INTERVAL: i32 = 140;

    #[must_use]
    pub const fn new(evoker: Weak<EvokerEntity>) -> Self {
        Self {
            evoker,
            use_spell: SpellcasterUseSpell::with_cast_warmup_time(Self::CAST_WARMUP_TIME),
        }
    }
}

impl Goal for EvokerWololoSpellGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(evoker) = self.evoker.upgrade() else {
            return false;
        };
        // Vanilla `EvokerWololoSpellGoal.canUse` also rules out casting while it has a target.
        if evoker.mob_entity.get_target().is_some() {
            return false;
        }
        if !self
            .use_spell
            .can_start_ignoring_target(mob, evoker.is_casting_spell())
        {
            return false;
        }

        // Find blue sheep within 16 blocks
        let entity = &evoker.mob_entity.living_entity.entity;
        let bb = entity.bounding_box.load().expand(16.0, 4.0, 16.0);
        let world = entity.world.load();
        let candidates = world.get_entities_at_box(&bb);

        for cand in candidates {
            if *cand.get_entity().entity_type == EntityType::SHEEP
                && let Some(sheep) = cand
                    .cast_any()
                    .downcast_ref::<crate::entity::passive::sheep::SheepEntity>()
            {
                // Blue color is 11 in Minecraft
                if sheep.get_color() == 11 {
                    *evoker
                        .wololo_target_id
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) =
                        Some(cand.get_entity().entity_id);
                    return true;
                }
            }
        }

        false
    }

    fn should_continue(&mut self, _mob: &dyn Mob) -> bool {
        self.use_spell.is_warming_up()
    }

    fn start(&mut self, mob: &dyn Mob) {
        let Some(evoker) = self.evoker.upgrade() else {
            return;
        };
        self.use_spell.start(
            mob,
            &*evoker,
            Self::CASTING_TIME,
            Self::CASTING_INTERVAL,
            IllagerSpell::Wololo,
            Some(Sound::EntityEvokerPrepareWololo),
        );
    }

    fn stop(&mut self, _mob: &dyn Mob) {
        if let Some(evoker) = self.evoker.upgrade() {
            *evoker
                .wololo_target_id
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        }
    }

    fn tick(&mut self, _mob: &dyn Mob) {
        if !self.use_spell.tick() {
            return;
        }
        let Some(evoker) = self.evoker.upgrade() else {
            return;
        };
        let evoker_ent = &evoker.mob_entity.living_entity.entity;
        evoker_ent.play_sound(Sound::EntityEvokerCastSpell);

        let target_id = *evoker
            .wololo_target_id
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(target_id) = target_id else {
            return;
        };

        let world = evoker_ent.world.load();
        let bb = evoker_ent.bounding_box.load().expand(16.0, 4.0, 16.0);
        let candidates = world.get_entities_at_box(&bb);

        for cand in candidates {
            if cand.get_entity().entity_id == target_id
                && let Some(sheep) = cand
                    .cast_any()
                    .downcast_ref::<crate::entity::passive::sheep::SheepEntity>()
            {
                // Convert color to Red (14)
                sheep.set_color(14);
                break;
            }
        }
    }
}
