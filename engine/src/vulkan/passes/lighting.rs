use crate::assets::ShaderRegistry;
use crate::lighting::buffer::LightBuffer;
use crate::render_graph::GpuImage;
use crate::vulkan::pipeline::builder::{cmd, descriptor, PipelineBuilder};
use crate::vulkan::Camera;
use ash::vk;
use cmd::begin_rendering_discard;
use descriptor::alloc_single_set;

#[repr(C)]
struct LightingPC {
    inv_proj: [[f32; 4]; 4],
    inv_view: [[f32; 4]; 4],
    viewport: [f32; 2],
    _pad: [f32; 2],
}

pub struct LightingPass {
    pub pipeline: vk::Pipeline,
    pub layout: vk::PipelineLayout,
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
        device: &ash::Device,
        physical_device: vk::PhysicalDevice,
        instance: &ash::Instance,
        hdr_format: vk::Format,
        registry: &mut ShaderRegistry,
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
            make_image_binding(0), // albedo
            make_image_binding(1), // normal
            make_image_binding(2), // depth
            vk::DescriptorSetLayoutBinding::default()
                .binding(3)
                .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
            make_image_binding(4), // shadow map
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
            .range(size_of::<crate::lighting::buffer::LightingUbo>() as vk::DeviceSize);

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

        let handle = registry.by_name("lighting").expect("шейдер 'lighting' не зарегистрирован");
        let (vert_spv, frag_spv) = registry.load_spv(handle)?;
        let vert_spv = vert_spv.to_vec();
        let frag_spv = frag_spv.expect("'lighting' должен иметь frag").to_vec();

        let (pipeline, layout) = PipelineBuilder::fullscreen(&vert_spv, &frag_spv, std::slice::from_ref(&hdr_format))
            .set_layouts(std::slice::from_ref(&descriptor_set_layout))
            .push_constants(std::slice::from_ref(&push_range))
            .build(device)?;

        log::debug!("LightingPass создан");
        Ok(Self {
            pipeline,
            layout,
            descriptor_pool,
            descriptor_set_layout,
            descriptor_set,
            sampler,
            shadow_sampler,
            light_buffer,
            device: device.clone(),
        })
    }

    pub fn upload_lights(&self, data: &crate::lighting::buffer::LightingUbo) {
        self.light_buffer.upload(data);
    }

    pub fn record(&self, device: &ash::Device, cmd: vk::CommandBuffer, hdr: &impl GpuImage, camera: &Camera) {
        let extent = hdr.extent();

        begin_rendering_discard(device, cmd, hdr.view(), extent);

        unsafe {
            device.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline);
            device.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::GRAPHICS,
                self.layout,
                0,
                &[self.descriptor_set],
                &[],
            );

            let aspect = extent.width as f32 / extent.height as f32;
            let view = glam::Mat4::look_at_rh(camera.eye, camera.target, camera.up);
            let mut proj = glam::Mat4::perspective_rh(camera.fov_y, aspect, camera.z_near, camera.z_far);
            proj.y_axis.y *= -1.0;

            let pc = LightingPC {
                inv_proj: proj.inverse().to_cols_array_2d(),
                inv_view: view.inverse().to_cols_array_2d(),
                viewport: [extent.width as f32, extent.height as f32],
                _pad: [0.0; 2],
            };
            let pc_bytes = std::slice::from_raw_parts(&pc as *const LightingPC as *const u8, size_of::<LightingPC>());
            device.cmd_push_constants(cmd, self.layout, vk::ShaderStageFlags::FRAGMENT, 0, pc_bytes);

            device.cmd_draw(cmd, 3, 1, 0, 0);
            device.cmd_end_rendering(cmd);
        }
    }
}

impl Drop for LightingPass {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_pipeline(self.pipeline, None);
            self.device.destroy_pipeline_layout(self.layout, None);
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
