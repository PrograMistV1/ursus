use crate::assets::gpu_server::GpuAssetServer;
use crate::render::gfx::types::{DescriptorSetId, SamplerId};
use crate::render::resource::desc::ResourceHandle;
use crate::render::resource::pool::ResourcePool;
use ash::vk;

#[derive(Debug, Clone, Copy)]
pub enum FlushReason {
    InitialAllocation,
    Resize,
    LateBinding,
}

impl std::fmt::Display for FlushReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::InitialAllocation => "initial allocation",
            Self::Resize => "resize",
            Self::LateBinding => "late binding (pass added after allocate)",
        };
        write!(f, "{s}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DescriptorImageType {
    CombinedImageSampler(SamplerId),
    SampledImage,
}

#[derive(Debug, Clone)]
pub struct DescriptorBinding {
    pub resource: ResourceHandle,
    pub set: DescriptorSetId,
    pub binding: u32,
    pub array_element: u32,
    pub image_type: DescriptorImageType,
    pub image_layout: vk::ImageLayout,
}

pub struct DescriptorBindingRegistry {
    bindings: Vec<DescriptorBinding>,
    device: ash::Device,
}

impl DescriptorBindingRegistry {
    pub fn new(device: ash::Device) -> Self {
        Self { bindings: Vec::new(), device }
    }

    pub fn register(&mut self, binding: DescriptorBinding) {
        self.bindings.push(binding);
    }

    pub fn flush(&self, pool: &ResourcePool, affected: &[ResourceHandle], gpu: &GpuAssetServer, reason: FlushReason) {
        let relevant: Vec<&DescriptorBinding> =
            self.bindings.iter().filter(|b| affected.contains(&b.resource)).collect();

        if relevant.is_empty() {
            return;
        }

        let image_infos: Vec<vk::DescriptorImageInfo> = relevant
            .iter()
            .map(|b| {
                let img = pool.image(b.resource);
                let sampler = match b.image_type {
                    DescriptorImageType::CombinedImageSampler(s) => gpu.samplers.handle(s),
                    DescriptorImageType::SampledImage => vk::Sampler::null(),
                };
                vk::DescriptorImageInfo::default().image_view(img.view).image_layout(b.image_layout).sampler(sampler)
            })
            .collect();

        let writes: Vec<vk::WriteDescriptorSet> = relevant
            .iter()
            .zip(image_infos.iter())
            .map(|(b, info)| {
                let desc_type = match b.image_type {
                    DescriptorImageType::CombinedImageSampler(_) => vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    DescriptorImageType::SampledImage => vk::DescriptorType::SAMPLED_IMAGE,
                };
                vk::WriteDescriptorSet::default()
                    .dst_set(gpu.descriptors.handle(b.set))
                    .dst_binding(b.binding)
                    .dst_array_element(b.array_element)
                    .descriptor_type(desc_type)
                    .image_info(std::slice::from_ref(info))
            })
            .collect();

        unsafe { self.device.update_descriptor_sets(&writes, &[]) };
        log::debug!("DescriptorBindingRegistry: rewrote {} descriptors ({reason})", writes.len());
    }

    pub fn flush_all(&self, pool: &ResourcePool, gpu: &GpuAssetServer, reason: FlushReason) {
        let all: Vec<ResourceHandle> = self.bindings.iter().map(|b| b.resource).collect();
        self.flush(pool, &all, gpu, reason);
    }
}
