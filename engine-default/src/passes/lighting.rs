use ash::vk;
use engine_core::assets::gpu_server::GpuAssetServer;
use engine_core::render::gfx::{CommandEncoder, PipelineId, ShaderStage};
use engine_core::render::resource::ResourceHandle;
use engine_core::render::world::{ExtractedCamera, ExtractedLights, RenderWorld};
use engine_core::vulkan::gfx_pipeline::builder::descriptor::alloc_single_set;
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
    pub descriptor_pool: vk::DescriptorPool,
    pub descriptor_set_layout: vk::DescriptorSetLayout,
    pub descriptor_set: vk::DescriptorSet,
    pub sampler: vk::Sampler,
    pub shadow_sampler: vk::Sampler,
    pub light_buffer: LightBuffer,
    device: ash::Device,
}

impl LightingPass {
    pub fn new(
        gpu: &mut GpuAssetServer,
        device: &ash::Device,
        physical_device: vk::PhysicalDevice,
        instance: &ash::Instance,
        hdr_format: vk::Format,
    ) -> anyhow::Result<Self> {
        let light_buffer = LightBuffer::new(device, physical_device, instance)?;

        let sampler = unsafe {
            device.create_sampler(
                &vk::SamplerCreateInfo::default()
                    .mag_filter(vk::Filter::NEAREST)
                    .min_filter(vk::Filter::NEAREST)
                    .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE),
                None,
            )?
        };

        let shadow_sampler = unsafe {
            device.create_sampler(
                &vk::SamplerCreateInfo::default()
                    .mag_filter(vk::Filter::LINEAR)
                    .min_filter(vk::Filter::LINEAR)
                    .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_BORDER)
                    .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_BORDER)
                    .border_color(vk::BorderColor::FLOAT_OPAQUE_WHITE)
                    .compare_enable(true)
                    .compare_op(vk::CompareOp::LESS_OR_EQUAL),
                None,
            )?
        };

        let bindings = [
            make_image_binding(0),
            make_image_binding(1),
            make_image_binding(2),
            vk::DescriptorSetLayoutBinding::default()
                .binding(3)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            make_image_binding(4),
        ];

        let descriptor_set_layout = unsafe {
            device
                .create_descriptor_set_layout(&vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings), None)?
        };

        let pool_sizes = [
            vk::DescriptorPoolSize { ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER, descriptor_count: 4 },
            vk::DescriptorPoolSize { ty: vk::DescriptorType::UNIFORM_BUFFER, descriptor_count: 1 },
        ];
        let (descriptor_pool, descriptor_set) = alloc_single_set(device, descriptor_set_layout, &pool_sizes)?;

        let buf_info = vk::DescriptorBufferInfo::default()
            .buffer(light_buffer.buffer)
            .offset(0)
            .range(size_of::<LightingUbo>() as vk::DeviceSize);

        let ubo_write = vk::WriteDescriptorSet::default()
            .dst_set(descriptor_set)
            .dst_binding(3)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .buffer_info(std::slice::from_ref(&buf_info));

        unsafe { device.update_descriptor_sets(&[ubo_write], &[]) };

        let push_range = vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .offset(0)
            .size(size_of::<LightingPC>() as u32);

        let handle = gpu.shaders.by_name("lighting").expect("шейдер 'lighting' не зарегистрирован");
        let (vert_spv, frag_spv) = gpu.shaders.load_spv(handle)?;
        let vert_spv = vert_spv.to_vec();
        let frag_spv = frag_spv.expect("'lighting' должен иметь frag").to_vec();

        let pipeline = gpu.create_fullscreen_pipeline(
            &vert_spv,
            &frag_spv,
            std::slice::from_ref(&hdr_format),
            std::slice::from_ref(&descriptor_set_layout),
            std::slice::from_ref(&push_range),
            None,
        )?;

        Ok(Self {
            pipeline,
            descriptor_pool,
            descriptor_set_layout,
            descriptor_set,
            sampler,
            shadow_sampler,
            light_buffer,
            device: device.clone(),
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

impl Drop for LightingPass {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_descriptor_pool(self.descriptor_pool, None);
            self.device.destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            self.device.destroy_sampler(self.sampler, None);
            self.device.destroy_sampler(self.shadow_sampler, None);
        }
    }
}

fn make_image_binding(binding: u32) -> vk::DescriptorSetLayoutBinding<'static> {
    vk::DescriptorSetLayoutBinding::default()
        .binding(binding)
        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .descriptor_count(1)
        .stage_flags(vk::ShaderStageFlags::FRAGMENT)
}
