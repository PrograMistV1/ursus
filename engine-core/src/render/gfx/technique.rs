use crate::assets::ShaderHandle;
use crate::render::gfx::types::{BlendState, CullMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TechniqueId(pub u32);

/// Declarative description of the technique, registered once when building the pipeline.
#[derive(Debug, Clone)]
pub struct TechniqueDesc {
    pub name: String,
    pub shader: ShaderHandle,
    pub cull_mode: CullMode,
    pub blend: Option<BlendState>,
}

impl TechniqueDesc {
    pub fn new(name: impl Into<String>, shader: ShaderHandle) -> Self {
        Self { name: name.into(), shader, cull_mode: CullMode::Back, blend: None }
    }

    pub fn with_cull_mode(mut self, mode: CullMode) -> Self {
        self.cull_mode = mode;
        self
    }

    pub fn with_blend(mut self, blend: BlendState) -> Self {
        self.blend = Some(blend);
        self
    }
}

#[derive(Default)]
pub struct TechniqueRegistry {
    techniques: Vec<TechniqueDesc>,
    by_name: std::collections::HashMap<String, TechniqueId>,
}

impl TechniqueRegistry {
    pub fn register(&mut self, desc: TechniqueDesc) -> TechniqueId {
        if let Some(&id) = self.by_name.get(&desc.name) {
            log::warn!("TechniqueRegistry: Technique '{}' is already registered, returning old id", desc.name);
            return id;
        }
        let id = TechniqueId(self.techniques.len() as u32);
        self.by_name.insert(desc.name.clone(), id);
        self.techniques.push(desc);
        id
    }

    pub fn by_name(&self, name: &str) -> Option<TechniqueId> {
        self.by_name.get(name).copied()
    }

    pub fn get(&self, id: TechniqueId) -> &TechniqueDesc {
        &self.techniques[id.0 as usize]
    }

    pub fn iter(&self) -> impl Iterator<Item = (TechniqueId, &TechniqueDesc)> {
        self.techniques.iter().enumerate().map(|(i, d)| (TechniqueId(i as u32), d))
    }
}
