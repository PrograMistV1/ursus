use crate::passes::material_buffer::MaterialBuffer;
use engine_core::assets::gpu_server::GpuAssetServer;
use engine_core::assets::Vertex;
use engine_core::render::gfx::types::{CompareOp, Format, PipelineId, PushConstantRange, ShaderStage, VertexFormat};
use engine_core::render::gfx::CommandEncoder;
use engine_core::render::resource::ResourceHandle;
use engine_core::render::world::{ExtractedLights, ExtractedShadowMeshes, RenderWorld};
use engine_core::vulkan::gfx_pipeline::pipeline::PipelineDesc;
use std::slice;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ShadowPC {
    pub light_space_mvp: [[f32; 4]; 4],
    pub material_id: u32,
    pub _pad: [u32; 3],
}

pub struct ShadowPass {
    pipeline: PipelineId,
}

impl ShadowPass {
    pub fn new(gpu: &mut GpuAssetServer, material_buffer: &MaterialBuffer) -> anyhow::Result<Self> {
        let layout = Vertex::layout().only_locations(&[0, 2]);
        let set_layouts = [gpu.bindless_set(), material_buffer.descriptor_set];

        let handle = gpu.shaders.by_name("shadow").expect("шейдер 'shadow' не зарегистрирован");
        let (vert_spv, frag_spv) = gpu.shaders.load_spv(handle)?;
        let vert_spv = vert_spv.to_vec();
        let frag_spv = frag_spv.expect("'shadow' должен иметь frag").to_vec();

        let push_range = PushConstantRange::of::<ShadowPC>(ShaderStage::VertexFragment);

        let desc = PipelineDesc::new(&vert_spv, &frag_spv, &[], &layout, slice::from_ref(&push_range))
            .depth_format(Format::Depth32Float)
            .depth_compare(CompareOp::LessOrEqual);

        let pipeline = gpu.create_graphics_pipeline(&desc, &set_layouts)?;

        Ok(Self { pipeline })
    }

    pub fn record(
        &self,
        enc: &mut CommandEncoder,
        rw: &RenderWorld,
        gpu: &GpuAssetServer,
        material_buffer: &MaterialBuffer,
        shadow_map: ResourceHandle,
    ) -> anyhow::Result<()> {
        let lights = rw.get::<ExtractedLights>().cloned().unwrap_or_default();
        let meshes = rw.get::<ExtractedShadowMeshes>().map(|m| m.instances.as_slice()).unwrap_or(&[]);

        enc.begin_rendering_depth_only(shadow_map);
        enc.bind_pipeline(self.pipeline);
        enc.bind_descriptor_sets(self.pipeline, &[gpu.bindless_set(), material_buffer.descriptor_set]);

        for inst in meshes {
            let Some(mesh) = gpu.meshes.get(inst.mesh) else {
                continue;
            };
            let mvp = lights.light_view_proj * inst.model;
            let pc = ShadowPC {
                light_space_mvp: mvp.to_cols_array_2d(),
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
