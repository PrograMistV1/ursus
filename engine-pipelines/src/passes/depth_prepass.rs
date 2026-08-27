use engine_core::assets::gpu_server::GpuAssetServer;
use engine_core::render::gfx::types::{PipelineId, ShaderStage};
use engine_core::render::gfx::CommandEncoder;
use engine_core::render::resource::ResourceHandle;
use engine_core::render::world::{ExtractedCamera, ExtractedMeshes, RenderWorld};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct DepthPrepassPC {
    mvp: [[f32; 4]; 4],
    material_id: u32,
    _pad: [u32; 3],
}

pub struct DepthPrepass {
    pipeline: PipelineId,
}

impl DepthPrepass {
    pub fn record(
        &self,
        enc: &mut CommandEncoder,
        rw: &RenderWorld,
        gpu: &GpuAssetServer,
        depth: ResourceHandle,
    ) -> anyhow::Result<()> {
        let camera = rw.get::<ExtractedCamera>().cloned().unwrap_or_default();
        let meshes = rw.get::<ExtractedMeshes>().map(|m| m.instances.as_slice()).unwrap_or(&[]);

        enc.begin_rendering_depth_only(depth);
        enc.bind_pipeline(self.pipeline);

        for inst in meshes {
            let Some(mesh) = gpu.meshes.get(inst.mesh) else {
                continue;
            };
            let mvp = camera.view_proj * inst.model;
            let pc = DepthPrepassPC {
                mvp: mvp.to_cols_array_2d(),
                material_id: inst.material.map(|m| m.0).unwrap_or(0),
                _pad: [0; 3],
            };
            enc.push_constants(self.pipeline, ShaderStage::VertexFragment, &pc);
            enc.bind_mesh(mesh);
            enc.draw_indexed(mesh.index_count);
        }

        enc.end_rendering();
        Ok(())
    }
}
