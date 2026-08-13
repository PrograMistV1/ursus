use crate::render::gfx::descriptor::{BindingKind, DescriptorBindingDesc, DescriptorSetDesc};
use crate::render::gfx::types::DescriptorSetId;
use ash::vk;

pub(crate) struct StoredDescriptorSet {
    layout: vk::DescriptorSetLayout,
    set: vk::DescriptorSet,
    pool: vk::DescriptorPool,
    bindings: Vec<DescriptorBindingDesc>,
    /// The layout/pool are owned by someone else (e.g. BindlessSet)
    /// and must not be destroyed by this storage's Drop.
    // TODO: This is a workaround.
    owns_resources: bool,
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
        let vk_bindings: Vec<vk::DescriptorSetLayoutBinding> = desc
            .bindings
            .iter()
            .map(|b| {
                vk::DescriptorSetLayoutBinding::default()
                    .binding(b.binding)
                    .descriptor_type(to_vk_type(b.kind))
                    .descriptor_count(1)
                    .stage_flags(b.stage.to_vk())
            })
            .collect();

        let layout = unsafe {
            self.device.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&vk_bindings),
                None,
            )?
        };

        let pool_sizes: Vec<vk::DescriptorPoolSize> = desc
            .bindings
            .iter()
            .map(|b| vk::DescriptorPoolSize { ty: to_vk_type(b.kind), descriptor_count: 1 })
            .collect();

        let (pool, set) =
            crate::vulkan::gfx_pipeline::builder::descriptor::alloc_single_set(&self.device, layout, &pool_sizes)?;

        let id = DescriptorSetId(self.sets.len() as u32);
        self.sets.push(StoredDescriptorSet { layout, set, pool, bindings: desc.bindings, owns_resources: true });
        Ok(id)
    }

    /// Registers an already existing descriptor set (e.g. bindless) so that it can
    /// be addressed using the same DescriptorSetId as regular sets, without
    /// transferring ownership of the resources — the caller remains responsible
    /// for destroying them.
    pub fn register_external(
        &mut self,
        layout: vk::DescriptorSetLayout,
        set: vk::DescriptorSet,
        pool: vk::DescriptorPool,
    ) -> DescriptorSetId {
        let id = DescriptorSetId(self.sets.len() as u32);
        self.sets.push(StoredDescriptorSet { layout, set, pool, bindings: Vec::new(), owns_resources: false });
        id
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
    }
}

impl Drop for DescriptorAllocator {
    fn drop(&mut self) {
        unsafe {
            for ds in &self.sets {
                if ds.owns_resources {
                    self.device.destroy_descriptor_pool(ds.pool, None);
                    self.device.destroy_descriptor_set_layout(ds.layout, None);
                }
            }
        }
    }
}
