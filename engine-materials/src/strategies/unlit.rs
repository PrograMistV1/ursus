use crate::error::MaterialError;
use crate::material::Material;
use crate::requirements::Requirements;
use crate::strategies::properties::*;
use crate::strategy::ShadingStrategy;
use crate::value::MaterialValue;

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct UnlitGpuData {
    base_color: [f32; 4],
    emissive: [f32; 4],
    metallic: f32,
    roughness: f32,
    _pad: [f32; 2],
    tex_indices0: [u32; 4],
    tex_indices1: [u32; 4],
}

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

    fn gpu_data_size(&self) -> usize {
        size_of::<UnlitGpuData>()
    }

    fn pack(&self, material: &Material) -> Result<Vec<u8>, MaterialError> {
        let base_color =
            self.requirements.get_or_default(material, BASE_COLOR).and_then(MaterialValue::as_vec4).ok_or_else(
                || MaterialError::MissingProperty {
                    material: material.name.clone(),
                    strategy: self.name(),
                    property: BASE_COLOR,
                },
            )?;

        let diffuse_slot = material.get(DIFFUSE_TEXTURE).and_then(MaterialValue::as_texture).map(|t| t.0).unwrap_or(0);

        let data = UnlitGpuData {
            base_color: base_color.into(),
            emissive: [0.0; 4],
            metallic: 0.0,
            roughness: 1.0,
            _pad: [0.0; 2],
            tex_indices0: [diffuse_slot, 0, 0, 0],
            tex_indices1: [0; 4],
        };

        Ok(bytemuck::bytes_of(&data).to_vec())
    }
}
