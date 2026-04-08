use std::collections::HashMap;
use std::sync::Arc;

use crate::core::event::GameEvent;
use crate::core::ids::{Seat, SkillId, TileId};

use super::hooks::{RuleHook, SkillContext};

pub trait SkillDefinition: RuleHook + Send + Sync {
    fn id(&self) -> &str;
    #[allow(dead_code)]
    fn name(&self) -> &'static str;

    fn activate(
        &self,
        ctx: &mut SkillContext<'_>,
        _target: Option<Seat>,
        _tile_ids: &[TileId],
    ) -> Result<Vec<GameEvent>, String> {
        Ok(vec![GameEvent::SkillActivated {
            seat: ctx.actor,
            skill_id: self.id().to_string(),
        }])
    }
}

pub trait SkillRegistry: Send + Sync {
    fn get(&self, id: &str) -> Option<&dyn SkillDefinition>;
}

#[derive(Default)]
pub struct StaticSkillRegistry {
    definitions: HashMap<SkillId, Arc<dyn SkillDefinition>>,
}

impl StaticSkillRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, definition: Arc<dyn SkillDefinition>) {
        self.definitions
            .insert(definition.id().to_string(), definition);
    }

    pub fn with_definition(mut self, definition: Arc<dyn SkillDefinition>) -> Self {
        self.register(definition);
        self
    }
}

impl SkillRegistry for StaticSkillRegistry {
    fn get(&self, id: &str) -> Option<&dyn SkillDefinition> {
        self.definitions.get(id).map(Arc::as_ref)
    }
}
