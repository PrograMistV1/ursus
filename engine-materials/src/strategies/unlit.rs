use crate::error::MaterialError;
use crate::field::{FieldDesc, FieldType};
use crate::material::Material;
use crate::requirements::Requirements;
use crate::strategies::properties::*;
use crate::strategy::ShadingStrategy;
use crate::value::{MaterialValue, TextureRef};

const FIELDS: &[FieldDesc] = &[
    FieldDesc::new("base_color", FieldType::Vec4),
    FieldDesc::new("diffuse_texture", FieldType::UInt),
];

pub struct UnlitStrategy {
    requirements: Requirements,
}

impl Default for UnlitStrategy {
    fn default() -> Self {
        let requirements = Requirements::new().optional(BASE_COLOR, MaterialValue::Color(glam::Vec4::ONE));
        Self { requirements }
    }
}

impl ShadingStrategy for UnlitStrategy {
    fn name(&self) -> &'static str {
        "unlit"
    }

    fn requirements(&self) -> &Requirements {
        &self.requirements
    }

    fn fields(&self) -> &[FieldDesc] {
        FIELDS
    }

    fn resolve(&self, material: &Material) -> Result<Vec<MaterialValue>, MaterialError> {
        let base_color = self.requirements.get_or_default(material, BASE_COLOR).copied().ok_or_else(|| {
            MaterialError::MissingProperty {
                material: material.name.clone(),
                strategy: self.name(),
                property: BASE_COLOR,
            }
        })?;

        let diffuse = material.get(DIFFUSE_TEXTURE).copied().unwrap_or(MaterialValue::Texture(TextureRef(0)));

        Ok(vec![base_color, diffuse])
    }
}
