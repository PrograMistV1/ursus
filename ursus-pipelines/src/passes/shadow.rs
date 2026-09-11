use crate::systems::lights::ExtractedLights;
use crate::systems::ExtractedShadowMeshes;
use std::slice;
use ursus_core::assets::gpu_server::GpuAssetServer;
use ursus_core::assets::mesh::Vertex;
use ursus_core::render::gfx::types::format::Format;
use ursus_core::render::gfx::types::{PipelineId, PushConstantRange, ShaderStage, VertexFormat};
use ursus_core::render::gfx::CommandEncoder;
use ursus_core::render::resource::ResourceHandle;
use ursus_core::render::world::RWorld;
use ursus_core::vulkan::gfx_pipeline::pipeline::PipelineDesc;
use ursus_materials::MaterialHandle;

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
    pub fn new(gpu: &mut GpuAssetServer) -> anyhow::Result<Self> {
        let push_range = PushConstantRange::of::<ShadowPC>(ShaderStage::VertexFragment);

        let handle = gpu.shaders.by_name("shadow").expect("shader 'shadow' not registered");
        let (vert_spv, frag_spv) = gpu.shaders.load_spv(handle)?;
        let vert_spv = vert_spv.to_vec();
        let frag_spv = frag_spv.expect("'shadow' must have frag").to_vec();

        let full_layout = Vertex::layout();
        let vertex_layout = full_layout.only_locations(&[0, 2]); // shadow.vert only reads inPosition (0) and inUV (2)

        let desc = PipelineDesc::new(&vert_spv, &frag_spv, &[], &vertex_layout, slice::from_ref(&push_range))
            .depth_format(Format::Depth32Float)
            .depth_test(true)
            .depth_write(true);

        let bindless_set = gpu.bindless_set();
        let material_set = gpu.materials.descriptor_set();

        let pipeline = gpu.create_graphics_pipeline(&desc, &[bindless_set, material_set])?;

        Ok(Self { pipeline })
    }

    pub fn record(
        &self,
        enc: &mut CommandEncoder,
        rw: &RWorld,
        gpu: &GpuAssetServer,
        shadow_map: ResourceHandle,
    ) -> anyhow::Result<()> {
        let lights = rw.get::<ExtractedLights>().cloned().unwrap_or_default();
        let meshes = rw.get::<ExtractedShadowMeshes>().map(|m| m.instances.as_slice()).unwrap_or(&[]);

        enc.begin_rendering_depth_only(shadow_map);
        enc.bind_pipeline(self.pipeline);
        enc.bind_descriptor_sets(self.pipeline, &[gpu.bindless_set(), gpu.materials.descriptor_set()]);

        for inst in meshes {
            let Some(mesh) = gpu.meshes.get(inst.mesh) else {
                continue;
            };
            let material_id = material_id_for(gpu, inst.material);
            let mvp = lights.light_view_proj * inst.model;
            let pc = ShadowPC { light_space_mvp: mvp.to_cols_array_2d(), material_id, _pad: [0; 3] };
            enc.push_constants(self.pipeline, ShaderStage::VertexFragment, &pc);
            enc.bind_mesh(mesh);
            enc.draw_indexed(mesh.index_count);
        }

        enc.end_rendering();
        Ok(())
    }
}

fn material_id_for(gpu: &GpuAssetServer, material: Option<MaterialHandle>) -> u32 {
    let Some(handle) = material else { return 0 };
    gpu.materials.index_of(handle).map(|(_, index)| index as u32).unwrap_or(0)
}
