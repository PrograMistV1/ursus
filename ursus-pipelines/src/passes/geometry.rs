use std::slice;
use ursus_core::assets::gpu_server::GpuAssetServer;
use ursus_core::assets::mesh::Vertex;
use ursus_core::render::gfx::types::format::Format;
use ursus_core::render::gfx::types::{CompareOp, CullMode, PipelineId, PushConstantRange, ShaderStage, VertexFormat};
use ursus_core::render::gfx::CommandEncoder;
use ursus_core::render::resource::ResourceHandle;
use ursus_core::render::world::{ExtractedCamera, ExtractedMeshes, RWorld};
use ursus_core::vulkan::gfx_pipeline::pipeline::PipelineDesc;
use ursus_materials::MaterialHandle;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MeshPushConstants {
    pub mvp: [[f32; 4]; 4],
    pub model: [[f32; 4]; 4],
    pub material_id: u32,
    pub _pad: [u32; 3],
}

pub struct GeometryPass {
    pipeline: PipelineId,
}

impl GeometryPass {
    pub fn new(gpu: &mut GpuAssetServer, albedo_format: Format, normal_format: Format) -> anyhow::Result<Self> {
        let push_range = PushConstantRange::of::<MeshPushConstants>(ShaderStage::VertexFragment);

        let handle = gpu.shaders.by_name("diffuse").expect("shader 'diffuse' not registered");
        let (vert_spv, frag_spv) = gpu.shaders.load_spv(handle)?;
        let vert_spv = vert_spv.to_vec();
        let frag_spv = frag_spv.expect("'diffuse' must have frag").to_vec();

        let vertex_layout = Vertex::layout();

        let color_formats = [albedo_format, normal_format];
        let desc =
            PipelineDesc::new(&vert_spv, &frag_spv, &color_formats, &vertex_layout, slice::from_ref(&push_range))
                .depth_format(Format::Depth32Float)
                .depth_test(true)
                .depth_write(true)
                .depth_compare(CompareOp::LessOrEqual)
                .cull_mode(CullMode::Back);

        let bindless_set = gpu.bindless_set();
        let material_set = gpu.materials.descriptor_set();

        let pipeline = gpu.create_graphics_pipeline(&desc, &[bindless_set, material_set])?;

        Ok(Self { pipeline })
    }

    //todo: this function has too many arguments (8/7)
    #[allow(clippy::too_many_arguments)]
    pub fn record(
        &self,
        enc: &mut CommandEncoder,
        r_world: &RWorld,
        gpu: &GpuAssetServer,
        albedo: ResourceHandle,
        normal: ResourceHandle,
        depth: ResourceHandle,
        clear_color: [f32; 4],
    ) -> anyhow::Result<()> {
        let camera = r_world.get::<ExtractedCamera>().cloned().unwrap_or_default();
        let meshes = r_world.get::<ExtractedMeshes>().map(|m| m.instances.as_slice()).unwrap_or(&[]);

        enc.begin_rendering_gbuffer(albedo, normal, depth, clear_color);
        enc.bind_pipeline(self.pipeline);
        enc.bind_descriptor_sets(self.pipeline, &[gpu.bindless_set(), gpu.materials.descriptor_set()]);

        for inst in meshes {
            let Some(mesh) = gpu.meshes.get(inst.mesh) else {
                continue;
            };

            let material_id = material_id_for(gpu, inst.material);

            let mvp = camera.view_proj * inst.model;
            let pc = MeshPushConstants {
                mvp: mvp.to_cols_array_2d(),
                model: inst.model.to_cols_array_2d(),
                material_id,
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

fn material_id_for(gpu: &GpuAssetServer, material: Option<MaterialHandle>) -> u32 {
    let Some(handle) = material else { return 0 };
    match gpu.materials.index_of(handle) {
        Some((_stride, index)) => index as u32,
        None => {
            log::warn!("GeometryPass: material {handle:?} has no uploaded GPU data yet, using slot 0");
            0
        }
    }
}
