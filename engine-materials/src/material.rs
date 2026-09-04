use crate::value::{MaterialValue, PropertyId};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MaterialId(pub u32);

/// A named bag of shading-agnostic properties. `Material` has no notion of
/// "which shader" or "which pipeline" renders it - that decision belongs to
/// whichever `ShadingStrategy` is paired with this material at pack time.
#[derive(Debug, Clone)]
pub struct Material {
    pub name: String,
    /// Which strategy this material is meant to be packed with, by name.
    /// Looked up in `StrategyRegistry::by_name` - kept as a plain string so
    /// `Material` doesn't depend on the strategy trait/registry types.
    pub strategy: &'static str,
    properties: HashMap<PropertyId, MaterialValue>,
}

impl Material {
    pub fn new(name: impl Into<String>, strategy: &'static str) -> Self {
        Self { name: name.into(), strategy, properties: HashMap::new() }
    }

    pub fn with(mut self, id: PropertyId, value: MaterialValue) -> Self {
        self.properties.insert(id, value);
        self
    }

    pub fn set(&mut self, id: PropertyId, value: MaterialValue) {
        self.properties.insert(id, value);
    }

    pub fn get(&self, id: PropertyId) -> Option<&MaterialValue> {
        self.properties.get(&id)
    }

    pub fn contains(&self, id: PropertyId) -> bool {
        self.properties.contains_key(&id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&PropertyId, &MaterialValue)> {
        self.properties.iter()
    }
}
