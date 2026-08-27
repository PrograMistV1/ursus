use engine_core::assets::GpuMesh;
use engine_core::components::mesh::MaterialHandle;
use engine_core::render::gfx::TechniqueId;
use glam::Mat4;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MeshPushConstants {
    pub mvp: [[f32; 4]; 4],
    pub model: [[f32; 4]; 4],
    pub material_id: u32,
    pub _pad: [u32; 3],
}

pub struct DrawCall<'a> {
    pub gpu_mesh: &'a GpuMesh,
    pub model: Mat4,
    pub material: Option<MaterialHandle>,
    pub technique: TechniqueId,
}

pub struct GeometryPass {}

impl GeometryPass {}
