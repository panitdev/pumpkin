use std::sync::Arc;

use crate::entity::{
    Entity,
    mob::skeleton::{SkeletonEntityBase, SkeletonKind},
};

pub struct BoggedSkeletonEntity {
    entity: Arc<SkeletonEntityBase>,
}

impl BoggedSkeletonEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let entity = SkeletonEntityBase::new(entity, SkeletonKind::Bogged);
        Arc::new(Self { entity })
    }
}

skeleton_mob!(BoggedSkeletonEntity);
