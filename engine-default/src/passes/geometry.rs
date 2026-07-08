use engine_core::assets::gpu_server::GpuAssetServer;
use engine_core::assets::{GpuMesh, ShaderHandle, Vertex};
use engine_core::components::mesh::MaterialHandle;
use engine_core::render::gfx::format::Format;
use engine_core::render::gfx::{CommandEncoder, PipelineId, PushConstantRange, ShaderStage, VertexFormat};
use engine_core::render::resource::ResourceHandle;
use engine_core::render::world::{ExtractedCamera, ExtractedMeshes, ExtractedRenderSettings, RenderWorld};
use engine_core::vulkan::gfx_pipeline::pipeline::PipelineDesc;
use glam::Mat4;
use std::collections::HashMap;

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
    pub shader: ShaderHandle,
}

pub struct GeometryPass {
    pipelines: HashMap<ShaderHandle, PipelineId>,
    color_formats: [Format; 2],
}

impl GeometryPass {
    pub fn new(gpu: &mut GpuAssetServer, color_formats: [Format; 2]) -> anyhow::Result<Self> {
        let mut pass = Self { pipelines: HashMap::new(), color_formats };
        let default = gpu.shaders.by_name("diffuse").unwrap();
        pass.get_or_create_pipeline(gpu, default)?;
        Ok(pass)
    }

    pub fn get_or_create_pipeline(
        &mut self,
        gpu: &mut GpuAssetServer,
        shader: ShaderHandle,
    ) -> anyhow::Result<PipelineId> {
        if let Some(&id) = self.pipelines.get(&shader) {
            return Ok(id);
        }

        let (vert_spv, frag_spv) = gpu.shaders.load_spv(shader)?;
        let vert_spv = vert_spv.to_vec();
        let frag_spv = frag_spv.unwrap().to_vec();

        let layout = Vertex::layout();
        let push_range = PushConstantRange::of::<MeshPushConstants>(ShaderStage::VertexFragment);

        let set_layouts = [gpu.bindless.layout, gpu.material_buffer.layout];

        let desc = PipelineDesc::with_depth_equal(
            &vert_spv,
            &frag_spv,
            &self.color_formats,
            &layout,
            std::slice::from_ref(&push_range),
        );

        let id = gpu.create_graphics_pipeline(&desc, &set_layouts)?;
        self.pipelines.insert(shader, id);
        Ok(id)
    }

    pub fn record(
        &mut self,
        enc: &mut CommandEncoder,
        rw: &RenderWorld,
        gpu: &GpuAssetServer,
        albedo: ResourceHandle,
        normal: ResourceHandle,
        depth: ResourceHandle,
    ) -> anyhow::Result<()> {
        let camera = rw.get::<ExtractedCamera>().cloned().unwrap_or_default();
        let meshes = rw.get::<ExtractedMeshes>().map(|m| m.instances.as_slice()).unwrap_or(&[]);
        let settings = rw.get::<ExtractedRenderSettings>().cloned().unwrap_or_default();

        let default_shader = gpu.shaders.by_name("diffuse").unwrap();
        let mut draw_calls: Vec<DrawCall> = meshes
            .iter()
            .filter_map(|inst| {
                Some(DrawCall {
                    gpu_mesh: gpu.get_gpu_mesh(inst.mesh)?,
                    model: inst.model,
                    material: inst.material,
                    shader: default_shader,
                })
            })
            .collect();

        draw_calls.sort_by_key(|dc| (dc.shader.0, dc.gpu_mesh as *const _ as usize));

        enc.begin_rendering_gbuffer(albedo, normal, depth, settings.clear_color);

        let mut current_shader: Option<ShaderHandle> = None;

        for dc in &draw_calls {
            if current_shader != Some(dc.shader) {
                let Some(&pipeline) = self.pipelines.get(&dc.shader) else {
                    log::warn!("Pipeline для шейдера {:?} не найден", dc.shader);
                    continue;
                };
                enc.bind_pipeline(pipeline);
                enc.bind_descriptor_sets(pipeline, &[gpu.bindless.set, gpu.material_buffer.set]);
                current_shader = Some(dc.shader);
            }

            let Some(&pipeline) = self.pipelines.get(&dc.shader) else {
                continue;
            };
            let mvp = camera.view_proj * dc.model;
            let pc = MeshPushConstants {
                mvp: mvp.to_cols_array_2d(),
                model: dc.model.to_cols_array_2d(),
                material_id: dc.material.map(|m| m.0).unwrap_or(0),
                _pad: [0; 3],
            };
            enc.push_constants(pipeline, ShaderStage::VertexFragment, &pc);
            enc.bind_mesh(dc.gpu_mesh);
            enc.draw_indexed(dc.gpu_mesh.index_count);
        }

        enc.end_rendering();
        Ok(())
    }
}
