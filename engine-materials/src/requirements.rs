use crate::error::MaterialError;
use crate::material::Material;
use crate::value::{MaterialValue, PropertyId};

/// Declares what a `ShadingStrategy` needs from a `Material` before it can
/// pack it. Checked once when a material is assigned to a strategy, so
/// mistakes surface immediately instead of as a panic deep in `pack()`.
#[derive(Debug, Default, Clone)]
pub struct Requirements {
    pub required: Vec<PropertyId>,
    pub optional_with_defaults: Vec<(PropertyId, MaterialValue)>,
}

impl Requirements {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn require(mut self, id: PropertyId) -> Self {
        self.required.push(id);
        self
    }

    pub fn optional(mut self, id: PropertyId, default: MaterialValue) -> Self {
        self.optional_with_defaults.push((id, default));
        self
    }

    /// Validates that `material` has every required property. Does not check
    /// types - individual `MaterialValue::as_*` accessors are the source of
    /// truth for type coercion inside `pack()`.
    pub fn validate(&self, material: &Material, strategy_name: &'static str) -> Result<(), MaterialError> {
        for &id in &self.required {
            if !material.contains(id) {
                return Err(MaterialError::MissingProperty {
                    material: material.name.clone(),
                    strategy: strategy_name,
                    property: id,
                });
            }
        }
        Ok(())
    }

    /// Reads a property from `material`, falling back to this requirement's
    /// declared default for optional properties. Returns `None` if the
    /// property is neither present nor has a declared default.
    pub fn get_or_default<'a>(&'a self, material: &'a Material, id: PropertyId) -> Option<&'a MaterialValue> {
        material
            .get(id)
            .or_else(|| self.optional_with_defaults.iter().find(|(pid, _)| *pid == id).map(|(_, default)| default))
    }
}
