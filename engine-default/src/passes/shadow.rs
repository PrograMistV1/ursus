use engine_core::assets::gpu_server::GpuAssetServer;
use engine_core::assets::Vertex;
use engine_core::render::gfx::{CommandEncoder, PipelineId, ShaderStage, VertexFormat};
use engine_core::render::resource::ResourceHandle;
use engine_core::render::world::{ExtractedLights, ExtractedShadowMeshes, RenderWorld};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ShadowPC {
    pub light_space_mvp: [[f32; 4]; 4],
}

pub struct ShadowPass {
    pipeline: PipelineId,
}

impl ShadowPass {
    pub fn new(gpu: &mut GpuAssetServer) -> anyhow::Result<Self> {
        let layout = Vertex::layout().only_locations(&[0]);

        let handle = gpu.shaders.by_name("shadow").expect("шейдер 'shadow' не зарегистрирован");
        let (vert_spv, _) = gpu.shaders.load_spv(handle)?;
        let vert_spv = vert_spv.to_vec();

        let push_range = ash::vk::PushConstantRange::default()
            .stage_flags(ash::vk::ShaderStageFlags::VERTEX)
            .offset(0)
            .size(size_of::<ShadowPC>() as u32);

        let pipeline =
            gpu.create_depth_only_pipeline(&vert_spv, &layout, std::slice::from_ref(&push_range), Some((2.0, 1.5)))?;

        Ok(Self { pipeline })
    }

    pub fn record(
        &self,
        enc: &mut CommandEncoder,
        rw: &RenderWorld,
        gpu: &GpuAssetServer,
        shadow_map: ResourceHandle,
    ) -> anyhow::Result<()> {
        let lights = rw.get::<ExtractedLights>().cloned().unwrap_or_default();
        let meshes = rw.get::<ExtractedShadowMeshes>().map(|m| m.instances.as_slice()).unwrap_or(&[]);

        enc.begin_rendering_depth_only(shadow_map);
        enc.bind_pipeline(self.pipeline);

        for inst in meshes {
            let Some(mesh) = gpu.get_gpu_mesh(inst.mesh) else {
                continue;
            };
            let mvp = lights.light_view_proj * inst.model;
            enc.push_constants(
                self.pipeline,
                ShaderStage::Vertex,
                &ShadowPC { light_space_mvp: mvp.to_cols_array_2d() },
            );
            enc.bind_mesh(mesh);
            enc.draw_indexed(mesh.index_count);
        }

        enc.end_rendering();
        Ok(())
    }
}
