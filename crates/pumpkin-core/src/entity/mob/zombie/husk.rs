use std::sync::Arc;

use crate::entity::Entity;
use crate::entity::mob::zombie::{ZombieEntityBase, ZombieKind};

pub struct HuskEntity {
    entity: Arc<ZombieEntityBase>,
}

impl HuskEntity {
    pub fn new(entity: Entity) -> Arc<Self> {
        let entity = ZombieEntityBase::new(entity, ZombieKind::Husk);
        let zombie = Self { entity };
        Arc::new(zombie)
    }

    #[must_use]
    pub fn with_can_break_doors(entity: Entity, can_break_doors: bool) -> Arc<Self> {
        let entity =
            ZombieEntityBase::with_can_break_doors(entity, ZombieKind::Husk, can_break_doors);
        let zombie = Self { entity };
        Arc::new(zombie)
    }
}

zombie_mob!(HuskEntity, entity);
