use crate::error::MaterialError;
use crate::field::{FieldDesc, FieldType};
use crate::material::Material;
use crate::requirements::Requirements;
use crate::strategies::properties::*;
use crate::strategy::ShadingStrategy;
use crate::value::{MaterialValue, PropertyId};

/// Field order matches the `MaterialData` struct in mesh.frag /
/// depth_prepass.frag / shadow.frag: base_color, emissive, metallic,
/// roughness, then texture indices.
const FIELDS: &[FieldDesc] = &[
    FieldDesc::new("base_color", FieldType::Vec4),
    FieldDesc::new("emissive", FieldType::Vec4),
    FieldDesc::new("metallic", FieldType::Float),
    FieldDesc::new("roughness", FieldType::Float),
    FieldDesc::new("tex_indices0", FieldType::UVec4),
    FieldDesc::new("tex_indices1", FieldType::UVec4),
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
    fn texture_slot(material: &Material, id: PropertyId) -> u32 {
        material.get(id).and_then(MaterialValue::as_texture).map(|t| t.0).unwrap_or(0)
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

        let tex_indices0 = MaterialValue::UVec4([
            Self::texture_slot(material, DIFFUSE_TEXTURE),
            Self::texture_slot(material, NORMAL_TEXTURE),
            Self::texture_slot(material, METALLIC_ROUGHNESS_TEXTURE),
            Self::texture_slot(material, EMISSIVE_TEXTURE),
        ]);
        let tex_indices1 = MaterialValue::UVec4([Self::texture_slot(material, OCCLUSION_TEXTURE), 0, 0, 0]);

        Ok(vec![
            get(BASE_COLOR)?,
            get(EMISSIVE)?,
            get(METALLIC)?,
            get(ROUGHNESS)?,
            tex_indices0,
            tex_indices1,
        ])
    }
}
