use std::sync::Arc;

use crate::entity::{
    Entity,
    mob::skeleton::{SkeletonEntityBase, SkeletonKind},
};

pub struct SkeletonEntity {
    entity: Arc<SkeletonEntityBase>,
}

impl SkeletonEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let entity = SkeletonEntityBase::new(entity, SkeletonKind::Skeleton);
        Arc::new(Self { entity })
    }
}

skeleton_mob!(SkeletonEntity);
