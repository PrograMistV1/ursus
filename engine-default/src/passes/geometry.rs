use ash::ext::debug_utils::Device;
use ash::vk;
use engine_core::assets::gpu_server::GpuAssetServer;
use engine_core::assets::{GpuMesh, ShaderRegistry};
use engine_core::assets::{ShaderHandle, Vertex};
use engine_core::components::mesh::MaterialHandle;
use engine_core::render::resource::GpuImage;
use engine_core::vulkan::core::debug::{cmd_begin_label, cmd_end_label};
use engine_core::vulkan::gfx_pipeline::pipeline::PipelineDesc;
use engine_core::vulkan::Pipeline;
use glam::Mat4;
use std::collections::HashMap;

#[repr(C)]
pub struct MeshPushConstants {
    pub mvp: [[f32; 4]; 4],
    pub model: [[f32; 4]; 4],
    pub material_id: u32,
}

pub struct DrawCall<'a> {
    pub gpu_mesh: &'a GpuMesh,
    pub model: Mat4,
    pub material: Option<MaterialHandle>,
    pub shader: ShaderHandle,
}

pub struct GeometryPass {
    pipelines: HashMap<ShaderHandle, Pipeline>,
    bindless_layout: vk::DescriptorSetLayout,
    material_layout: vk::DescriptorSetLayout,
    color_formats: [vk::Format; 2],
}

impl GeometryPass {
    pub fn new(
        device: &ash::Device,
        color_formats: [vk::Format; 2],
        bindless_layout: vk::DescriptorSetLayout,
        material_layout: vk::DescriptorSetLayout,
        registry: &mut ShaderRegistry,
    ) -> anyhow::Result<Self> {
        let mut pass = Self { pipelines: HashMap::new(), bindless_layout, material_layout, color_formats };
        let default = registry.by_name("diffuse").unwrap();
        pass.get_or_create_pipeline(device, default, registry)?;
        Ok(pass)
    }

    pub fn get_or_create_pipeline(
        &mut self,
        device: &ash::Device,
        shader: ShaderHandle,
        registry: &mut ShaderRegistry,
    ) -> anyhow::Result<&Pipeline> {
        if self.pipelines.contains_key(&shader) {
            return Ok(&self.pipelines[&shader]);
        }
        let (vert_spv, frag_spv) = registry.load_spv(shader)?;
        let vert_spv = vert_spv.to_vec();
        let frag_spv = frag_spv.unwrap().to_vec();
        let set_layouts = [self.bindless_layout, self.material_layout];

        let binding = Vertex::binding_description();
        let attributes = Vertex::attribute_descriptions();
        let push_range = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
            .offset(0)
            .size(size_of::<MeshPushConstants>() as u32);

        let desc = PipelineDesc::with_depth_equal(
            &vert_spv,
            &frag_spv,
            &self.color_formats,
            std::slice::from_ref(&binding),
            &attributes,
            std::slice::from_ref(&push_range),
        );

        let pipeline = Pipeline::new(device, &desc, &set_layouts)?;
        self.pipelines.insert(shader, pipeline);
        Ok(&self.pipelines[&shader])
    }

    pub fn record(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        albedo: &impl GpuImage,
        normal: &impl GpuImage,
        depth: &impl GpuImage,
        clear_color: [f32; 4],
        view_proj: Mat4,
        draw_calls: &[DrawCall<'_>],
        gpu_assets: &GpuAssetServer,
        debug_utils: Option<&Device>,
    ) {
        let extent = albedo.extent();

        unsafe {
            let color_attachments = [
                vk::RenderingAttachmentInfo::default()
                    .image_view(albedo.view())
                    .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .load_op(vk::AttachmentLoadOp::CLEAR)
                    .store_op(vk::AttachmentStoreOp::STORE)
                    .clear_value(vk::ClearValue { color: vk::ClearColorValue { float32: clear_color } }),
                vk::RenderingAttachmentInfo::default()
                    .image_view(normal.view())
                    .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                    .load_op(vk::AttachmentLoadOp::CLEAR)
                    .store_op(vk::AttachmentStoreOp::STORE)
                    .clear_value(vk::ClearValue { color: vk::ClearColorValue { float32: [0.0; 4] } }),
            ];

            let depth_attachment = vk::RenderingAttachmentInfo::default()
                .image_view(depth.view())
                .image_layout(vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::LOAD)
                .store_op(vk::AttachmentStoreOp::STORE);

            device.cmd_begin_rendering(
                cmd,
                &vk::RenderingInfo::default()
                    .render_area(vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent })
                    .layer_count(1)
                    .color_attachments(&color_attachments)
                    .depth_attachment(&depth_attachment),
            );

            device.cmd_set_viewport(
                cmd,
                0,
                &[vk::Viewport {
                    x: 0.0,
                    y: 0.0,
                    width: extent.width as f32,
                    height: extent.height as f32,
                    min_depth: 0.0,
                    max_depth: 1.0,
                }],
            );
            device.cmd_set_scissor(cmd, 0, &[vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent }]);

            let mut sorted: Vec<&DrawCall> = draw_calls.iter().collect();
            {
                puffin::profile_scope!("sort_draw_calls");
                sorted.sort_by_key(|dc| (dc.shader.0, dc.gpu_mesh as *const _ as usize));
            }

            let mut current_shader: Option<ShaderHandle> = None;
            let mut current_layout = vk::PipelineLayout::null();

            for dc in &sorted {
                puffin::profile_scope!("draw_call");
                if let Some(du) = &debug_utils {
                    let label_name = format!("mesh_{}", dc.gpu_mesh.name);
                    cmd_begin_label(du, cmd, &label_name);
                }
                if current_shader != Some(dc.shader) {
                    let pipeline = match self.pipelines.get(&dc.shader) {
                        Some(p) => p,
                        None => {
                            log::warn!("Pipeline для шейдера {:?} не найден", dc.shader);
                            continue;
                        }
                    };
                    device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, pipeline.handle);
                    device.cmd_bind_descriptor_sets(
                        cmd,
                        vk::PipelineBindPoint::GRAPHICS,
                        pipeline.layout,
                        0,
                        &[gpu_assets.bindless.set, gpu_assets.material_buffer.set],
                        &[],
                    );
                    current_shader = Some(dc.shader);
                    current_layout = pipeline.layout;
                }

                let mvp = view_proj * dc.model;
                let pc = MeshPushConstants {
                    mvp: mvp.to_cols_array_2d(),
                    model: dc.model.to_cols_array_2d(),
                    material_id: dc.material.map(|m| m.0).unwrap_or(0),
                };
                let pc_bytes = std::slice::from_raw_parts(
                    &pc as *const MeshPushConstants as *const u8,
                    size_of::<MeshPushConstants>(),
                );
                device.cmd_push_constants(
                    cmd,
                    current_layout,
                    vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
                    0,
                    pc_bytes,
                );
                device.cmd_bind_vertex_buffers(cmd, 0, &[dc.gpu_mesh.vertex_buffer], &[0]);
                device.cmd_bind_index_buffer(cmd, dc.gpu_mesh.index_buffer, 0, vk::IndexType::UINT32);
                device.cmd_draw_indexed(cmd, dc.gpu_mesh.index_count, 1, 0, 0, 0);
                if let Some(du) = &debug_utils {
                    cmd_end_label(du, cmd);
                }
            }

            device.cmd_end_rendering(cmd);
        }
    }
}
