use crate::assets::ShaderRegistry;
use crate::render::resource::GpuImage;
use crate::vulkan::core::sampler;
use crate::vulkan::gfx_pipeline::builder::{cmd, descriptor, PipelineBuilder};
use ash::vk;
use descriptor::alloc_single_set;

#[repr(C)]
struct PostProcessPC {
    texel_size: [f32; 2],
    exposure: f32,
    flags: u32,
}

pub struct PostProcessPass {
    pub pipeline: vk::Pipeline,
    pub layout: vk::PipelineLayout,
    pub descriptor_pool: vk::DescriptorPool,
    pub descriptor_set_layout: vk::DescriptorSetLayout,
    pub descriptor_set: vk::DescriptorSet,
    pub sampler: vk::Sampler,
    device: ash::Device,
}

impl PostProcessPass {
    pub fn new(
        device: &ash::Device,
        swapchain_format: vk::Format,
        registry: &mut ShaderRegistry,
    ) -> anyhow::Result<Self> {
        let sampler = sampler::create_linear_clamp_sampler(device)?;

        let binding = vk::DescriptorSetLayoutBinding::default()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT);

        let descriptor_set_layout = unsafe {
            device.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(std::slice::from_ref(&binding)),
                None,
            )?
        };

        let pool_size = vk::DescriptorPoolSize { ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER, descriptor_count: 1 };
        let (descriptor_pool, descriptor_set) = alloc_single_set(device, descriptor_set_layout, &[pool_size])?;

        let push_range = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .offset(0)
            .size(size_of::<PostProcessPC>() as u32);

        let handle = registry.by_name("post_process").expect("шейдер 'post_process' не зарегистрирован");
        let (vert_spv, frag_spv) = registry.load_spv(handle)?;
        let vert_spv = vert_spv.to_vec();
        let frag_spv = frag_spv.expect("'post_process' должен иметь frag").to_vec();

        let color_formats = [swapchain_format];
        let (pipeline, layout) = PipelineBuilder::fullscreen(&vert_spv, &frag_spv, &color_formats)
            .set_layouts(std::slice::from_ref(&descriptor_set_layout))
            .push_constants(std::slice::from_ref(&push_range))
            .build(device)?;

        log::debug!("PostProcessPass создан");
        Ok(Self {
            pipeline,
            layout,
            descriptor_pool,
            descriptor_set_layout,
            descriptor_set,
            sampler,
            device: device.clone(),
        })
    }

    pub fn record_to_target(
        &self,
        device: &ash::Device,
        cmd_buf: vk::CommandBuffer,
        target: &impl GpuImage,
        exposure: f32,
    ) {
        let extent = target.extent();

        cmd::begin_rendering_discard(device, cmd_buf, target.view(), extent);

        unsafe {
            device.cmd_bind_pipeline(cmd_buf, vk::PipelineBindPoint::GRAPHICS, self.pipeline);
            device.cmd_bind_descriptor_sets(
                cmd_buf,
                vk::PipelineBindPoint::GRAPHICS,
                self.layout,
                0,
                &[self.descriptor_set],
                &[],
            );

            let pc = PostProcessPC {
                texel_size: [1.0 / extent.width as f32, 1.0 / extent.height as f32],
                exposure,
                flags: 0,
            };
            let pc_bytes =
                std::slice::from_raw_parts(&pc as *const PostProcessPC as *const u8, size_of::<PostProcessPC>());
            device.cmd_push_constants(cmd_buf, self.layout, vk::ShaderStageFlags::FRAGMENT, 0, pc_bytes);

            device.cmd_draw(cmd_buf, 3, 1, 0, 0);
            device.cmd_end_rendering(cmd_buf);
        }
    }
}

impl Drop for PostProcessPass {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_pipeline(self.pipeline, None);
            self.device.destroy_pipeline_layout(self.layout, None);
            self.device.destroy_descriptor_pool(self.descriptor_pool, None);
            self.device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            self.device.destroy_sampler(self.sampler, None);
        }
    }
}
