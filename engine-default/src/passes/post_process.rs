use ash::vk;
use engine_core::assets::gpu_server::GpuAssetServer;
use engine_core::render::gfx::format::Format;
use engine_core::render::gfx::{CommandEncoder, PipelineId, PushConstantRange, ShaderStage};
use engine_core::render::resource::ResourceHandle;
use engine_core::render::world::{ExtractedRenderSettings, RenderWorld};
use engine_core::vulkan::core::sampler;
use engine_core::vulkan::gfx_pipeline::builder::descriptor::alloc_single_set;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct PostProcessPC {
    texel_size: [f32; 2],
    exposure: f32,
    flags: u32,
}

pub struct PostProcessPass {
    pipeline: PipelineId,
    pub descriptor_pool: vk::DescriptorPool,
    pub descriptor_set_layout: vk::DescriptorSetLayout,
    pub descriptor_set: vk::DescriptorSet,
    pub sampler: vk::Sampler,
    device: ash::Device,
}

impl PostProcessPass {
    pub fn new(gpu: &mut GpuAssetServer, device: &ash::Device, swapchain_format: Format) -> anyhow::Result<Self> {
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

        let push_range = PushConstantRange::of::<PostProcessPC>(ShaderStage::Fragment);

        let handle = gpu.shaders.by_name("post_process").expect("шейдер 'post_process' не зарегистрирован");
        let (vert_spv, frag_spv) = gpu.shaders.load_spv(handle)?;
        let vert_spv = vert_spv.to_vec();
        let frag_spv = frag_spv.expect("'post_process' должен иметь frag").to_vec();

        let pipeline = gpu.create_fullscreen_pipeline(
            &vert_spv,
            &frag_spv,
            std::slice::from_ref(&swapchain_format),
            std::slice::from_ref(&descriptor_set_layout),
            std::slice::from_ref(&push_range),
            None,
        )?;

        Ok(Self { pipeline, descriptor_pool, descriptor_set_layout, descriptor_set, sampler, device: device.clone() })
    }

    pub fn record(
        &self,
        enc: &mut CommandEncoder,
        rw: &RenderWorld,
        _gpu: &GpuAssetServer,
        ldr: ResourceHandle,
    ) -> anyhow::Result<()> {
        let settings = rw.get::<ExtractedRenderSettings>().cloned().unwrap_or_default();
        let extent = enc.extent_of(ldr);

        enc.begin_rendering_discard(ldr);
        enc.bind_pipeline(self.pipeline);
        enc.bind_descriptor_sets(self.pipeline, &[self.descriptor_set]);

        let pc =
            PostProcessPC { texel_size: [1.0 / extent[0], 1.0 / extent[1]], exposure: settings.exposure, flags: 0 };
        enc.push_constants(self.pipeline, ShaderStage::Fragment, &pc);
        enc.draw(3);
        enc.end_rendering();
        Ok(())
    }
}

impl Drop for PostProcessPass {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_descriptor_pool(self.descriptor_pool, None);
            self.device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            self.device.destroy_sampler(self.sampler, None);
        }
    }
}
