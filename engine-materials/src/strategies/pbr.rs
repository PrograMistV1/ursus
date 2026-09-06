use crate::error::MaterialError;
use crate::field::{FieldDesc, FieldType};
use crate::material::Material;
use crate::requirements::Requirements;
use crate::strategies::properties::*;
use crate::strategy::ShadingStrategy;
use crate::value::{MaterialValue, TextureRef};

/// Field order matches the `MaterialData` struct in mesh.frag /
/// depth_prepass.frag / shadow.frag: base_color, emissive, metallic,
/// roughness, then texture indices.
const FIELDS: &[FieldDesc] = &[
    FieldDesc::new("base_color", FieldType::Vec4),
    FieldDesc::new("emissive", FieldType::Vec4),
    FieldDesc::new("metallic", FieldType::Float),
    FieldDesc::new("roughness", FieldType::Float),
    FieldDesc::new("diffuse_texture", FieldType::UInt),
    FieldDesc::new("normal_texture", FieldType::UInt),
    FieldDesc::new("metallic_roughness_texture", FieldType::UInt),
    FieldDesc::new("emissive_texture", FieldType::UInt),
    FieldDesc::new("occlusion_texture", FieldType::UInt),
];

pub struct PbrStrategy {
    requirements: Requirements,
}

impl Default for PbrStrategy {
    fn default() -> Self {
        let requirements = Requirements::new()
            .optional(BASE_COLOR, MaterialValue::Color(glam::Vec4::ONE))
            .optional(EMISSIVE, MaterialValue::Color(glam::Vec4::new(0.0, 0.0, 0.0, 1.0)))
            .optional(METALLIC, MaterialValue::Float(0.0))
            .optional(ROUGHNESS, MaterialValue::Float(1.0));
        Self { requirements }
    }
}

impl PbrStrategy {
    /// Texture fields fall back to bindless slot 0 (the white-texture
    /// fallback) when a material has no value for them.
    fn texture_value(material: &Material, id: crate::value::PropertyId) -> MaterialValue {
        material.get(id).copied().unwrap_or(MaterialValue::Texture(TextureRef(0)))
    }
}

impl ShadingStrategy for PbrStrategy {
    fn name(&self) -> &'static str {
        "pbr"
    }

    fn requirements(&self) -> &Requirements {
        &self.requirements
    }

    fn fields(&self) -> &[FieldDesc] {
        FIELDS
    }

    fn resolve(&self, material: &Material) -> Result<Vec<MaterialValue>, MaterialError> {
        let get = |id| {
            self.requirements.get_or_default(material, id).copied().ok_or_else(|| MaterialError::MissingProperty {
                material: material.name.clone(),
                strategy: self.name(),
                property: id,
            })
        };

        Ok(vec![
            get(BASE_COLOR)?,
            get(EMISSIVE)?,
            get(METALLIC)?,
            get(ROUGHNESS)?,
            Self::texture_value(material, DIFFUSE_TEXTURE),
            Self::texture_value(material, NORMAL_TEXTURE),
            Self::texture_value(material, METALLIC_ROUGHNESS_TEXTURE),
            Self::texture_value(material, EMISSIVE_TEXTURE),
            Self::texture_value(material, OCCLUSION_TEXTURE),
        ])
    }
}
