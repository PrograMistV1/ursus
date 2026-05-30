use crate::assets::shader_registry::ShaderHandle;
use crate::assets::{AssetServer, GpuMesh};
use crate::ecs::components::{MaterialHandle, Transform};
use crate::vulkan::pipeline::PipelineDesc;
use crate::vulkan::{depth::DepthBuffer, render_target::RenderTarget, Pipeline};
use ash::vk;
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
    pub transform: &'a Transform,
    pub material: Option<MaterialHandle>,
    pub shader: ShaderHandle,
}

pub struct GeometryPass {
    pipelines: HashMap<ShaderHandle, Pipeline>,
    bindless_layout: vk::DescriptorSetLayout,
    material_layout: vk::DescriptorSetLayout,
    color_format: vk::Format,
}

impl GeometryPass {
    pub fn new(
        device: &ash::Device,
        color_format: vk::Format,
        bindless_layout: vk::DescriptorSetLayout,
        material_layout: vk::DescriptorSetLayout,
        assets: &mut AssetServer,
    ) -> anyhow::Result<Self> {
        let mut pass = Self {
            pipelines: HashMap::new(),
            bindless_layout,
            material_layout,
            color_format,
        };

        let default = assets.shaders.diffuse();
        pass.get_or_create_pipeline(device, default, &mut assets.shaders)?;

        Ok(pass)
    }

    pub fn get_or_create_pipeline(
        &mut self,
        device: &ash::Device,
        shader: ShaderHandle,
        registry: &mut crate::assets::shader_registry::ShaderRegistry,
    ) -> anyhow::Result<&Pipeline> {
        if self.pipelines.contains_key(&shader) {
            return Ok(&self.pipelines[&shader]);
        }

        log::info!("Создаём pipeline для шейдера {:?}", shader);

        let (vert_spv, frag_spv) = registry.load_spv(shader)?;
        let vert_spv = vert_spv.to_vec();
        let frag_spv = frag_spv.to_vec();

        let set_layouts = [self.bindless_layout, self.material_layout];

        let pipeline = Pipeline::new(
            device,
            &PipelineDesc::standard(&vert_spv, &frag_spv, self.color_format),
            &set_layouts,
        )?;

        self.pipelines.insert(shader, pipeline);
        Ok(&self.pipelines[&shader])
    }

    pub fn record(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        target: &RenderTarget,
        depth: &DepthBuffer,
        clear_color: [f32; 4],
        view_proj: Mat4,
        draw_calls: &[DrawCall<'_>],
        assets: &AssetServer,
    ) {
        unsafe {
            transition(device, cmd, target.image,
                       vk::ImageLayout::UNDEFINED,
                       vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                       vk::ImageAspectFlags::COLOR,
            );
            transition(device, cmd, depth.image,
                       vk::ImageLayout::UNDEFINED,
                       vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL,
                       vk::ImageAspectFlags::DEPTH,
            );

            let color_attachment = vk::RenderingAttachmentInfo::default()
                .image_view(target.view)
                .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue {
                    color: vk::ClearColorValue { float32: clear_color },
                });

            let depth_attachment = vk::RenderingAttachmentInfo::default()
                .image_view(depth.view)
                .image_layout(vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::DONT_CARE)
                .clear_value(vk::ClearValue {
                    depth_stencil: vk::ClearDepthStencilValue { depth: 1.0, stencil: 0 },
                });

            device.cmd_begin_rendering(
                cmd,
                &vk::RenderingInfo::default()
                    .render_area(vk::Rect2D {
                        offset: vk::Offset2D { x: 0, y: 0 },
                        extent: target.extent,
                    })
                    .layer_count(1)
                    .color_attachments(std::slice::from_ref(&color_attachment))
                    .depth_attachment(&depth_attachment),
            );

            device.cmd_set_viewport(cmd, 0, &[vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: target.extent.width as f32,
                height: target.extent.height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            }]);
            device.cmd_set_scissor(cmd, 0, &[vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: target.extent,
            }]);

            let mut sorted: Vec<(usize, &DrawCall)> = draw_calls
                .iter()
                .enumerate()
                .collect();
            sorted.sort_by_key(|(_, dc)| dc.shader.0);

            let mut current_shader: Option<ShaderHandle> = None;
            let mut current_layout: vk::PipelineLayout = vk::PipelineLayout::null();

            for (_i, dc) in &sorted {
                if current_shader != Some(dc.shader) {
                    let pipeline = match self.get_or_create_pipeline_inner(dc.shader) {
                        Some(p) => p,
                        None => {
                            log::warn!("Pipeline для шейдера {:?} не найден, пропускаем", dc.shader);
                            continue;
                        }
                    };

                    device.cmd_bind_pipeline(
                        cmd,
                        vk::PipelineBindPoint::GRAPHICS,
                        pipeline.handle,
                    );
                    device.cmd_bind_descriptor_sets(
                        cmd,
                        vk::PipelineBindPoint::GRAPHICS,
                        pipeline.layout,
                        0,
                        &[assets.bindless.set, assets.material_buffer_set()],
                        &[],
                    );

                    current_shader = Some(dc.shader);
                    current_layout = pipeline.layout;
                }

                let model = dc.transform.matrix();
                let mvp = view_proj * model;
                let material_id = dc.material.map(|m| m.0).unwrap_or(0);

                let pc = MeshPushConstants {
                    mvp: mvp.to_cols_array_2d(),
                    model: model.to_cols_array_2d(),
                    material_id,
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
            }

            device.cmd_end_rendering(cmd);

            transition(device, cmd, target.image,
                       vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                       vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                       vk::ImageAspectFlags::COLOR,
            );
        }
    }

    fn get_or_create_pipeline_inner(&self, shader: ShaderHandle) -> Option<&Pipeline> {
        self.pipelines.get(&shader)
    }
}

fn transition(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    image: vk::Image,
    from: vk::ImageLayout,
    to: vk::ImageLayout,
    aspect: vk::ImageAspectFlags,
) {
    let (src_stage, src_access, dst_stage, dst_access) = match (from, to) {
        (vk::ImageLayout::UNDEFINED, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL) => (
            vk::PipelineStageFlags2::TOP_OF_PIPE, vk::AccessFlags2::empty(),
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT, vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
        ),
        (vk::ImageLayout::UNDEFINED, vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL) => (
            vk::PipelineStageFlags2::TOP_OF_PIPE, vk::AccessFlags2::empty(),
            vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS,
            vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ | vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE,
        ),
        (vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL) => (
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT, vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            vk::PipelineStageFlags2::FRAGMENT_SHADER, vk::AccessFlags2::SHADER_READ,
        ),
        _ => panic!("transition: неизвестная пара {:?} → {:?}", from, to),
    };

    let barrier = vk::ImageMemoryBarrier2::default()
        .src_stage_mask(src_stage).src_access_mask(src_access)
        .dst_stage_mask(dst_stage).dst_access_mask(dst_access)
        .old_layout(from).new_layout(to)
        .image(image)
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask: aspect,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        });

    unsafe {
        device.cmd_pipeline_barrier2(
            cmd,
            &vk::DependencyInfo::default()
                .image_memory_barriers(std::slice::from_ref(&barrier)),
        );
    }
}