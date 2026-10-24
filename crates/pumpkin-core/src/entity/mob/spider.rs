use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};

use pumpkin_data::effect::StatusEffect;
use pumpkin_data::entity::EntityType;
use pumpkin_data::potion::Effect;
use pumpkin_util::difficulty::Difficulty;
use rand::RngExt;
use uuid::Uuid;

use crate::entity::{
    Entity, EntityBase,
    ai::goal::{
        active_target::{ActiveTargetGoal, TargetCondition},
        avoid_entity::AvoidEntityGoal,
        leap_at_target::LeapAtTargetGoal,
        look_around::RandomLookAroundGoal,
        look_at_entity::LookAtEntityGoal,
        revenge::RevengeGoal,
        spider_attack::SpiderAttackGoal,
        swim::SwimGoal,
        wander_around::WanderAroundGoal,
    },
    ai::pathfinder::Navigator,
    mob::{
        Mob, MobEntity,
        equipment::RegionalDifficulty,
        spawn::{SpawnGroupData, finalize_spawn},
    },
    passive::armadillo::ArmadilloEntity,
    r#type::from_type,
};
use crate::world::World;

pub struct SpiderEntity {
    pub mob_entity: MobEntity,
    pub is_climbing: AtomicBool,
}

/// Vanilla `Spider.registerGoals` and `createNavigation` (also used by the cave spider).
pub fn register_spider_goals(mob_entity: &MobEntity, mob_weak: Weak<dyn Mob>) {
    *mob_entity
        .navigator
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Navigator::wall_climber();

    let goal_selector = &mob_entity.goals_selector;
    let target_selector = &mob_entity.target_selector;

    goal_selector.add_goal(1, Box::new(SwimGoal::default()));
    goal_selector.add_goal(
        2,
        Box::new(
            AvoidEntityGoal::new(&EntityType::ARMADILLO, 6.0, 1.0, 1.2).with_filter(|entity| {
                entity
                    .cast_any()
                    .downcast_ref::<ArmadilloEntity>()
                    .is_some_and(|armadillo| !armadillo.is_scared())
            }),
        ),
    );
    goal_selector.add_goal(3, Box::new(LeapAtTargetGoal::new(0.4)));
    goal_selector.add_goal(4, SpiderAttackGoal::new(1.0, true));
    goal_selector.add_goal(5, Box::new(WanderAroundGoal::new(0.8)));
    goal_selector.add_goal(
        6,
        LookAtEntityGoal::with_default(mob_weak, &EntityType::PLAYER, 8.0),
    );
    goal_selector.add_goal(6, Box::new(RandomLookAroundGoal::default()));

    target_selector.add_goal(1, Box::new(RevengeGoal::new(true)));
    target_selector.add_goal(
        2,
        ActiveTargetGoal::with_default(mob_entity, &EntityType::PLAYER, true)
            .when(TargetCondition::NoDaylight),
    );
    target_selector.add_goal(
        3,
        ActiveTargetGoal::with_default(mob_entity, &EntityType::IRON_GOLEM, true)
            .when(TargetCondition::NoDaylight),
    );
}

/// Vanilla `Spider.tick`: the climbing flag follows horizontal collision. `onClimbable` is
/// the same flag, so it also lets the living entity climb.
pub fn tick_climbing(mob: &dyn Mob, is_climbing: &AtomicBool) {
    let mob_entity = mob.get_mob_entity();
    let entity = &mob_entity.living_entity.entity;
    let climbing = entity.horizontal_collision.load(Ordering::Relaxed);
    if is_climbing.swap(climbing, Ordering::Relaxed) != climbing {
        entity.set_synced_data(
            pumpkin_data::tracked_data::spider::DATA_FLAGS_ID,
            i8::from(climbing),
        );
        if let Some(meta) = mob.mob_metadata() {
            entity.send_bedrock_actor_data(&meta.to_bedrock_actor_data());
        }
    }
    mob_entity
        .living_entity
        .climbs_walls
        .store(climbing, Ordering::Relaxed);
}

impl SpiderEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let spider = Self {
            mob_entity,
            is_climbing: AtomicBool::new(false),
        };
        let mob_arc = Arc::new(spider);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };
        register_spider_goals(&mob_arc.mob_entity, mob_weak);
        mob_arc
    }

    pub fn is_climbing(&self) -> bool {
        self.is_climbing.load(Ordering::Relaxed)
    }
}

/// Vanilla `Spider.finalizeSpawn` (also used by the cave spider): a skeleton jockey,
/// and on hard difficulty a random effect shared by the whole spawn group.
pub fn finalize_spider_spawn(
    mob: &MobEntity,
    world: &Arc<World>,
    group_data: Option<SpawnGroupData>,
) -> Option<SpawnGroupData> {
    mob.finalize_spawn_base();
    let mut rng = rand::rng();
    let entity = &mob.living_entity.entity;
    let pos = entity.pos.load();

    if rng.random_range(0..100) == 0 {
        let skeleton = from_type(&EntityType::SKELETON, pos, world, Uuid::new_v4());
        skeleton.get_entity().set_rotation(entity.yaw.load(), 0.0);
        finalize_spawn(&skeleton, world, None);
        mob.add_pending_rider(skeleton);
    }

    let group_data = group_data.unwrap_or_else(|| {
        let difficulty = RegionalDifficulty::at(world, pos);
        let effect = (difficulty.base_difficulty == Difficulty::Hard
            && rng.random::<f32>() < 0.1 * difficulty.special_multiplier)
            .then(|| match rng.random_range(0..5) {
                0 | 1 => &StatusEffect::SPEED,
                2 => &StatusEffect::STRENGTH,
                3 => &StatusEffect::REGENERATION,
                _ => &StatusEffect::INVISIBILITY,
            });
        SpawnGroupData::SpiderEffects(effect)
    });

    if let SpawnGroupData::SpiderEffects(Some(effect_type)) = &group_data {
        mob.living_entity.add_effect(Effect {
            effect_type,
            duration: -1,
            amplifier: 0,
            ambient: false,
            show_particles: true,
            show_icon: true,
            blend: false,
        });
    }
    Some(group_data)
}

impl Mob for SpiderEntity {
    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        finalize_spider_spawn(&self.mob_entity, world, group_data)
    }

    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn mob_metadata(&self) -> Option<crate::entity::mob::MobMetadata> {
        let mut meta = crate::entity::mob::MobMetadata::new();
        if self.is_climbing() {
            meta = meta.spider_climbing(true);
        }
        Some(meta)
    }

    fn mob_tick(&self, _caller: &dyn EntityBase) {
        if self.mob_entity.living_entity.entity.is_alive() {
            tick_climbing(self, &self.is_climbing);
        }
    }
}
