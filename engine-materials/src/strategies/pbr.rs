use crate::error::MaterialError;
use crate::material::Material;
use crate::requirements::Requirements;
use crate::strategies::properties::*;
use crate::strategy::ShadingStrategy;
use crate::value::MaterialValue;
use glam::Vec4;

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct PbrGpuData {
    base_color: [f32; 4],
    emissive: [f32; 4],
    metallic: f32,
    roughness: f32,
    _pad: [f32; 2],
    tex_indices0: [u32; 4], // diffuse, normal, metallic_roughness, emissive
    tex_indices1: [u32; 4], // occlusion, pad, pad, pad
}

pub struct PbrStrategy {
    requirements: Requirements,
}

impl Default for PbrStrategy {
    fn default() -> Self {
        let requirements = Requirements::new()
            .optional(BASE_COLOR, MaterialValue::Color(Vec4::ONE))
            .optional(EMISSIVE, MaterialValue::Color(Vec4::new(0.0, 0.0, 0.0, 1.0)))
            .optional(METALLIC, MaterialValue::Float(0.0))
            .optional(ROUGHNESS, MaterialValue::Float(1.0));
        // Texture properties are intentionally not in `requirements` at all
        // (not even `optional`): a missing texture property means "no
        // texture", packed as bindless slot 0, not "use this default value".
        Self { requirements }
    }
}

impl PbrStrategy {
    fn texture_slot(material: &Material, id: crate::value::PropertyId) -> u32 {
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

    fn gpu_data_size(&self) -> usize {
        size_of::<PbrGpuData>()
    }

    fn pack(&self, material: &Material) -> Result<Vec<u8>, MaterialError> {
        let get = |id| {
            self.requirements.get_or_default(material, id).ok_or_else(|| MaterialError::MissingProperty {
                material: material.name.clone(),
                strategy: self.name(),
                property: id,
            })
        };

        let base_color = get(BASE_COLOR)?
            .as_vec4()
            .ok_or_else(|| type_mismatch(material, BASE_COLOR, "Color/Vec4", get(BASE_COLOR).unwrap()))?;
        let emissive = get(EMISSIVE)?
            .as_vec4()
            .ok_or_else(|| type_mismatch(material, EMISSIVE, "Color/Vec4", get(EMISSIVE).unwrap()))?;
        let metallic = get(METALLIC)?
            .as_float()
            .ok_or_else(|| type_mismatch(material, METALLIC, "Float", get(METALLIC).unwrap()))?;
        let roughness = get(ROUGHNESS)?
            .as_float()
            .ok_or_else(|| type_mismatch(material, ROUGHNESS, "Float", get(ROUGHNESS).unwrap()))?;

        let data = PbrGpuData {
            base_color: base_color.into(),
            emissive: emissive.into(),
            metallic,
            roughness,
            _pad: [0.0; 2],
            tex_indices0: [
                Self::texture_slot(material, DIFFUSE_TEXTURE),
                Self::texture_slot(material, NORMAL_TEXTURE),
                Self::texture_slot(material, METALLIC_ROUGHNESS_TEXTURE),
                Self::texture_slot(material, EMISSIVE_TEXTURE),
            ],
            tex_indices1: [Self::texture_slot(material, OCCLUSION_TEXTURE), 0, 0, 0],
        };

        Ok(bytemuck::bytes_of(&data).to_vec())
    }
}

fn type_mismatch(
    material: &Material,
    property: crate::value::PropertyId,
    expected: &'static str,
    actual: &MaterialValue,
) -> MaterialError {
    MaterialError::TypeMismatch { material: material.name.clone(), property, expected, actual: actual.type_name() }
}
