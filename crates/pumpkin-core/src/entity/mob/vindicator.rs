use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};

use pumpkin_data::entity::EntityType;
use pumpkin_data::sound::Sound;
use pumpkin_data::tag::{self, Taggable};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::Difficulty;
use pumpkin_util::text::TextComponent;
use rand::RngExt;

use crate::entity::{
    Entity, EntityBase,
    ai::goal::{
        Controls, Goal, active_target::ActiveTargetGoal, avoid_entity::AvoidEntityGoal,
        break_door::BreakDoorGoal, look_around::RandomLookAroundGoal,
        look_at_entity::LookAtEntityGoal, melee_attack::MeleeAttackGoal, open_door::OpenDoorGoal,
        revenge::RevengeGoal, swim::SwimGoal, to_goal_ticks, wander_around::WanderAroundGoal,
    },
    mob::{
        Mob, MobEntity,
        patrol::{LongDistancePatrolGoal, PatrolData, PatrollingMonster},
        raider::{
            HoldGroundAttackGoal, ObtainRaidLeaderBannerGoal, PathfindToRaidGoal, Raider,
            RaiderCelebrationGoal, RaiderData, RaiderMoveThroughVillageGoal,
        },
    },
};

pub struct VindicatorEntity {
    pub mob_entity: MobEntity,
    pub raider_data: RaiderData,
    johnny: AtomicBool,
}

impl VindicatorEntity {
    #[must_use]
    pub fn new(entity: Entity) -> Arc<Self> {
        let mob_entity = MobEntity::new(entity);
        let vindicator = Self {
            mob_entity,
            raider_data: RaiderData::default(),
            johnny: AtomicBool::new(false),
        };
        let mob_arc = Arc::new(vindicator);
        let mob_weak: Weak<dyn Mob> = {
            let mob_arc: Arc<dyn Mob> = mob_arc.clone();
            Arc::downgrade(&mob_arc)
        };

        {
            let goal_selector = &mob_arc.mob_entity.goals_selector;

            goal_selector.add_goal(0, Box::new(SwimGoal::default()));
            goal_selector.add_goal(1, Box::new(ObtainRaidLeaderBannerGoal));
            goal_selector.add_goal(
                1,
                Box::new(AvoidEntityGoal::new(&EntityType::CREAKING, 8.0, 1.0, 1.2)),
            );
            goal_selector.add_goal(2, Box::new(VindicatorBreakDoorGoal::default()));
            goal_selector.add_goal(3, Box::new(PathfindToRaidGoal::default()));
            goal_selector.add_goal(3, Box::new(RaiderOpenDoorGoal::default()));
            goal_selector.add_goal(4, Box::new(LongDistancePatrolGoal::new(0.7, 0.595)));
            goal_selector.add_goal(4, Box::new(RaiderMoveThroughVillageGoal::new(1.05)));
            goal_selector.add_goal(4, Box::new(HoldGroundAttackGoal::new(10.0)));
            goal_selector.add_goal(5, Box::new(RaiderCelebrationGoal));
            goal_selector.add_goal(5, Box::new(MeleeAttackGoal::new(1.0, false)));
            goal_selector.add_goal(8, Box::new(WanderAroundGoal::new(0.6)));
            goal_selector.add_goal(
                9,
                LookAtEntityGoal::with_default(mob_weak, &EntityType::PLAYER, 3.0),
            );
            goal_selector.add_goal(10, Box::new(RandomLookAroundGoal::default()));

            let target_selector = &mob_arc.mob_entity.target_selector;
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
                ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::PLAYER, true),
            );
            target_selector.add_goal(
                3,
                ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::VILLAGER, true),
            );
            target_selector.add_goal(
                3,
                ActiveTargetGoal::with_default(&mob_arc.mob_entity, &EntityType::IRON_GOLEM, true),
            );
            target_selector.add_goal(
                4,
                Box::new(VindicatorJohnnyAttackGoal::new(
                    &mob_arc.mob_entity,
                    Arc::downgrade(&mob_arc),
                )),
            );
        };

        mob_arc
    }

    #[must_use]
    pub fn is_johnny(&self) -> bool {
        self.johnny.load(Ordering::Relaxed)
    }

    /// Vanilla `Vindicator.setCustomName`: any custom name of "Johnny" flips the flag, not just a
    /// name tag.
    fn check_johnny_name(&self, name: &TextComponent) {
        if !self.is_johnny() && name.clone().get_text() == "Johnny" {
            self.johnny.store(true, Ordering::Relaxed);
        }
    }
}

impl Mob for VindicatorEntity {
    fn get_mob_entity(&self) -> &MobEntity {
        &self.mob_entity
    }

    fn as_patrolling_monster(&self) -> Option<&dyn PatrollingMonster> {
        Some(self)
    }

    fn as_raider(&self) -> Option<&dyn Raider> {
        Some(self)
    }

    fn on_custom_name_set(&self, name: &TextComponent) {
        self.check_johnny_name(name);
    }

    fn custom_server_ai_step(&self, _caller: &dyn EntityBase) {
        let entity = &self.mob_entity.living_entity.entity;
        let world = entity.world.load();
        let can_open_doors = world.is_raided(&entity.block_pos.load());
        let mut navigator = self
            .mob_entity
            .navigator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if navigator.can_navigate_ground() {
            navigator.set_can_open_doors(can_open_doors);
        }
    }

    fn mob_write_nbt(&self, nbt: &mut NbtCompound) {
        self.write_raider_nbt(nbt);
        if self.is_johnny() {
            nbt.put_bool("Johnny", true);
        }
    }

    fn mob_read_nbt(&self, nbt: &NbtCompound) {
        self.read_raider_nbt(nbt);
        if let Some(johnny) = nbt.get_bool("Johnny") {
            self.johnny.store(johnny, Ordering::Relaxed);
        }
        // `Entity::read_nbt_non_mut` sets the custom name before `mob_read_nbt`, so a saved or
        // `/summon`ed "Johnny" name flips the flag here too.
        if let Some(name) = &**self.mob_entity.living_entity.entity.custom_name.load() {
            self.check_johnny_name(name);
        }
    }
}

impl PatrollingMonster for VindicatorEntity {
    fn get_patrol_data(&self) -> &PatrolData {
        &self.raider_data.patrol_data
    }
}

impl Raider for VindicatorEntity {
    fn get_raider_data(&self) -> &RaiderData {
        &self.raider_data
    }

    fn get_celebrate_sound(&self) -> Sound {
        Sound::EntityVindicatorCelebrate
    }
}

/// Vanilla `Vindicator.VindicatorBreakDoorGoal`: breaks doors only during an active raid and on
/// Normal or Hard difficulty.
///
/// It passes 6 for the door break time, but `BreakDoorGoal` clamps it to at least 240
/// (`Math.max(240, doorBreakTime)`), so it still takes 240 ticks like a zombie.
pub struct VindicatorBreakDoorGoal {
    inner: BreakDoorGoal,
}

impl VindicatorBreakDoorGoal {
    const DOOR_BREAK_TIME: i32 = 6;

    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: BreakDoorGoal::with_door_break_time(
                Self::DOOR_BREAK_TIME,
                Arc::new(|difficulty| {
                    difficulty == Difficulty::Normal || difficulty == Difficulty::Hard
                }),
            ),
        }
    }
}

impl Default for VindicatorBreakDoorGoal {
    fn default() -> Self {
        Self::new()
    }
}

impl Goal for VindicatorBreakDoorGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        let Some(raider) = mob.as_raider() else {
            return false;
        };
        raider.has_active_raid()
            // Vanilla `random.nextInt(reducedTickDelay(10)) == 0`, i.e. 1 in 5.
            && mob.get_random().random_range(0..to_goal_ticks(10)) == 0
            && self.inner.can_start(mob)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        mob.as_raider().is_some_and(Raider::has_active_raid) && self.inner.should_continue(mob)
    }

    fn start(&mut self, mob: &dyn Mob) {
        self.inner.start(mob);
        // TODO: Vanilla `Vindicator.VindicatorBreakDoorGoal.start` also clears `noActionTime`,
        // which Pumpkin does not model.
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.inner.stop(mob);
    }

    fn tick(&mut self, mob: &dyn Mob) {
        self.inner.tick(mob);
    }

    fn should_run_every_tick(&self) -> bool {
        self.inner.should_run_every_tick()
    }

    fn controls(&self) -> Controls {
        Controls::MOVE
    }
}

/// Vanilla `AbstractIllager.RaiderOpenDoorGoal`: the plain open-door goal, but only while
/// the raider has an active raid.
pub struct RaiderOpenDoorGoal {
    inner: OpenDoorGoal,
}

impl RaiderOpenDoorGoal {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            inner: OpenDoorGoal::new(false),
        }
    }
}

impl Default for RaiderOpenDoorGoal {
    fn default() -> Self {
        Self::new()
    }
}

impl Goal for RaiderOpenDoorGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        mob.as_raider().is_some_and(Raider::has_active_raid) && self.inner.can_start(mob)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        self.inner.should_continue(mob)
    }

    fn start(&mut self, mob: &dyn Mob) {
        self.inner.start(mob);
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.inner.stop(mob);
    }

    fn tick(&mut self, mob: &dyn Mob) {
        self.inner.tick(mob);
    }

    fn should_run_every_tick(&self) -> bool {
        self.inner.should_run_every_tick()
    }

    fn controls(&self) -> Controls {
        self.inner.controls()
    }
}

/// Vanilla `Vindicator.VindicatorJohnnyAttackGoal`: a Vindicator named "Johnny" targets every
/// attackable living entity except other illagers, and only while the flag is set.
pub struct VindicatorJohnnyAttackGoal {
    inner: Box<ActiveTargetGoal>,
    vindicator: Weak<VindicatorEntity>,
}

impl VindicatorJohnnyAttackGoal {
    #[must_use]
    pub fn new(mob: &MobEntity, vindicator: Weak<VindicatorEntity>) -> Self {
        Self {
            inner: ActiveTargetGoal::predicated(mob, 0, true, |target, _world| {
                target.is_attackable()
                    && !target
                        .get_entity()
                        .entity_type
                        .has_tag(&tag::EntityType::MINECRAFT_ILLAGER_FRIENDS)
            })
            .including_players(),
            vindicator,
        }
    }
}

impl Goal for VindicatorJohnnyAttackGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        self.vindicator.upgrade().is_some_and(|v| v.is_johnny()) && self.inner.can_start(mob)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        self.inner.should_continue(mob)
    }

    fn start(&mut self, mob: &dyn Mob) {
        self.inner.start(mob);
        // TODO: Vanilla `Vindicator.VindicatorJohnnyAttackGoal.start` clears `noActionTime`.
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.inner.stop(mob);
    }

    fn controls(&self) -> Controls {
        self.inner.controls()
    }
}
