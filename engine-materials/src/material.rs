use crate::value::{MaterialValue, PropertyId};
use std::collections::HashMap;

/// Stable handle identifying a material instance. Lives here rather than in
/// ursus-core so engine-materials stays the single source of truth for
/// material identity - ursus-core's ECS just stores this as a component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MaterialHandle(pub u32);

#[derive(Debug, Clone)]
pub struct Material {
    pub name: String,
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
