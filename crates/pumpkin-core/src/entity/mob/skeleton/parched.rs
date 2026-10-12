use std::sync::Arc;

use crate::entity::{
    Entity,
    mob::skeleton::{SkeletonEntityBase, SkeletonKind},
};

pub struct ParchedSkeletonEntity {
    entity: Arc<SkeletonEntityBase>,
}

impl ParchedSkeletonEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let entity = SkeletonEntityBase::new(entity, SkeletonKind::Parched);
        Arc::new(Self { entity })
    }
}

skeleton_mob!(ParchedSkeletonEntity);
