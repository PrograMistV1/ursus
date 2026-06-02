use crate::assets::GpuMesh;
use crate::ecs::components::Transform;
use crate::vulkan::resources::shadow_map::{ShadowMap, SHADOW_MAP_SIZE};
use ash::vk;
use glam::Mat4;

#[repr(C)]
pub struct ShadowPC {
    pub light_space_mvp: [[f32; 4]; 4],
}

pub struct ShadowPass {
    pub pipeline: vk::Pipeline,
    pub layout: vk::PipelineLayout,
    device: ash::Device,
}

pub struct ShadowDrawCall<'a> {
    pub gpu_mesh: &'a GpuMesh,
    pub transform: &'a Transform,
}

impl ShadowPass {
    pub fn new(device: &ash::Device) -> anyhow::Result<Self> {
        let push_range = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::VERTEX)
            .offset(0)
            .size(std::mem::size_of::<ShadowPC>() as u32);

        let layout = unsafe {
            device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .push_constant_ranges(std::slice::from_ref(&push_range)),
                None,
            )?
        };

        let vert = crate::vulkan::pipeline::shader::ShaderModule::from_bytes(
            device,
            include_bytes!(concat!(env!("OUT_DIR"), "/shadow.vert.spv")),
        )?;

        let binding = vk::VertexInputBindingDescription::default()
            .binding(0)
            .stride(32) // size_of::<Vertex>()
            .input_rate(vk::VertexInputRate::VERTEX);

        let attributes = [vk::VertexInputAttributeDescription::default()
            .binding(0)
            .location(0)
            .format(vk::Format::R32G32B32_SFLOAT)
            .offset(0)];

        let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_binding_descriptions(std::slice::from_ref(&binding))
            .vertex_attribute_descriptions(&attributes);

        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST);

        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .viewport_count(1)
            .scissor_count(1);

        let rasterizer = vk::PipelineRasterizationStateCreateInfo::default()
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::FRONT) // front-face culling для теней
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .depth_bias_enable(true)
            .depth_bias_constant_factor(2.0)
            .depth_bias_slope_factor(1.5)
            .line_width(1.0);

        let multisampling = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);

        let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default()
            .depth_test_enable(true)
            .depth_write_enable(true)
            .depth_compare_op(vk::CompareOp::LESS_OR_EQUAL);

        let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let dynamic_state =
            vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

        let color_blending = vk::PipelineColorBlendStateCreateInfo::default();

        let mut rendering_info = vk::PipelineRenderingCreateInfo::default()
            .depth_attachment_format(vk::Format::D32_SFLOAT);

        let stage = vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert.handle)
            .name(c"main");

        let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
            .stages(std::slice::from_ref(&stage))
            .vertex_input_state(&vertex_input)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterizer)
            .multisample_state(&multisampling)
            .depth_stencil_state(&depth_stencil)
            .color_blend_state(&color_blending)
            .dynamic_state(&dynamic_state)
            .layout(layout)
            .push_next(&mut rendering_info);

        let pipeline = unsafe {
            device
                .create_graphics_pipelines(
                    vk::PipelineCache::null(),
                    std::slice::from_ref(&pipeline_info),
                    None,
                )
                .map_err(|(_, e)| e)?[0]
        };

        log::debug!("ShadowPass создан");
        Ok(Self {
            pipeline,
            layout,
            device: device.clone(),
        })
    }

    pub fn record(
        &self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        shadow_map: &ShadowMap,
        light_view_proj: Mat4,
        draw_calls: &[ShadowDrawCall<'_>],
    ) {
        let extent = vk::Extent2D {
            width: SHADOW_MAP_SIZE,
            height: SHADOW_MAP_SIZE,
        };

        unsafe {
            // UNDEFINED -> DEPTH_ATTACHMENT
            transition_depth(
                device,
                cmd,
                shadow_map.image,
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL,
            );

            let depth_attachment = vk::RenderingAttachmentInfo::default()
                .image_view(shadow_map.view)
                .image_layout(vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .clear_value(vk::ClearValue {
                    depth_stencil: vk::ClearDepthStencilValue {
                        depth: 1.0,
                        stencil: 0,
                    },
                });

            device.cmd_begin_rendering(
                cmd,
                &vk::RenderingInfo::default()
                    .render_area(vk::Rect2D {
                        offset: vk::Offset2D { x: 0, y: 0 },
                        extent,
                    })
                    .layer_count(1)
                    .depth_attachment(&depth_attachment),
            );

            device.cmd_set_viewport(
                cmd,
                0,
                &[vk::Viewport {
                    x: 0.0,
                    y: 0.0,
                    width: SHADOW_MAP_SIZE as f32,
                    height: SHADOW_MAP_SIZE as f32,
                    min_depth: 0.0,
                    max_depth: 1.0,
                }],
            );
            device.cmd_set_scissor(
                cmd,
                0,
                &[vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent,
                }],
            );

            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline);

            for dc in draw_calls {
                let mvp = light_view_proj * dc.transform.matrix();
                let pc = ShadowPC {
                    light_space_mvp: mvp.to_cols_array_2d(),
                };
                let pc_bytes = std::slice::from_raw_parts(
                    &pc as *const ShadowPC as *const u8,
                    std::mem::size_of::<ShadowPC>(),
                );
                device.cmd_push_constants(
                    cmd,
                    self.layout,
                    vk::ShaderStageFlags::VERTEX,
                    0,
                    pc_bytes,
                );

                device.cmd_bind_vertex_buffers(cmd, 0, &[dc.gpu_mesh.vertex_buffer], &[0]);
                device.cmd_bind_index_buffer(
                    cmd,
                    dc.gpu_mesh.index_buffer,
                    0,
                    vk::IndexType::UINT32,
                );
                device.cmd_draw_indexed(cmd, dc.gpu_mesh.index_count, 1, 0, 0, 0);
            }

            device.cmd_end_rendering(cmd);

            // DEPTH_ATTACHMENT -> SHADER_READ_ONLY
            transition_depth(
                device,
                cmd,
                shadow_map.image,
                vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            );
        }
    }
}

impl Drop for ShadowPass {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_pipeline(self.pipeline, None);
            self.device.destroy_pipeline_layout(self.layout, None);
        }
    }
}

fn transition_depth(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    image: vk::Image,
    from: vk::ImageLayout,
    to: vk::ImageLayout,
) {
    let (src_stage, src_access, dst_stage, dst_access) = match (from, to) {
        (vk::ImageLayout::UNDEFINED, vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL) => (
            vk::PipelineStageFlags2::TOP_OF_PIPE,
            vk::AccessFlags2::empty(),
            vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS,
            vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_READ
                | vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE,
        ),
        (vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL) => (
            vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS,
            vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE,
            vk::PipelineStageFlags2::FRAGMENT_SHADER,
            vk::AccessFlags2::SHADER_READ,
        ),
        _ => panic!("shadow transition: неизвестная пара {:?} -> {:?}", from, to),
    };

    let barrier = vk::ImageMemoryBarrier2::default()
        .src_stage_mask(src_stage)
        .src_access_mask(src_access)
        .dst_stage_mask(dst_stage)
        .dst_access_mask(dst_access)
        .old_layout(from)
        .new_layout(to)
        .image(image)
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask: vk::ImageAspectFlags::DEPTH,
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        });

    unsafe {
        device.cmd_pipeline_barrier2(
            cmd,
            &vk::DependencyInfo::default().image_memory_barriers(std::slice::from_ref(&barrier)),
        );
    }
}
