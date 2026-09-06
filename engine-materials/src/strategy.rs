use crate::error::MaterialError;
use crate::field::FieldDesc;
use crate::material::Material;
use crate::requirements::Requirements;
use crate::value::MaterialValue;
use std::collections::HashMap;
use std::sync::Arc;

/// Interprets a `Material`'s properties for a specific way of shading it.
/// A strategy describes what GPU fields it needs (`fields`) and how to
/// resolve each one from a material (`resolve`); it does not decide how
/// those fields are laid out in memory - that is the responsibility of a
/// separate packing module (see `pack::aos`) that consumes both methods.
///
/// Custom strategies are ordinary implementations of this trait, registered
/// into a `StrategyRegistry` - no changes to engine-materials are needed to
/// add one.
pub trait ShadingStrategy: Send + Sync {
    /// Stable name used for lookup in `StrategyRegistry` and stored on
    /// `Material::strategy`.
    fn name(&self) -> &'static str;

    /// What this strategy needs from a material to be able to resolve it.
    fn requirements(&self) -> &Requirements;

    /// The fields this strategy produces, in a fixed order. `resolve()`
    /// returns one value per field, in this same order.
    fn fields(&self) -> &[FieldDesc];

    /// Resolves this material's value for every field in `fields()`,
    /// applying this strategy's own fallbacks for missing properties.
    fn resolve(&self, material: &Material) -> Result<Vec<MaterialValue>, MaterialError>;

    /// Validates a material's properties against `requirements()`.
    fn validate(&self, material: &Material) -> Result<(), MaterialError> {
        self.requirements().validate(material, self.name())
    }
}

/// Registry of available shading strategies, looked up by name.
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

    /// Looks up the strategy named by `material.strategy`, validates the
    /// material against it, and resolves it.
    pub fn resolve(&self, material: &Material) -> Result<Vec<MaterialValue>, MaterialError> {
        let strategy = self
            .by_name(material.strategy)
            .ok_or_else(|| MaterialError::UnknownStrategy(material.strategy.to_string()))?;
        strategy.validate(material)?;
        strategy.resolve(material)
    }
}
