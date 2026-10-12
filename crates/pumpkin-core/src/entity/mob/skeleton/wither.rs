use std::sync::Arc;

use crate::entity::{
    Entity,
    mob::skeleton::{SkeletonEntityBase, SkeletonKind},
};

pub struct WitherSkeletonEntity {
    entity: Arc<SkeletonEntityBase>,
}

impl WitherSkeletonEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let entity = SkeletonEntityBase::new(entity, SkeletonKind::WitherSkeleton);
        Arc::new(Self { entity })
    }
}

skeleton_mob!(WitherSkeletonEntity);
