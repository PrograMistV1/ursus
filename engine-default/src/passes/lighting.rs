use engine_core::assets::gpu_server::GpuAssetServer;
use engine_core::render::gfx::format::Format;
use engine_core::render::gfx::{
    CommandEncoder, DescriptorSetDesc, DescriptorSetId, PipelineId, PushConstantRange, SamplerDesc, SamplerId,
    ShaderStage,
};
use engine_core::render::resource::ResourceHandle;
use engine_core::render::world::{ExtractedCamera, ExtractedLights, RenderWorld};
use engine_core::vulkan::resources::light_buffer::{LightBuffer, LightingUbo};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct LightingPC {
    inv_proj: [[f32; 4]; 4],
    inv_view: [[f32; 4]; 4],
    viewport: [f32; 2],
    _pad: [f32; 2],
}

pub struct LightingPass {
    pipeline: PipelineId,
    pub descriptor_set: DescriptorSetId,
    pub sampler: SamplerId,
    pub shadow_sampler: SamplerId,
    pub light_buffer: LightBuffer,
}

impl LightingPass {
    pub fn new(gpu: &mut GpuAssetServer, hdr_format: Format) -> anyhow::Result<Self> {
        let light_buffer = gpu.create_light_buffer()?;

        let sampler_id = gpu.create_sampler(SamplerDesc::nearest_clamp())?;
        let shadow_sampler_id = gpu.create_sampler(SamplerDesc::shadow_compare())?;

        let set_id = gpu.create_descriptor_set(
            DescriptorSetDesc::new()
                .with_sampled_image(0, ShaderStage::Fragment)
                .with_sampled_image(1, ShaderStage::Fragment)
                .with_sampled_image(2, ShaderStage::Fragment)
                .with_uniform_buffer::<LightingUbo>(3, ShaderStage::Fragment)
                .with_sampled_image(4, ShaderStage::Fragment),
        )?;

        gpu.bind_uniform_buffer(set_id, 3, light_buffer.buffer(), light_buffer.size());

        let push_range = PushConstantRange::of::<LightingPC>(ShaderStage::Fragment);

        let handle = gpu.shaders.by_name("lighting").expect("шейдер 'lighting' не зарегистрирован");
        let (vert_spv, frag_spv) = gpu.shaders.load_spv(handle)?;
        let vert_spv = vert_spv.to_vec();
        let frag_spv = frag_spv.expect("'lighting' должен иметь frag").to_vec();

        let pipeline = gpu.create_fullscreen_pipeline(
            &vert_spv,
            &frag_spv,
            std::slice::from_ref(&hdr_format),
            std::slice::from_ref(&set_id),
            std::slice::from_ref(&push_range),
            None,
        )?;

        Ok(Self {
            pipeline,
            descriptor_set: set_id,
            sampler: sampler_id,
            shadow_sampler: shadow_sampler_id,
            light_buffer,
        })
    }

    pub fn record(
        &self,
        enc: &mut CommandEncoder,
        rw: &RenderWorld,
        _gpu: &GpuAssetServer,
        hdr: ResourceHandle,
    ) -> anyhow::Result<()> {
        let camera = rw.get::<ExtractedCamera>().cloned().unwrap_or_default();
        let lights = rw.get::<ExtractedLights>().cloned().unwrap_or_default();

        let ubo = LightingUbo {
            directional: lights.directional,
            point_lights: lights.point_lights,
            point_light_count: lights.point_light_count,
            _pad: [0; 3],
            light_space_matrix: lights.light_view_proj.to_cols_array_2d(),
        };
        self.light_buffer.upload(&ubo);

        enc.begin_rendering_discard(hdr);
        enc.bind_pipeline(self.pipeline);
        enc.bind_descriptor_sets(self.pipeline, &[self.descriptor_set]);

        let pc = LightingPC {
            inv_proj: camera.proj.inverse().to_cols_array_2d(),
            inv_view: camera.view.inverse().to_cols_array_2d(),
            viewport: enc.extent_of(hdr),
            _pad: [0.0; 2],
        };
        enc.push_constants(self.pipeline, ShaderStage::Fragment, &pc);
        enc.draw(3);
        enc.end_rendering();
        Ok(())
    }
}
