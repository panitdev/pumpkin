use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use crossbeam::atomic::AtomicCell;
use pumpkin_data::{
    Block, BlockDirection, BlockState,
    block_properties::{Axis, PistonHeadLikeProperties, PistonType},
};
use pumpkin_nbt::compound::NbtCompound;
use pumpkin_util::math::{boundingbox::BoundingBox, position::BlockPos, vector3::Vector3};

use crate::world::{BlockFlags, World};

use super::BlockEntity;

pub struct PistonBlockEntity {
    pub position: BlockPos,
    pub pushed_block_state: &'static BlockState,
    pub facing: BlockDirection,
    pub current_progress: AtomicCell<f32>,
    pub last_progress: AtomicCell<f32>,
    pub extending: bool,
    pub source: bool,
    /// World age of the last tick, vanilla `lastTicked`.
    pub last_ticked: AtomicI64,
}

impl PistonBlockEntity {
    pub const ID: &'static str = "minecraft:piston";

    #[must_use]
    pub fn new(
        position: BlockPos,
        pushed_block_state: &'static BlockState,
        facing: BlockDirection,
        extending: bool,
        source: bool,
    ) -> Self {
        Self {
            position,
            pushed_block_state,
            facing,
            current_progress: 0.0.into(),
            last_progress: 0.0.into(),
            extending,
            source,
            last_ticked: AtomicI64::new(0),
        }
    }

    const fn movement_direction(&self) -> BlockDirection {
        if self.extending {
            self.facing
        } else {
            self.facing.opposite()
        }
    }

    /// Vanilla's `getAmountExtended`: how far back from the block's final position
    /// the visual is at a given animation progress. Negative for extending.
    fn amount_extended(&self, progress: f32) -> f32 {
        if self.extending {
            progress - 1.0
        } else {
            1.0 - progress
        }
    }

    fn dir_vec(dir: BlockDirection, scale: f64) -> Vector3<f64> {
        let off = dir.to_offset();
        Vector3::new(
            f64::from(off.x) * scale,
            f64::from(off.y) * scale,
            f64::from(off.z) * scale,
        )
    }

    /// Vanilla `getCollisionRelatedBlockState`: a retracting source piston collides like its
    /// head, everything else like the block it carries.
    fn collision_related_state(&self, progress: f32) -> &'static BlockState {
        let moved_block = self.pushed_block_state.id.to_block();
        if !self.extending
            && self.source
            && (moved_block == &Block::PISTON || moved_block == &Block::STICKY_PISTON)
        {
            let mut head = PistonHeadLikeProperties::default(&Block::PISTON_HEAD);
            head.short = progress > 0.25;
            head.r#type = if moved_block == &Block::STICKY_PISTON {
                PistonType::Sticky
            } else {
                PistonType::Normal
            };
            head.facing = self.facing.to_facing();
            return BlockState::from_id(head.to_state_id(&Block::PISTON_HEAD));
        }
        self.pushed_block_state
    }

    /// Vanilla `PistonMath.getMovementArea`: the slab in front of `aabb`'s leading face that it
    /// sweeps over `amount`.
    fn movement_area(aabb: BoundingBox, dir: BlockDirection, amount: f64) -> BoundingBox {
        let delta = amount * f64::from(dir.to_offset().x + dir.to_offset().y + dir.to_offset().z);
        let (lo, hi) = (delta.min(0.0), delta.max(0.0));
        let (mn, mx) = (aabb.min, aabb.max);
        match dir {
            BlockDirection::West => BoundingBox::new(
                Vector3::new(mn.x + lo, mn.y, mn.z),
                Vector3::new(mn.x + hi, mx.y, mx.z),
            ),
            BlockDirection::East => BoundingBox::new(
                Vector3::new(mx.x + lo, mn.y, mn.z),
                Vector3::new(mx.x + hi, mx.y, mx.z),
            ),
            BlockDirection::Down => BoundingBox::new(
                Vector3::new(mn.x, mn.y + lo, mn.z),
                Vector3::new(mx.x, mn.y + hi, mx.z),
            ),
            BlockDirection::Up => BoundingBox::new(
                Vector3::new(mn.x, mx.y + lo, mn.z),
                Vector3::new(mx.x, mx.y + hi, mx.z),
            ),
            BlockDirection::North => BoundingBox::new(
                Vector3::new(mn.x, mn.y, mn.z + lo),
                Vector3::new(mx.x, mx.y, mn.z + hi),
            ),
            BlockDirection::South => BoundingBox::new(
                Vector3::new(mn.x, mn.y, mx.z + lo),
                Vector3::new(mx.x, mx.y, mx.z + hi),
            ),
        }
    }

    /// Ports vanilla `moveCollidedEntities`: entities in front of any collision box of the moving
    /// block get shoved along by the part of the movement they overlap.
    fn push_entities(&self, world: &Arc<World>, new_progress: f32) {
        let last = self.last_progress.load();
        let delta = f64::from(new_progress - last);
        if delta <= 0.0 {
            return;
        }

        let motion_dir = self.movement_direction();
        let state = self.collision_related_state(last);
        let offset = Self::dir_vec(self.facing, f64::from(self.amount_extended(last)))
            + self.position.to_f64();
        let shapes: Vec<BoundingBox> = state
            .get_block_collision_shapes()
            .map(|shape| shape.shift(offset))
            .collect();
        let Some(first) = shapes.first() else {
            return;
        };
        let mut bounds = *first;
        for shape in &shapes[1..] {
            bounds.min = Vector3::new(
                bounds.min.x.min(shape.min.x),
                bounds.min.y.min(shape.min.y),
                bounds.min.z.min(shape.min.z),
            );
            bounds.max = Vector3::new(
                bounds.max.x.max(shape.max.x),
                bounds.max.y.max(shape.max.y),
                bounds.max.z.max(shape.max.z),
            );
        }
        let area = Self::aabb_union(Self::movement_area(bounds, motion_dir, delta), bounds);
        let bounce = self.pushed_block_state.id.to_block() == &Block::SLIME_BLOCK;

        for entity in world.get_entities_at_box(&area) {
            let e = entity.get_entity();
            if e.no_physics.load(Ordering::Relaxed) {
                continue;
            }
            // Player movement is client-authoritative; pushing them server-side would rubber-band.
            if entity.get_player().is_some() {
                continue;
            }

            // Slime blocks launch whatever they hit along the push axis.
            if bounce {
                let mut velocity = e.velocity.load();
                let step = Self::dir_vec(motion_dir, 1.0);
                match motion_dir.to_axis() {
                    Axis::X => velocity.x = step.x,
                    Axis::Y => velocity.y = step.y,
                    Axis::Z => velocity.z = step.z,
                }
                e.velocity.store(velocity);
                e.velocity_dirty.store(true, Ordering::Relaxed);
            }

            let entity_aabb = e.bounding_box.load();
            let mut push = 0.0f64;
            for shape in &shapes {
                let moving = Self::movement_area(*shape, motion_dir, delta);
                if moving.intersects(&entity_aabb) {
                    push = push.max(Self::intersection_size(moving, motion_dir, entity_aabb));
                    if push >= delta {
                        break;
                    }
                }
            }
            if push <= 0.0 {
                continue;
            }
            Self::move_entity(e, motion_dir, push.min(delta) + 0.01);

            // A retracting source piston also shoves the entity out of the piston's own cube.
            if !self.extending && self.source {
                Self::push_out_of_piston_body(e, &self.position, motion_dir, delta);
            }
        }
    }

    /// Vanilla `moveStuckEntities`: a honey block moving sideways carries the entities standing
    /// on it.
    fn move_stuck_entities(&self, world: &Arc<World>, new_progress: f32) {
        if self.pushed_block_state.id.to_block() != &Block::HONEY_BLOCK {
            return;
        }
        let motion_dir = self.movement_direction();
        if motion_dir.to_axis() == Axis::Y {
            return;
        }
        let delta = f64::from(new_progress - self.last_progress.load());
        let amount = f64::from(self.amount_extended(self.last_progress.load()));
        let base = self.position.to_f64() + Self::dir_vec(self.facing, amount);
        // Honey's collision shape tops out at 15/16.
        let area = BoundingBox::new(
            base + Vector3::new(0.0, 15.0 / 16.0, 0.0),
            base + Vector3::new(1.0, 1.500_001, 1.0),
        );
        for entity in world.get_entities_at_box(&area) {
            let e = entity.get_entity();
            if entity.get_player().is_some()
                || e.no_physics.load(Ordering::Relaxed)
                || !e.on_ground.load(Ordering::Relaxed)
            {
                continue;
            }
            let pos = e.pos.load();
            let supported = (e.bounding_box.load().min.y - (base.y + 1.0)).abs() < 1.0E-5
                || (pos.x >= area.min.x
                    && pos.x <= area.max.x
                    && pos.z >= area.min.z
                    && pos.z <= area.max.z);
            if supported {
                Self::move_entity(e, motion_dir, delta);
            }
        }
    }

    /// Vanilla `getIntersectionSize`: how much `entity` overlaps `swept` along
    /// `motion_dir`. Positive means the entity is in the path of the moving block.
    fn intersection_size(
        swept: BoundingBox,
        motion_dir: BlockDirection,
        entity: BoundingBox,
    ) -> f64 {
        match motion_dir {
            BlockDirection::East => swept.max.x - entity.min.x,
            BlockDirection::West => entity.max.x - swept.min.x,
            BlockDirection::Up => swept.max.y - entity.min.y,
            BlockDirection::Down => entity.max.y - swept.min.y,
            BlockDirection::South => swept.max.z - entity.min.z,
            BlockDirection::North => entity.max.z - swept.min.z,
        }
    }

    fn move_entity(entity: &crate::entity::Entity, dir: BlockDirection, distance: f64) {
        let new_pos = entity.pos.load() + Self::dir_vec(dir, distance);
        entity.set_pos(new_pos);
    }

    /// Vanilla `push`: when a piston head retracts, shove entities that ended up
    /// inside the piston-body cube back out the opposite direction (slightly past
    /// the move they just got, so the net motion is essentially zero).
    fn push_out_of_piston_body(
        entity: &crate::entity::Entity,
        piston_pos: &BlockPos,
        motion_dir: BlockDirection,
        amount: f64,
    ) {
        let body_aabb = BoundingBox::from_block(piston_pos);
        let entity_aabb = entity.bounding_box.load();
        if !body_aabb.intersects(&entity_aabb) {
            return;
        }
        let back = motion_dir.opposite();
        let e = Self::intersection_size(body_aabb, back, entity_aabb) + 0.01;
        let f = Self::intersection_size(
            body_aabb,
            back,
            Self::aabb_intersection(body_aabb, entity_aabb),
        ) + 0.01;
        if (e - f).abs() < 0.01 {
            let distance = e.min(amount) + 0.01;
            Self::move_entity(entity, back, distance);
        }
    }

    const fn aabb_union(a: BoundingBox, b: BoundingBox) -> BoundingBox {
        BoundingBox::new(
            Vector3::new(
                a.min.x.min(b.min.x),
                a.min.y.min(b.min.y),
                a.min.z.min(b.min.z),
            ),
            Vector3::new(
                a.max.x.max(b.max.x),
                a.max.y.max(b.max.y),
                a.max.z.max(b.max.z),
            ),
        )
    }

    const fn aabb_intersection(a: BoundingBox, b: BoundingBox) -> BoundingBox {
        BoundingBox::new(
            Vector3::new(
                a.min.x.max(b.min.x),
                a.min.y.max(b.min.y),
                a.min.z.max(b.min.z),
            ),
            Vector3::new(
                a.max.x.min(b.max.x),
                a.max.y.min(b.max.y),
                a.max.z.min(b.max.z),
            ),
        )
    }

    /// Vanilla `finalTick`: snaps the moving block into its final position right away.
    pub fn finish(&self, world: &Arc<World>) {
        if self.last_progress.load() < 1.0 {
            self.current_progress.store(1.0);
            self.last_progress.store(1.0);
            let pos = self.position;
            world.remove_block_entity(&pos);
            if world.get_block(&pos) == &Block::MOVING_PISTON {
                let state = if self.source {
                    Block::AIR.default_state.id
                } else {
                    world.update_from_neighbor_shapes(self.pushed_block_state.id, &pos)
                };
                world.set_block_state(&pos, state, BlockFlags::NOTIFY_ALL);
                world.update_neighbor(&pos, state.to_block());
            }
        }
    }

    fn state_to_nbt(state: &BlockState) -> NbtCompound {
        let block = state.id.to_block();
        let mut nbt = NbtCompound::new();
        nbt.put_string("Name", format!("minecraft:{}", block.name));
        if let Some(props) = block.properties(state.id) {
            let props = props.to_props();
            if !props.is_empty() {
                let mut props_nbt = NbtCompound::new();
                for (key, value) in props {
                    props_nbt.put_string(key, value.to_string());
                }
                nbt.put_compound("Properties", props_nbt);
            }
        }
        nbt
    }

    fn state_from_nbt(nbt: &NbtCompound) -> Option<&'static BlockState> {
        let name = nbt.get_string("Name")?;
        let block = Block::from_name(name.strip_prefix("minecraft:").unwrap_or(name))?;
        let Some(props) = nbt.get_compound("Properties") else {
            return Some(block.default_state);
        };
        let props: Vec<(&str, &str)> = props
            .child_tags
            .iter()
            .filter_map(|(key, value)| Some((&**key, value.extract_string()?)))
            .collect();
        Some(BlockState::from_id(
            block.from_properties(&props).to_state_id(block),
        ))
    }
}

const BLOCK_STATE: &str = "blockState";
const FACING: &str = "facing";
const LAST_PROGRESS: &str = "progress";
const EXTENDING: &str = "extending";
const SOURCE: &str = "source";

impl BlockEntity for PistonBlockEntity {
    fn resource_location(&self) -> &'static str {
        Self::ID
    }

    fn get_position(&self) -> BlockPos {
        self.position
    }

    fn tick(&self, world: &Arc<World>) {
        self.last_ticked
            .store(world.get_world_age(), Ordering::Relaxed);
        let current_progress = self.current_progress.load();
        self.last_progress.store(current_progress);
        if current_progress >= 1.0 {
            let pos = self.position;
            world.remove_block_entity(&pos);
            if world.get_block(&pos) == &Block::MOVING_PISTON {
                let mut updated_state =
                    world.update_from_neighbor_shapes(self.pushed_block_state.id, &pos);
                if updated_state.to_block().is_air() {
                    // The block can't stay here (e.g. a torch pushed off its wall): place it
                    // anyway, then break it so it drops.
                    world.set_block_state(
                        &pos,
                        self.pushed_block_state.id,
                        BlockFlags::FORCE_STATE | BlockFlags::MOVED,
                    );
                    world.break_block(&pos, None, BlockFlags::NOTIFY_ALL);
                } else {
                    // Pushed blocks never carry water along.
                    let block = updated_state.to_block();
                    if updated_state.is_waterlogged()
                        && let Some(dry) = block.set_waterlogged(updated_state, false)
                    {
                        updated_state = dry;
                    }
                    world.set_block_state(
                        &pos,
                        updated_state,
                        BlockFlags::NOTIFY_ALL | BlockFlags::MOVED,
                    );
                    world.update_neighbor(&pos, block);
                }
            }
            return;
        }
        let new_progress = (current_progress + 0.5).min(1.0);
        self.push_entities(world, new_progress);
        self.move_stuck_entities(world, new_progress);
        self.current_progress.store(new_progress);
    }

    fn from_nbt(nbt: &pumpkin_nbt::compound::NbtCompound, position: BlockPos) -> Self
    where
        Self: Sized,
    {
        let pushed_block_state = nbt
            .get_compound(BLOCK_STATE)
            .and_then(Self::state_from_nbt)
            .unwrap_or(Block::AIR.default_state);
        let facing = nbt.get_byte(FACING).unwrap_or(0);
        let last_progress = nbt.get_float(LAST_PROGRESS).unwrap_or(0.0);
        let extending = nbt.get_bool(EXTENDING).unwrap_or(false);
        let source = nbt.get_bool(SOURCE).unwrap_or(false);
        Self {
            pushed_block_state,
            position,
            facing: BlockDirection::from_index(facing as u8).unwrap_or(BlockDirection::Down),
            current_progress: last_progress.into(),
            last_progress: last_progress.into(),
            extending,
            source,
            last_ticked: AtomicI64::new(0),
        }
    }

    fn write_nbt(&self, nbt: &mut NbtCompound) {
        nbt.put_compound(BLOCK_STATE, Self::state_to_nbt(self.pushed_block_state));
        nbt.put_byte(FACING, self.facing.to_index() as i8);
        nbt.put_float(LAST_PROGRESS, self.last_progress.load());
        nbt.put_bool(EXTENDING, self.extending);
        nbt.put_bool(SOURCE, self.source);
    }

    fn sends_update_packet(&self) -> bool {
        // The client animates from the block event; an update would overwrite its progress.
        false
    }

    fn chunk_data_nbt(&self) -> Option<NbtCompound> {
        let mut nbt = NbtCompound::new();
        nbt.put_compound(BLOCK_STATE, Self::state_to_nbt(self.pushed_block_state));
        nbt.put_byte(FACING, self.facing.to_index() as i8);
        nbt.put_float(LAST_PROGRESS, self.last_progress.load());
        nbt.put_bool(EXTENDING, self.extending);
        nbt.put_bool(SOURCE, self.source);
        // TODO: duplicated code because of async :c
        Some(nbt)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use pumpkin_data::block_properties::{Facing, StickyPistonLikeProperties};

    use super::*;

    #[test]
    fn moved_state_survives_nbt_round_trip() {
        let mut props = StickyPistonLikeProperties::default(&Block::STICKY_PISTON);
        props.facing = Facing::East;
        let state = BlockState::from_id(props.to_state_id(&Block::STICKY_PISTON));

        let nbt = PistonBlockEntity::state_to_nbt(state);
        assert_eq!(nbt.get_string("Name"), Some("minecraft:sticky_piston"));
        let loaded = PistonBlockEntity::state_from_nbt(&nbt).expect("state should load");
        assert_eq!(loaded.id, state.id);
    }

    #[test]
    fn movement_area_is_the_slab_in_front_of_the_leading_face() {
        let cube = BoundingBox::full_block();
        let east = PistonBlockEntity::movement_area(cube, BlockDirection::East, 0.5);
        assert_eq!((east.min.x, east.max.x), (1.0, 1.5));
        let west = PistonBlockEntity::movement_area(cube, BlockDirection::West, 0.5);
        assert_eq!((west.min.x, west.max.x), (-0.5, 0.0));
        let up = PistonBlockEntity::movement_area(cube, BlockDirection::Up, 0.5);
        assert_eq!((up.min.y, up.max.y), (1.0, 1.5));
    }

    #[test]
    fn stateless_block_round_trips_without_properties() {
        let nbt = PistonBlockEntity::state_to_nbt(Block::STONE.default_state);
        assert!(nbt.get_compound("Properties").is_none());
        let loaded = PistonBlockEntity::state_from_nbt(&nbt).expect("state should load");
        assert_eq!(loaded.id, Block::STONE.default_state.id);
    }

    #[test]
    fn unknown_block_name_does_not_load() {
        let mut nbt = NbtCompound::new();
        nbt.put_string("Name", "minecraft:not_a_block".to_string());
        assert!(PistonBlockEntity::state_from_nbt(&nbt).is_none());
    }
}
