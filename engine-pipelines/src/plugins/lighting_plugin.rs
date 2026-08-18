use crate::systems::LightExtract;
use engine_core::app::{EngineContext, Plugin};

pub struct LightingPlugin;

impl Plugin for LightingPlugin {
    fn build(&self, ctx: &mut EngineContext) {
        ctx.add_extract_system(LightExtract);
    }
}
