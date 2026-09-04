use crate::error::MaterialError;
use crate::material::Material;
use crate::requirements::Requirements;
use std::collections::HashMap;
use std::sync::Arc;

/// Converts a `Material`'s properties into the raw GPU bytes a specific
/// shader expects. Each strategy owns its own `#[repr(C)]` GPU-data struct
/// internally and is the only place in engine-materials that knows that
/// struct's layout - `pack` erases it to `Vec<u8>` at the trait boundary so
/// callers (the registry, the renderer) can stay polymorphic over strategies.
///
/// Custom strategies are ordinary implementations of this trait, registered
/// into a `StrategyRegistry` - no changes to engine-materials are needed to
/// add one.
pub trait ShadingStrategy: Send + Sync {
    /// Stable name used for lookup in `StrategyRegistry` and stored on
    /// `Material::strategy`.
    fn name(&self) -> &'static str;

    /// What this strategy needs from a material to be able to pack it.
    fn requirements(&self) -> &Requirements;

    /// Size in bytes of the packed GPU data this strategy produces. Used by
    /// callers that need to lay out a fixed-stride buffer (e.g. a
    /// `MaterialData` SSBO indexed by `material_id`) without packing first.
    fn gpu_data_size(&self) -> usize;

    /// Packs `material`'s properties into this strategy's GPU data layout.
    /// Returns exactly `gpu_data_size()` bytes on success.
    fn pack(&self, material: &Material) -> Result<Vec<u8>, MaterialError>;

    /// Validates a material's properties against `requirements()`. Provided
    /// so callers can validate once (e.g. at material-assignment time)
    /// separately from packing every frame.
    fn validate(&self, material: &Material) -> Result<(), MaterialError> {
        self.requirements().validate(material, self.name())
    }
}

/// Registry of available shading strategies, looked up by name. Analogous in
/// spirit to `ShaderRegistry` elsewhere in the engine.
#[derive(Default)]
pub struct StrategyRegistry {
    by_name: HashMap<&'static str, Arc<dyn ShadingStrategy>>,
}

impl StrategyRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, strategy: impl ShadingStrategy + 'static) {
        let name = strategy.name();
        if self.by_name.insert(name, Arc::new(strategy)).is_some() {
            log::warn!("StrategyRegistry: strategy '{name}' was already registered, overwriting");
        }
    }

    pub fn by_name(&self, name: &str) -> Option<Arc<dyn ShadingStrategy>> {
        self.by_name.get(name).cloned()
    }

    /// Looks up and packs `material` using the strategy named by
    /// `material.strategy`. The single entry point most callers should use.
    pub fn pack(&self, material: &Material) -> Result<Vec<u8>, MaterialError> {
        let strategy = self
            .by_name(material.strategy)
            .ok_or_else(|| MaterialError::UnknownStrategy(material.strategy.to_string()))?;
        strategy.validate(material)?;
        strategy.pack(material)
    }
}
