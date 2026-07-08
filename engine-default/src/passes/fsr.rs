use ash::vk;
use engine_core::assets::gpu_server::GpuAssetServer;
use engine_core::render::gfx::format::Format;
use engine_core::render::gfx::{CommandEncoder, PipelineId, PushConstantRange, ShaderStage};
use engine_core::render::resource::ResourceHandle;
use engine_core::render::world::{ExtractedRenderSettings, RenderWorld};
use engine_core::vulkan::core::sampler;
use engine_core::vulkan::gfx_pipeline::builder::descriptor::alloc_sets;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct EasuPC {
    pub con0: [u32; 4],
    pub con1: [u32; 4],
    pub con2: [u32; 4],
    pub con3: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct RcasPC {
    pub con0: [u32; 4],
}

pub struct FsrPass {
    easu_pipeline: PipelineId,
    pub easu_descriptor_set_layout: vk::DescriptorSetLayout,
    pub easu_descriptor_set: vk::DescriptorSet,

    rcas_pipeline: PipelineId,
    pub rcas_descriptor_set: vk::DescriptorSet,

    pub sampler: vk::Sampler,
    descriptor_pool: vk::DescriptorPool,
    device: ash::Device,
}

impl FsrPass {
    pub fn new(gpu: &mut GpuAssetServer, device: &ash::Device, output_format: Format) -> anyhow::Result<Self> {
        let sampler = sampler::create_linear_clamp_sampler(device)?;

        let pool_sizes =
            [vk::DescriptorPoolSize { ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER, descriptor_count: 2 }];
        let dsl = create_sampled_image_dsl(device)?;
        let (descriptor_pool, sets) = alloc_sets(device, dsl, &pool_sizes, 2)?;
        let easu_descriptor_set = sets[0];
        let rcas_descriptor_set = sets[1];

        let easu_push = PushConstantRange::of::<EasuPC>(ShaderStage::Fragment);
        let rcas_push = PushConstantRange::of::<RcasPC>(ShaderStage::Fragment);

        let easu_pipeline = build_stage_pipeline(gpu, "fsr_easu", dsl, easu_push, output_format)?;
        let rcas_pipeline = build_stage_pipeline(gpu, "fsr_rcas", dsl, rcas_push, output_format)?;

        Ok(Self {
            easu_pipeline,
            easu_descriptor_set_layout: dsl,
            easu_descriptor_set,
            rcas_pipeline,
            rcas_descriptor_set,
            sampler,
            descriptor_pool,
            device: device.clone(),
        })
    }

    pub fn record_easu_pass(
        &self,
        enc: &mut CommandEncoder,
        rw: &RenderWorld,
        _gpu: &GpuAssetServer,
        src: ResourceHandle,
        dst: ResourceHandle,
    ) -> anyhow::Result<()> {
        let settings = rw.get::<ExtractedRenderSettings>().cloned().unwrap_or_default();
        let input_extent = enc.extent_of(src);
        let (ow, oh) = settings.output_size;
        let pc = compute_easu_con((input_extent[0], input_extent[1]), (input_extent[0], input_extent[1]), (ow, oh));

        enc.begin_rendering_discard(dst);
        enc.bind_pipeline(self.easu_pipeline);
        enc.bind_descriptor_sets(self.easu_pipeline, &[self.easu_descriptor_set]);
        enc.push_constants(self.easu_pipeline, ShaderStage::Fragment, &pc);
        enc.draw(3);
        enc.end_rendering();
        Ok(())
    }

    pub fn record_rcas_pass(
        &self,
        enc: &mut CommandEncoder,
        rw: &RenderWorld,
        _gpu: &GpuAssetServer,
        dst: ResourceHandle,
    ) -> anyhow::Result<()> {
        let settings = rw.get::<ExtractedRenderSettings>().cloned().unwrap_or_default();
        let pc = compute_rcas_con(settings.fsr_sharpness);

        enc.begin_rendering_discard(dst);
        enc.bind_pipeline(self.rcas_pipeline);
        enc.bind_descriptor_sets(self.rcas_pipeline, &[self.rcas_descriptor_set]);
        enc.push_constants(self.rcas_pipeline, ShaderStage::Fragment, &pc);
        enc.draw(3);
        enc.end_rendering();
        Ok(())
    }
}

impl Drop for FsrPass {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_descriptor_set_layout(self.easu_descriptor_set_layout, None);
            self.device.destroy_descriptor_pool(self.descriptor_pool, None);
            self.device.destroy_sampler(self.sampler, None);
        }
    }
}

fn build_stage_pipeline(
    gpu: &mut GpuAssetServer,
    shader_name: &str,
    dsl: vk::DescriptorSetLayout,
    push_range: PushConstantRange,
    output_format: Format,
) -> anyhow::Result<PipelineId> {
    let handle =
        gpu.shaders.by_name(shader_name).unwrap_or_else(|| panic!("шейдер '{shader_name}' не зарегистрирован"));
    let (vert, frag) = gpu.shaders.load_spv(handle)?;
    let vert = vert.to_vec();
    let frag = frag.expect("FSR-шейдер должен иметь frag").to_vec();

    gpu.create_fullscreen_pipeline(
        &vert,
        &frag,
        std::slice::from_ref(&output_format),
        std::slice::from_ref(&dsl),
        std::slice::from_ref(&push_range),
        None,
    )
}

fn create_sampled_image_dsl(device: &ash::Device) -> anyhow::Result<vk::DescriptorSetLayout> {
    let binding = vk::DescriptorSetLayoutBinding::default()
        .binding(0)
        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .descriptor_count(1)
        .stage_flags(vk::ShaderStageFlags::FRAGMENT);
    Ok(unsafe {
        device.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo::default().bindings(std::slice::from_ref(&binding)),
            None,
        )?
    })
}

pub fn compute_easu_con(input_viewport: (f32, f32), input_size: (f32, f32), output_size: (f32, f32)) -> EasuPC {
    let (ivw, ivh) = input_viewport;
    let (isw, ish) = input_size;
    let (osw, osh) = output_size;
    EasuPC {
        con0: [
            f32_to_bits(ivw / osw),
            f32_to_bits(ivh / osh),
            f32_to_bits(0.5 * ivw / osw - 0.5),
            f32_to_bits(0.5 * ivh / osh - 0.5),
        ],
        con1: [
            f32_to_bits(1.0 / isw),
            f32_to_bits(1.0 / ish),
            f32_to_bits(1.0 / isw),
            f32_to_bits(-1.0 / ish),
        ],
        con2: [
            f32_to_bits(-1.0 / isw),
            f32_to_bits(2.0 / ish),
            f32_to_bits(1.0 / isw),
            f32_to_bits(2.0 / ish),
        ],
        con3: [f32_to_bits(0.0 / isw), f32_to_bits(4.0 / ish), 0, 0],
    }
}

pub fn compute_rcas_con(sharpness: f32) -> RcasPC {
    RcasPC { con0: [f32_to_bits(sharpness.exp2()), 0, 0, 0] }
}

#[inline]
fn f32_to_bits(v: f32) -> u32 {
    v.to_bits()
}
