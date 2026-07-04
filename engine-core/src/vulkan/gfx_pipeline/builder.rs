use super::shader::ShaderModule;
use ash::vk;

pub struct PipelineBuilder<'a> {
    vert_spv: &'a [u8],
    frag_spv: Option<&'a [u8]>,
    color_formats: &'a [vk::Format],
    depth_format: vk::Format,
    cull_mode: vk::CullModeFlags,
    depth_test: bool,
    depth_write: bool,
    depth_compare: vk::CompareOp,
    depth_bias: Option<DepthBias>,
    vertex_bindings: &'a [vk::VertexInputBindingDescription],
    vertex_attributes: &'a [vk::VertexInputAttributeDescription],
    set_layouts: &'a [vk::DescriptorSetLayout],
    push_constant_ranges: &'a [vk::PushConstantRange],
    blend_attachments: Option<&'a [vk::PipelineColorBlendAttachmentState]>,
}

#[derive(Clone, Copy)]
pub struct DepthBias {
    pub constant_factor: f32,
    pub slope_factor: f32,
}

impl<'a> PipelineBuilder<'a> {
    pub fn fullscreen(vert_spv: &'a [u8], frag_spv: &'a [u8], color_formats: &'a [vk::Format]) -> Self {
        Self {
            vert_spv,
            frag_spv: Some(frag_spv),
            color_formats,
            depth_format: vk::Format::UNDEFINED,
            cull_mode: vk::CullModeFlags::NONE,
            depth_test: false,
            depth_write: false,
            depth_compare: vk::CompareOp::ALWAYS,
            depth_bias: None,
            vertex_bindings: &[],
            vertex_attributes: &[],
            set_layouts: &[],
            push_constant_ranges: &[],
            blend_attachments: None,
        }
    }

    pub fn mesh(
        vert_spv: &'a [u8],
        frag_spv: &'a [u8],
        color_formats: &'a [vk::Format],
        vertex_bindings: &'a [vk::VertexInputBindingDescription],
        vertex_attributes: &'a [vk::VertexInputAttributeDescription],
    ) -> Self {
        Self {
            vert_spv,
            frag_spv: Some(frag_spv),
            color_formats,
            depth_format: vk::Format::D32_SFLOAT,
            cull_mode: vk::CullModeFlags::BACK,
            depth_test: true,
            depth_write: true,
            depth_compare: vk::CompareOp::LESS,
            depth_bias: None,
            vertex_bindings,
            vertex_attributes,
            set_layouts: &[],
            push_constant_ranges: &[],
            blend_attachments: None,
        }
    }

    pub fn depth_only(
        vert_spv: &'a [u8],
        vertex_bindings: &'a [vk::VertexInputBindingDescription],
        vertex_attributes: &'a [vk::VertexInputAttributeDescription],
    ) -> Self {
        Self {
            vert_spv,
            frag_spv: None,
            color_formats: &[],
            depth_format: vk::Format::D32_SFLOAT,
            cull_mode: vk::CullModeFlags::NONE,
            depth_test: true,
            depth_write: true,
            depth_compare: vk::CompareOp::LESS_OR_EQUAL,
            depth_bias: None,
            vertex_bindings,
            vertex_attributes,
            set_layouts: &[],
            push_constant_ranges: &[],
            blend_attachments: None,
        }
    }

    pub fn cull_mode(mut self, mode: vk::CullModeFlags) -> Self {
        self.cull_mode = mode;
        self
    }

    pub fn depth_test(mut self, test: bool, write: bool) -> Self {
        self.depth_test = test;
        self.depth_write = write;
        self
    }

    pub fn depth_format(mut self, format: vk::Format) -> Self {
        self.depth_format = format;
        self
    }

    pub fn depth_compare(mut self, op: vk::CompareOp) -> Self {
        self.depth_compare = op;
        self
    }

    pub fn depth_bias(mut self, constant_factor: f32, slope_factor: f32) -> Self {
        self.depth_bias = Some(DepthBias { constant_factor, slope_factor });
        self
    }

    pub fn set_layouts(mut self, layouts: &'a [vk::DescriptorSetLayout]) -> Self {
        self.set_layouts = layouts;
        self
    }

    pub fn push_constants(mut self, ranges: &'a [vk::PushConstantRange]) -> Self {
        self.push_constant_ranges = ranges;
        self
    }

    pub fn blend_attachments(mut self, attachments: &'a [vk::PipelineColorBlendAttachmentState]) -> Self {
        self.blend_attachments = Some(attachments);
        self
    }

    pub fn build(self, device: &ash::Device) -> anyhow::Result<(vk::Pipeline, vk::PipelineLayout)> {
        let vert = ShaderModule::from_bytes(device, self.vert_spv)?;
        let frag = self.frag_spv.map(|spv| ShaderModule::from_bytes(device, spv)).transpose()?;

        let entry = c"main";
        let mut stages = vec![vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert.handle)
            .name(entry)];
        if let Some(ref f) = frag {
            stages.push(
                vk::PipelineShaderStageCreateInfo::default()
                    .stage(vk::ShaderStageFlags::FRAGMENT)
                    .module(f.handle)
                    .name(entry),
            );
        }

        let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_binding_descriptions(self.vertex_bindings)
            .vertex_attribute_descriptions(self.vertex_attributes);

        let input_assembly =
            vk::PipelineInputAssemblyStateCreateInfo::default().topology(vk::PrimitiveTopology::TRIANGLE_LIST);

        let viewport_state = vk::PipelineViewportStateCreateInfo::default().viewport_count(1).scissor_count(1);

        let mut rasterizer = vk::PipelineRasterizationStateCreateInfo::default()
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(self.cull_mode)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .line_width(1.0);
        if let Some(bias) = self.depth_bias {
            rasterizer = rasterizer
                .depth_bias_enable(true)
                .depth_bias_constant_factor(bias.constant_factor)
                .depth_bias_slope_factor(bias.slope_factor);
        }

        let multisampling =
            vk::PipelineMultisampleStateCreateInfo::default().rasterization_samples(vk::SampleCountFlags::TYPE_1);

        let default_blend: Vec<vk::PipelineColorBlendAttachmentState>;
        let blend_attachments = if let Some(explicit) = self.blend_attachments {
            explicit
        } else {
            default_blend = self
                .color_formats
                .iter()
                .map(|_| {
                    vk::PipelineColorBlendAttachmentState::default().color_write_mask(vk::ColorComponentFlags::RGBA)
                })
                .collect();
            &default_blend
        };

        let color_blending = vk::PipelineColorBlendStateCreateInfo::default().attachments(blend_attachments);

        let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let dynamic_state = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);

        let depth_stencil = vk::PipelineDepthStencilStateCreateInfo::default()
            .depth_test_enable(self.depth_test)
            .depth_write_enable(self.depth_write)
            .depth_compare_op(self.depth_compare)
            .depth_bounds_test_enable(false)
            .stencil_test_enable(false);

        let layout = unsafe {
            device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(self.set_layouts)
                    .push_constant_ranges(self.push_constant_ranges),
                None,
            )?
        };

        let mut rendering_info = vk::PipelineRenderingCreateInfo::default()
            .color_attachment_formats(self.color_formats)
            .depth_attachment_format(self.depth_format);

        let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&stages)
            .vertex_input_state(&vertex_input)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterizer)
            .multisample_state(&multisampling)
            .color_blend_state(&color_blending)
            .dynamic_state(&dynamic_state)
            .depth_stencil_state(&depth_stencil)
            .layout(layout)
            .push_next(&mut rendering_info);

        let pipeline = unsafe {
            device
                .create_graphics_pipelines(vk::PipelineCache::null(), std::slice::from_ref(&pipeline_info), None)
                .map_err(|(_, e)| {
                    device.destroy_pipeline_layout(layout, None);
                    e
                })?[0]
        };

        Ok((pipeline, layout))
    }
}

pub mod cmd {
    use ash::vk;

    pub fn begin_rendering_clear(
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        view: vk::ImageView,
        extent: vk::Extent2D,
        clear: [f32; 4],
    ) {
        let color_attachment = color_attachment_clear(view, clear);
        begin_rendering_impl(device, cmd, &[color_attachment], None, extent);
    }

    pub fn begin_rendering_discard(
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        view: vk::ImageView,
        extent: vk::Extent2D,
    ) {
        let color_attachment = vk::RenderingAttachmentInfo::default()
            .image_view(view)
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::DONT_CARE)
            .store_op(vk::AttachmentStoreOp::STORE);
        begin_rendering_impl(device, cmd, &[color_attachment], None, extent);
    }

    pub fn begin_rendering_load(
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        view: vk::ImageView,
        extent: vk::Extent2D,
    ) {
        let color_attachment = vk::RenderingAttachmentInfo::default()
            .image_view(view)
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::LOAD)
            .store_op(vk::AttachmentStoreOp::STORE);
        begin_rendering_impl(device, cmd, &[color_attachment], None, extent);
    }

    pub fn begin_rendering_with_depth(
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        color_views: &[(vk::ImageView, [f32; 4])],
        depth_view: vk::ImageView,
        depth_load_op: vk::AttachmentLoadOp,
        extent: vk::Extent2D,
    ) {
        let color_attachments: Vec<_> =
            color_views.iter().map(|&(view, clear)| color_attachment_clear(view, clear)).collect();

        let depth_attachment = vk::RenderingAttachmentInfo::default()
            .image_view(depth_view)
            .image_layout(vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
            .load_op(depth_load_op)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue { depth_stencil: vk::ClearDepthStencilValue { depth: 1.0, stencil: 0 } });

        begin_rendering_impl(device, cmd, &color_attachments, Some(depth_attachment), extent);
    }

    pub fn begin_rendering_depth_only(
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        depth_view: vk::ImageView,
        extent: vk::Extent2D,
    ) {
        let depth_attachment = depth_attachment_clear(depth_view);
        begin_rendering_impl(device, cmd, &[], Some(depth_attachment), extent);
    }

    fn begin_rendering_impl(
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        color_attachments: &[vk::RenderingAttachmentInfo],
        depth_attachment: Option<vk::RenderingAttachmentInfo>,
        extent: vk::Extent2D,
    ) {
        let mut rendering_info = vk::RenderingInfo::default()
            .render_area(vk::Rect2D { offset: vk::Offset2D { x: 0, y: 0 }, extent })
            .layer_count(1)
            .color_attachments(color_attachments);

        if let Some(ref depth) = depth_attachment {
            rendering_info = rendering_info.depth_attachment(depth);
        }

        unsafe {
            device.cmd_begin_rendering(cmd, &rendering_info);
            set_full_viewport_scissor(device, cmd, extent);
        }
    }

    pub fn set_full_viewport_scissor(device: &ash::Device, cmd: vk::CommandBuffer, extent: vk::Extent2D) {
        unsafe {
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
        }
    }

    pub fn color_attachment_clear(view: vk::ImageView, clear: [f32; 4]) -> vk::RenderingAttachmentInfo<'static> {
        vk::RenderingAttachmentInfo::default()
            .image_view(view)
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue { color: vk::ClearColorValue { float32: clear } })
    }

    pub fn depth_attachment_clear(view: vk::ImageView) -> vk::RenderingAttachmentInfo<'static> {
        vk::RenderingAttachmentInfo::default()
            .image_view(view)
            .image_layout(vk::ImageLayout::DEPTH_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(vk::ClearValue { depth_stencil: vk::ClearDepthStencilValue { depth: 1.0, stencil: 0 } })
    }
}

pub mod descriptor {
    use ash::vk;

    pub fn alloc_sets(
        device: &ash::Device,
        layout: vk::DescriptorSetLayout,
        pool_sizes: &[vk::DescriptorPoolSize],
        count: u32,
    ) -> anyhow::Result<(vk::DescriptorPool, Vec<vk::DescriptorSet>)> {
        let pool = unsafe {
            device.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default().pool_sizes(pool_sizes).max_sets(count),
                None,
            )?
        };

        let layouts = vec![layout; count as usize];
        let sets = unsafe {
            device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default().descriptor_pool(pool).set_layouts(&layouts),
            )?
        };

        Ok((pool, sets))
    }

    pub fn alloc_single_set(
        device: &ash::Device,
        layout: vk::DescriptorSetLayout,
        pool_sizes: &[vk::DescriptorPoolSize],
    ) -> anyhow::Result<(vk::DescriptorPool, vk::DescriptorSet)> {
        let (pool, mut sets) = alloc_sets(device, layout, pool_sizes, 1)?;
        Ok((pool, sets.remove(0)))
    }
}
