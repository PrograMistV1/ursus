use crate::vulkan::resources::mapped_uniform::MappedUniformBuffer;
use ash::vk;

pub const MAX_POINT_LIGHTS: usize = 16;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DirectionalLight {
    pub direction: [f32; 4],
    pub color: [f32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct GpuPointLight {
    pub position: [f32; 4], // xyz = pos, w = radius
    pub color: [f32; 4],    // rgb = color, a = intensity
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LightingUbo {
    pub directional: DirectionalLight,
    pub point_lights: [GpuPointLight; MAX_POINT_LIGHTS],
    pub point_light_count: u32,
    pub _pad: [u32; 3],
    pub light_space_matrix: [[f32; 4]; 4],
}

impl Default for LightingUbo {
    fn default() -> Self {
        Self {
            directional: DirectionalLight { direction: [0.0; 4], color: [0.0; 4] },
            point_lights: [GpuPointLight { position: [0.0; 4], color: [0.0; 4] }; MAX_POINT_LIGHTS],
            point_light_count: 0,
            _pad: [0; 3],
            light_space_matrix: glam::Mat4::IDENTITY.to_cols_array_2d(),
        }
    }
}

pub struct LightBuffer(MappedUniformBuffer<LightingUbo>);

unsafe impl Send for LightBuffer {}
unsafe impl Sync for LightBuffer {}

impl LightBuffer {
    pub(crate) fn new(
        device: &ash::Device,
        physical_device: vk::PhysicalDevice,
        instance: &ash::Instance,
    ) -> anyhow::Result<Self> {
        Ok(Self(MappedUniformBuffer::new(device, physical_device, instance, LightingUbo::default())?))
    }

    pub fn upload(&self, data: &LightingUbo) {
        self.0.upload(data);
    }

    pub fn buffer(&self) -> vk::Buffer {
        self.0.buffer
    }

    pub fn size(&self) -> vk::DeviceSize {
        self.0.size()
    }
}
