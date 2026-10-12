use std::sync::Arc;

use crate::entity::{
    Entity,
    mob::skeleton::{SkeletonEntityBase, SkeletonKind},
};

pub struct StraySkeletonEntity {
    entity: Arc<SkeletonEntityBase>,
}

impl StraySkeletonEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let entity = SkeletonEntityBase::new(entity, SkeletonKind::Stray);
        Arc::new(Self { entity })
    }
}

skeleton_mob!(StraySkeletonEntity);
