use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};

use pumpkin_data::damage::DamageType;
use pumpkin_data::entity::EntityType;
use pumpkin_data::entity_status::EntityStatus;
use pumpkin_data::{Block, BlockDirection, BlockStateId};
use pumpkin_util::math::{position::BlockPos, vector3::Vector3};
use pumpkin_world::world::BlockFlags;
use rand::RngExt;

use crate::block::{SpawnAfterBreakArgs, drop_loot};
use crate::entity::{
    Entity, EntityBase,
    ai::goal::{
        Controls, Goal, active_target::ActiveTargetGoal,
        climb_on_top_of_powder_snow::ClimbOnTopOfPowderSnowGoal, melee_attack::MeleeAttackGoal,
        revenge::RevengeGoal, swim::SwimGoal, to_goal_ticks, wander_around::WanderAroundGoal,
    },
    mob::{Mob, MobEntity},
};
use crate::world::loot::LootContextParameters;

const INFESTED_PREFIX: &str = "infested_";
const WAKE_UP_FRIENDS_DELAY: i32 = 20;
const WAKE_UP_RANGE_XZ: i32 = 10;
const WAKE_UP_RANGE_Y: i32 = 5;

/// Vanilla `InfestedBlock.isCompatibleHostBlock` + `infestedStateByHost`: every host block has
/// an `infested_` twin, and the block properties carry over (deepslate keeps its axis).
fn infested_state_by_host(host: BlockStateId) -> Option<BlockStateId> {
    let host_block = Block::from_state_id(host);
    let infested = Block::from_name(&format!("{INFESTED_PREFIX}{}", host_block.name))?;
    let props = host_block
        .properties(host)
        .map(|properties| properties.to_props())
        .unwrap_or_default();
    Some(infested.from_properties(&props).to_state_id(infested))
}

/// Vanilla `InfestedBlock.hostStateByInfested`; `None` when the block is not an infested block.
fn host_state_by_infested(infested: BlockStateId) -> Option<BlockStateId> {
    let infested_block = Block::from_state_id(infested);
    let host = Block::from_name(infested_block.name.strip_prefix(INFESTED_PREFIX)?)?;
    let props = infested_block
        .properties(infested)
        .map(|properties| properties.to_props())
        .unwrap_or_default();
    Some(host.from_properties(&props).to_state_id(host))
}

/// 0, 1, -1, 2, -2, ... up to `max`, the order the wake up scan visits offsets in.
fn alternating_offsets(max: i32) -> impl Iterator<Item = i32> {
    std::iter::once(0).chain((1..=max).flat_map(|offset| [offset, -offset]))
}

pub struct SilverfishEntity {
    pub entity: Arc<MobEntity>,
    look_for_friends: Arc<AtomicI32>,
}

impl SilverfishEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let entity = Arc::new(MobEntity::new(entity));
        let silverfish = Self {
            entity,
            look_for_friends: Arc::new(AtomicI32::new(0)),
        };
        let mob_arc = Arc::new(silverfish);

        {
            let goal_selector = &mob_arc.entity.goals_selector;
            let target_selector = &mob_arc.entity.target_selector;

            goal_selector.add_goal(1, Box::new(SwimGoal::default()));
            goal_selector.add_goal(1, Box::new(ClimbOnTopOfPowderSnowGoal));
            goal_selector.add_goal(
                3,
                Box::new(WakeUpFriendsGoal {
                    look_for_friends: mob_arc.look_for_friends.clone(),
                }),
            );
            goal_selector.add_goal(4, Box::new(MeleeAttackGoal::new(1.0, false)));
            goal_selector.add_goal(5, Box::new(MergeWithStoneGoal::new()));

            target_selector.add_goal(1, Box::new(RevengeGoal::new(true)));
            target_selector.add_goal(
                2,
                ActiveTargetGoal::with_default(&mob_arc.entity, &EntityType::PLAYER, true),
            );
        };

        mob_arc
    }
}

/// Vanilla `Silverfish.SilverfishMergeWithStoneGoal`.
struct MergeWithStoneGoal {
    wander: WanderAroundGoal,
    selected_direction: Option<BlockDirection>,
    do_merge: bool,
}

impl MergeWithStoneGoal {
    fn new() -> Self {
        Self {
            wander: WanderAroundGoal::with_interval(1.0, 10),
            selected_direction: None,
            do_merge: false,
        }
    }

    fn target_pos(mob: &dyn Mob, direction: BlockDirection) -> BlockPos {
        let pos = mob.get_entity().pos.load();
        BlockPos::new(
            pos.x.floor() as i32,
            (pos.y + 0.5).floor() as i32,
            pos.z.floor() as i32,
        )
        .offset(direction.to_offset())
    }
}

impl Goal for MergeWithStoneGoal {
    fn can_start(&mut self, mob: &dyn Mob) -> bool {
        if mob.get_mob_entity().get_target().is_some() || !mob.is_navigator_idle() {
            return false;
        }

        let world = mob.get_entity().world.load();
        if world.level_info.load().game_rules.mob_griefing
            && mob.get_random().random_range(0..to_goal_ticks(10)) == 0
        {
            let direction = BlockDirection::all()[mob.get_random().random_range(0..6)];
            self.selected_direction = Some(direction);
            let state = world.get_block_state_id(&Self::target_pos(mob, direction));
            if infested_state_by_host(state).is_some() {
                self.do_merge = true;
                return true;
            }
        }

        self.do_merge = false;
        self.wander.can_start(mob)
    }

    fn should_continue(&mut self, mob: &dyn Mob) -> bool {
        !self.do_merge && self.wander.should_continue(mob)
    }

    fn start(&mut self, mob: &dyn Mob) {
        if !self.do_merge {
            self.wander.start(mob);
            return;
        }
        let Some(direction) = self.selected_direction else {
            return;
        };

        let entity = mob.get_entity();
        let world = entity.world.load();
        let pos = Self::target_pos(mob, direction);
        if let Some(infested) = infested_state_by_host(world.get_block_state_id(&pos)) {
            world.set_block_state(&pos, infested, BlockFlags::NOTIFY_ALL);
            world.send_entity_status(entity, EntityStatus::SilverfishMergeAnim, None);
            entity.remove();
        }
    }

    fn stop(&mut self, mob: &dyn Mob) {
        self.wander.stop(mob);
    }

    fn controls(&self) -> Controls {
        Controls::MOVE
    }
}

/// Vanilla `Silverfish.SilverfishWakeUpFriendsGoal`.
struct WakeUpFriendsGoal {
    look_for_friends: Arc<AtomicI32>,
}

impl Goal for WakeUpFriendsGoal {
    fn can_start(&mut self, _mob: &dyn Mob) -> bool {
        self.look_for_friends.load(Ordering::Relaxed) > 0
    }

    fn tick(&mut self, mob: &dyn Mob) {
        if self.look_for_friends.fetch_sub(1, Ordering::Relaxed) - 1 > 0 {
            return;
        }

        let entity = mob.get_entity();
        let world = entity.world.load_full();
        let base_pos = entity.block_pos.load();
        let mob_griefing = world.level_info.load().game_rules.mob_griefing;

        for y_off in alternating_offsets(WAKE_UP_RANGE_Y) {
            for x_off in alternating_offsets(WAKE_UP_RANGE_XZ) {
                for z_off in alternating_offsets(WAKE_UP_RANGE_XZ) {
                    let test_pos = base_pos.offset(Vector3::new(x_off, y_off, z_off));
                    let Some(host) = host_state_by_infested(world.get_block_state_id(&test_pos))
                    else {
                        continue;
                    };

                    if mob_griefing {
                        // Vanilla `destroyBlock(pos, true, entity)`: drops and `spawnAfterBreak`, no break event.
                        let (block, state) = world.get_block_and_state(&test_pos);
                        let params = LootContextParameters {
                            block_state: Some(state),
                            position: Some(test_pos.0.to_f64()),
                            ..Default::default()
                        };
                        drop_loot(&world, block, &test_pos, false, &params);
                        if let Some(pumpkin_block) =
                            world.block_registry.get_pumpkin_block(block.id)
                        {
                            pumpkin_block.spawn_after_break(SpawnAfterBreakArgs {
                                world: &world,
                                block,
                                position: &test_pos,
                            });
                        }
                        world.set_block_state(&test_pos, BlockStateId::AIR, BlockFlags::NOTIFY_ALL);
                    } else {
                        world.set_block_state(&test_pos, host, BlockFlags::NOTIFY_ALL);
                    }

                    if mob.get_random().random_bool(0.5) {
                        return;
                    }
                }
            }
        }
    }
}

impl Mob for SilverfishEntity {
    fn get_mob_entity(&self) -> &MobEntity {
        &self.entity
    }

    fn mob_tick(&self, _caller: &dyn EntityBase) {
        let entity = &self.entity.living_entity.entity;
        if !entity.is_alive() {
            return;
        }

        let yaw = entity.yaw.load();
        entity.body_yaw.store(yaw);
        entity.head_yaw.store(yaw);
    }

    fn on_damage(&self, damage_type: DamageType, source: Option<&dyn EntityBase>) {
        let (always_triggers, _) =
            pumpkin_data::tag::DamageType::MINECRAFT_ALWAYS_TRIGGERS_SILVERFISH;
        if (source.is_some() || always_triggers.contains(&damage_type.message_id))
            && self.look_for_friends.load(Ordering::Relaxed) == 0
        {
            self.look_for_friends
                .store(to_goal_ticks(WAKE_UP_FRIENDS_DELAY), Ordering::Relaxed);
        }
    }
}
