use engine_core::assets::gpu_server::GpuAssetServer;
use engine_core::assets::Vertex;
use engine_core::render::gfx::{CommandEncoder, PipelineId, PushConstantRange, ShaderStage, VertexFormat};
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
    pub fn new(gpu: &mut GpuAssetServer) -> anyhow::Result<Self> {
        let layout = Vertex::layout().only_locations(&[0, 2]);

        let set_layouts = [gpu.bindless.layout, gpu.material_buffer.layout];

        let handle = gpu.shaders.by_name("depth_prepass").expect("шейдер 'depth_prepass' не зарегистрирован");
        let (vert_spv, frag_spv) = gpu.shaders.load_spv(handle)?;
        let vert_spv = vert_spv.to_vec();
        let frag_spv = frag_spv.unwrap().to_vec();

        let push_range = PushConstantRange::of::<DepthPrepassPC>(ShaderStage::VertexFragment);

        let pipeline = gpu.create_depth_only_pipeline(
            &vert_spv,
            Some(&frag_spv),
            &layout,
            std::slice::from_ref(&push_range),
            &set_layouts,
            None,
        )?;

        Ok(Self { pipeline })
    }

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
        enc.bind_descriptor_sets(self.pipeline, &[gpu.bindless_set(), gpu.material_buffer_set()]);

        for inst in meshes {
            let Some(mesh) = gpu.get_gpu_mesh(inst.mesh) else {
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
