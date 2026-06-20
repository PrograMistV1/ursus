use crate::vulkan::core::sampler;
use crate::vulkan::GpuTexture;
use ash::vk;

pub const MAX_TEXTURES: u32 = 4096;

pub struct BindlessSet {
    pub layout: vk::DescriptorSetLayout,
    pub set: vk::DescriptorSet,
    pool: vk::DescriptorPool,
    pub sampler: vk::Sampler,
    next_slot: u32,
    owned_textures: Vec<GpuTexture>,
    device: ash::Device,
}

impl BindlessSet {
    pub fn new(
        device: &ash::Device,
        physical_device: vk::PhysicalDevice,
        instance: &ash::Instance,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
    ) -> anyhow::Result<Self> {
        let max_aniso =
            unsafe { instance.get_physical_device_properties(physical_device).limits.max_sampler_anisotropy.min(16.0) };
        let sampler = sampler::create_linear_repeat_aniso_sampler(device, max_aniso)?;

        let binding_flags = [
            vk::DescriptorBindingFlags::empty(),
            vk::DescriptorBindingFlags::PARTIALLY_BOUND
                | vk::DescriptorBindingFlags::VARIABLE_DESCRIPTOR_COUNT
                | vk::DescriptorBindingFlags::UPDATE_AFTER_BIND,
        ];
        let mut binding_flags_info =
            vk::DescriptorSetLayoutBindingFlagsCreateInfo::default().binding_flags(&binding_flags);

        let bindings = [
            vk::DescriptorSetLayoutBinding::default()
                .binding(0)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                .immutable_samplers(std::slice::from_ref(&sampler)),
            vk::DescriptorSetLayoutBinding::default()
                .binding(1)
                .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(MAX_TEXTURES)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT),
        ];

        let layout = unsafe {
            device.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default()
                    .bindings(&bindings)
                    .flags(vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL)
                    .push_next(&mut binding_flags_info),
                None,
            )?
        };

        let pool_sizes = [
            vk::DescriptorPoolSize { ty: vk::DescriptorType::SAMPLER, descriptor_count: 1 },
            vk::DescriptorPoolSize { ty: vk::DescriptorType::SAMPLED_IMAGE, descriptor_count: MAX_TEXTURES },
        ];
        let pool = unsafe {
            device.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .pool_sizes(&pool_sizes)
                    .max_sets(1)
                    .flags(vk::DescriptorPoolCreateFlags::UPDATE_AFTER_BIND),
                None,
            )?
        };

        let mut variable_count_info = vk::DescriptorSetVariableDescriptorCountAllocateInfo::default()
            .descriptor_counts(std::slice::from_ref(&MAX_TEXTURES));

        let set = unsafe {
            device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(pool)
                    .set_layouts(std::slice::from_ref(&layout))
                    .push_next(&mut variable_count_info),
            )?[0]
        };

        let mut bindless =
            Self { layout, set, pool, sampler, next_slot: 0, owned_textures: Vec::new(), device: device.clone() };

        let white = GpuTexture::upload(
            device,
            physical_device,
            instance,
            command_pool,
            queue,
            &[255u8, 255, 255, 255],
            1,
            1,
            vk::Format::R8G8B8A8_SRGB,
            "white_fallback",
        )?;
        let slot = bindless.register_view(white.view);
        assert_eq!(slot, 0, "white fallback должен быть слотом 0");

        bindless.owned_textures.push(white);

        log::info!("BindlessSet создан (MAX_TEXTURES={})", MAX_TEXTURES);
        Ok(bindless)
    }

    pub fn register_view(&mut self, view: vk::ImageView) -> u32 {
        let slot = self.next_slot;
        assert!(slot < MAX_TEXTURES, "bindless texture array переполнен");

        let image_info =
            vk::DescriptorImageInfo::default().image_view(view).image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);

        let write = vk::WriteDescriptorSet::default()
            .dst_set(self.set)
            .dst_binding(1)
            .dst_array_element(slot)
            .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
            .image_info(std::slice::from_ref(&image_info));

        unsafe { self.device.update_descriptor_sets(std::slice::from_ref(&write), &[]) };

        self.next_slot += 1;
        slot
    }

    pub fn register_view_at(&mut self, slot: u32, view: vk::ImageView) {
        assert!(slot < MAX_TEXTURES, "bindless texture array переполнен");

        let image_info =
            vk::DescriptorImageInfo::default().image_view(view).image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);

        let write = vk::WriteDescriptorSet::default()
            .dst_set(self.set)
            .dst_binding(1)
            .dst_array_element(slot)
            .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
            .image_info(std::slice::from_ref(&image_info));

        unsafe { self.device.update_descriptor_sets(std::slice::from_ref(&write), &[]) };

        if slot >= self.next_slot {
            self.next_slot = slot + 1;
        }
    }
}

impl Drop for BindlessSet {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_descriptor_pool(self.pool, None);
            self.device.destroy_descriptor_set_layout(self.layout, None);
            self.device.destroy_sampler(self.sampler, None);
        }
    }
}
