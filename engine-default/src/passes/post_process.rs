use engine_core::assets::gpu_server::GpuAssetServer;
use engine_core::render::gfx::format::Format;
use engine_core::render::gfx::{
    CommandEncoder, DescriptorSetDesc, DescriptorSetId, PipelineId, PushConstantRange, SamplerDesc, SamplerId,
    ShaderStage,
};
use engine_core::render::resource::ResourceHandle;
use engine_core::render::world::{ExtractedRenderSettings, RenderWorld};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct PostProcessPC {
    texel_size: [f32; 2],
    exposure: f32,
    flags: u32,
}

pub struct PostProcessPass {
    pipeline: PipelineId,
    pub descriptor_set: DescriptorSetId,
    pub sampler: SamplerId,
    device: ash::Device,
}

impl PostProcessPass {
    pub fn new(gpu: &mut GpuAssetServer, device: &ash::Device, swapchain_format: Format) -> anyhow::Result<Self> {
        let sampler_id = gpu.create_sampler(SamplerDesc::linear_clamp())?;
        let set_id =
            gpu.create_descriptor_set(DescriptorSetDesc::new().with_sampled_image(0, ShaderStage::Fragment))?;

        let push_range = PushConstantRange::of::<PostProcessPC>(ShaderStage::Fragment);

        let handle = gpu.shaders.by_name("post_process").expect("шейдер 'post_process' не зарегистрирован");
        let (vert_spv, frag_spv) = gpu.shaders.load_spv(handle)?;
        let vert_spv = vert_spv.to_vec();
        let frag_spv = frag_spv.expect("'post_process' должен иметь frag").to_vec();

        let pipeline = gpu.create_fullscreen_pipeline(
            &vert_spv,
            &frag_spv,
            std::slice::from_ref(&swapchain_format),
            std::slice::from_ref(&set_id),
            std::slice::from_ref(&push_range),
            None,
        )?;

        Ok(Self { pipeline, descriptor_set: set_id, sampler: sampler_id, device: device.clone() })
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
