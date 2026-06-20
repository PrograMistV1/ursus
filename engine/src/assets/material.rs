#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MaterialData {
    pub base_color: [f32; 4],   // 16 B
    pub emissive: [f32; 4],     // 16 B
    pub metallic: f32,          //  4 B
    pub roughness: f32,         //  4 B
    pub _pad: [f32; 2],         //  8 B
    pub tex_indices0: [u32; 4], // 16 B  diffuse | normal | metallic_roughness | emissive
    pub tex_indices1: [u32; 4], // 16 B  occlusion | pad | pad | pad
} // = 80 B

impl MaterialData {
    pub fn default_white() -> Self {
        Self {
            base_color: [1.0, 1.0, 1.0, 1.0],
            emissive: [0.0; 4],
            metallic: 0.0,
            roughness: 0.5,
            _pad: [0.0; 2],
            tex_indices0: [0; 4],
            tex_indices1: [0; 4],
        }
    }
}

unsafe impl bytemuck::Pod for MaterialData {}
unsafe impl bytemuck::Zeroable for MaterialData {}

pub use crate::ecs::components::MaterialHandle;
