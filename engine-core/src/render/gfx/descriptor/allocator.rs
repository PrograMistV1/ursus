use crate::render::gfx::descriptor::{BindingKind, DescriptorBindingDesc, DescriptorSetDesc};
use crate::render::gfx::types::DescriptorSetId;
use ash::vk;

pub(crate) struct StoredDescriptorSet {
    layout: vk::DescriptorSetLayout,
    set: vk::DescriptorSet,
    pool: vk::DescriptorPool,
    bindings: Vec<DescriptorBindingDesc>,
}

/// A single entry point for creating and populating regular (non-bindless) descriptor sets.
pub struct DescriptorAllocator {
    sets: Vec<StoredDescriptorSet>,
    device: ash::Device,
}

impl DescriptorAllocator {
    pub fn new(device: ash::Device) -> Self {
        Self { sets: Vec::new(), device }
    }

    pub fn create_set(&mut self, desc: DescriptorSetDesc) -> anyhow::Result<DescriptorSetId> {
        let has_bindless = desc.bindings.iter().any(|b| b.bindless);

        let vk_bindings: Vec<vk::DescriptorSetLayoutBinding> = desc
            .bindings
            .iter()
            .map(|b| {
                let mut vb = vk::DescriptorSetLayoutBinding::default()
                    .binding(b.binding)
                    .descriptor_type(to_vk_type(b.kind))
                    .descriptor_count(b.count)
                    .stage_flags(b.stage.to_vk());
                if let Some(sampler) = &b.immutable_sampler {
                    vb = vb.immutable_samplers(std::slice::from_ref(sampler));
                }
                vb
            })
            .collect();

        let binding_flags: Vec<vk::DescriptorBindingFlags> = desc
            .bindings
            .iter()
            .map(|b| {
                if b.bindless {
                    vk::DescriptorBindingFlags::PARTIALLY_BOUND
                        | vk::DescriptorBindingFlags::VARIABLE_DESCRIPTOR_COUNT
                        | vk::DescriptorBindingFlags::UPDATE_AFTER_BIND
                } else {
                    vk::DescriptorBindingFlags::empty()
                }
            })
            .collect();

        let mut flags_info = vk::DescriptorSetLayoutBindingFlagsCreateInfo::default().binding_flags(&binding_flags);

        let mut layout_info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&vk_bindings);
        if has_bindless {
            layout_info = layout_info
                .flags(vk::DescriptorSetLayoutCreateFlags::UPDATE_AFTER_BIND_POOL)
                .push_next(&mut flags_info);
        }

        let layout = unsafe { self.device.create_descriptor_set_layout(&layout_info, None)? };

        let pool_sizes: Vec<vk::DescriptorPoolSize> = desc
            .bindings
            .iter()
            .map(|b| vk::DescriptorPoolSize { ty: to_vk_type(b.kind), descriptor_count: b.count })
            .collect();

        let mut pool_info = vk::DescriptorPoolCreateInfo::default().pool_sizes(&pool_sizes).max_sets(1);
        if has_bindless {
            pool_info = pool_info.flags(vk::DescriptorPoolCreateFlags::UPDATE_AFTER_BIND);
        }
        let pool = unsafe { self.device.create_descriptor_pool(&pool_info, None)? };

        let variable_count: Option<u32> = desc.bindings.iter().find(|b| b.bindless).map(|b| b.count);
        let variable_count_value: u32 = variable_count.unwrap_or(0);
        let mut var_count_info = variable_count.map(|_| {
            vk::DescriptorSetVariableDescriptorCountAllocateInfo::default()
                .descriptor_counts(std::slice::from_ref(&variable_count_value))
        });

        let mut alloc_info =
            vk::DescriptorSetAllocateInfo::default().descriptor_pool(pool).set_layouts(std::slice::from_ref(&layout));
        if let Some(vci) = var_count_info.as_mut() {
            alloc_info = alloc_info.push_next(vci);
        }

        let set = unsafe { self.device.allocate_descriptor_sets(&alloc_info)?[0] };

        let id = DescriptorSetId(self.sets.len() as u32);
        self.sets.push(StoredDescriptorSet { layout, set, pool, bindings: desc.bindings });
        Ok(id)
    }

    pub fn write_sampled_image_array(
        &self,
        set: DescriptorSetId,
        binding: u32,
        array_element: u32,
        view: vk::ImageView,
    ) {
        let stored = &self.sets[set.0 as usize];
        let image_info =
            vk::DescriptorImageInfo::default().image_view(view).image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL);
        let write = vk::WriteDescriptorSet::default()
            .dst_set(stored.set)
            .dst_binding(binding)
            .dst_array_element(array_element)
            .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
            .image_info(std::slice::from_ref(&image_info));
        unsafe { self.device.update_descriptor_sets(std::slice::from_ref(&write), &[]) };
    }

    pub(crate) fn layout(&self, id: DescriptorSetId) -> vk::DescriptorSetLayout {
        self.sets[id.0 as usize].layout
    }

    pub fn handle(&self, id: DescriptorSetId) -> vk::DescriptorSet {
        self.sets[id.0 as usize].set
    }

    pub fn bind_uniform_buffer(
        &self,
        set: DescriptorSetId,
        binding: u32,
        buffer: vk::Buffer,
        size: vk::DeviceSize,
    ) -> anyhow::Result<()> {
        self.check_kind(set, binding, "UniformBuffer", |k| matches!(k, BindingKind::UniformBuffer { .. }))?;

        let stored = &self.sets[set.0 as usize];
        let buf_info = vk::DescriptorBufferInfo::default().buffer(buffer).offset(0).range(size);
        let write = vk::WriteDescriptorSet::default()
            .dst_set(stored.set)
            .dst_binding(binding)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .buffer_info(std::slice::from_ref(&buf_info));

        unsafe { self.device.update_descriptor_sets(std::slice::from_ref(&write), &[]) };
        Ok(())
    }

    pub fn bind_mapped_uniform_buffer<T: Copy>(
        &self,
        set: DescriptorSetId,
        binding: u32,
        mapped: &crate::vulkan::MappedGpuBuffer<T>,
    ) -> anyhow::Result<()> {
        self.bind_uniform_buffer(set, binding, mapped.buffer, mapped.size())
    }

    pub fn bind_storage_buffer(
        &self,
        set: DescriptorSetId,
        binding: u32,
        buffer: vk::Buffer,
        size: vk::DeviceSize,
    ) -> anyhow::Result<()> {
        self.check_kind(set, binding, "StorageBuffer", |k| matches!(k, BindingKind::StorageBuffer { .. }))?;

        let stored = &self.sets[set.0 as usize];
        let buf_info = vk::DescriptorBufferInfo::default().buffer(buffer).offset(0).range(size);
        let write = vk::WriteDescriptorSet::default()
            .dst_set(stored.set)
            .dst_binding(binding)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(std::slice::from_ref(&buf_info));

        unsafe { self.device.update_descriptor_sets(std::slice::from_ref(&write), &[]) };
        Ok(())
    }

    pub fn bind_mapped_storage_buffer<T: Copy>(
        &self,
        set: DescriptorSetId,
        binding: u32,
        mapped: &crate::vulkan::MappedGpuBuffer<T>,
    ) -> anyhow::Result<()> {
        self.bind_storage_buffer(set, binding, mapped.buffer, mapped.size())
    }

    pub fn bind_sampled_image(
        &self,
        set: DescriptorSetId,
        binding: u32,
        view: vk::ImageView,
        layout: vk::ImageLayout,
        sampler: vk::Sampler,
    ) -> anyhow::Result<()> {
        self.check_kind(set, binding, "CombinedImageSampler", |k| matches!(k, BindingKind::CombinedImageSampler))?;

        let stored = &self.sets[set.0 as usize];
        let image_info = vk::DescriptorImageInfo::default().image_view(view).image_layout(layout).sampler(sampler);
        let write = vk::WriteDescriptorSet::default()
            .dst_set(stored.set)
            .dst_binding(binding)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(std::slice::from_ref(&image_info));

        unsafe { self.device.update_descriptor_sets(std::slice::from_ref(&write), &[]) };
        Ok(())
    }

    fn check_kind(
        &self,
        set: DescriptorSetId,
        binding: u32,
        expected_name: &str,
        matches: impl Fn(BindingKind) -> bool,
    ) -> anyhow::Result<()> {
        let stored = self
            .sets
            .get(set.0 as usize)
            .ok_or_else(|| anyhow::anyhow!("DescriptorAllocator: DescriptorSetId {:?} not found", set))?;

        match stored.bindings.iter().find(|b| b.binding == binding) {
            Some(b) if matches(b.kind) => Ok(()),
            Some(b) => anyhow::bail!(
                "DescriptorAllocator: binding {} in set {:?} is declared as {:?}, expected {}",
                binding,
                set,
                b.kind,
                expected_name
            ),
            None => anyhow::bail!("DescriptorAllocator: binding {} is not declared in set {:?}", binding, set),
        }
    }
}

fn to_vk_type(kind: BindingKind) -> vk::DescriptorType {
    match kind {
        BindingKind::CombinedImageSampler => vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
        BindingKind::UniformBuffer { .. } => vk::DescriptorType::UNIFORM_BUFFER,
        BindingKind::StorageBuffer { .. } => vk::DescriptorType::STORAGE_BUFFER,
        BindingKind::Sampler => vk::DescriptorType::SAMPLER,
        BindingKind::SampledImageArray => vk::DescriptorType::SAMPLED_IMAGE,
    }
}

impl Drop for DescriptorAllocator {
    fn drop(&mut self) {
        unsafe {
            for ds in &self.sets {
                self.device.destroy_descriptor_pool(ds.pool, None);
                self.device.destroy_descriptor_set_layout(ds.layout, None);
            }
        }
    }
}
