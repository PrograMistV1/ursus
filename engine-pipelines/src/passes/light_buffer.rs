use engine_core::assets::gpu_server::GpuAssetServer;
use engine_core::render::gfx::{BufferUsage, DirectionalLight, GpuPointLight, MAX_POINT_LIGHTS};
use engine_core::vulkan::MappedGpuBuffer;

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

unsafe impl bytemuck::Pod for LightingUbo {}
unsafe impl bytemuck::Zeroable for LightingUbo {}

pub struct LightBuffer(MappedGpuBuffer<LightingUbo>);

unsafe impl Send for LightBuffer {}
unsafe impl Sync for LightBuffer {}

impl LightBuffer {
    pub fn new(gpu: &GpuAssetServer) -> anyhow::Result<Self> {
        let inner = gpu.create_mapped_buffer::<LightingUbo>(BufferUsage::Uniform, 1)?;
        inner.upload_one(&LightingUbo::default());
        Ok(Self(inner))
    }

    pub fn upload(&self, data: &LightingUbo) {
        self.0.upload_one(data);
    }

    pub(crate) fn inner(&self) -> &MappedGpuBuffer<LightingUbo> {
        &self.0
    }
}
