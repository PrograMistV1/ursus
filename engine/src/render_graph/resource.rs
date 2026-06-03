use crate::vulkan::core::memory::find_memory_type;
use ash::vk;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResourceHandle(pub(crate) u32);

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ResourceExtent {
    Absolute(u32, u32),
    ScaleInternal(f32),
    ScaleOutput(f32),
}

impl ResourceExtent {
    pub fn resolve(&self, internal: (u32, u32), output: (u32, u32)) -> (u32, u32) {
        match *self {
            Self::Absolute(w, h) => (w, h),
            Self::ScaleInternal(s) => (
                ((internal.0 as f32 * s).round() as u32).max(1),
                ((internal.1 as f32 * s).round() as u32).max(1),
            ),
            Self::ScaleOutput(s) => (
                ((output.0 as f32 * s).round() as u32).max(1),
                ((output.1 as f32 * s).round() as u32).max(1),
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    Color,
    Depth,
}

impl ResourceKind {
    pub fn aspect_mask(self) -> vk::ImageAspectFlags {
        match self {
            Self::Color => vk::ImageAspectFlags::COLOR,
            Self::Depth => vk::ImageAspectFlags::DEPTH,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResourceDesc {
    pub name: String,
    pub format: vk::Format,
    pub extent: ResourceExtent,
    pub kind: ResourceKind,
    pub usage: vk::ImageUsageFlags,
}

impl ResourceDesc {
    pub fn color(name: impl Into<String>, format: vk::Format, extent: ResourceExtent) -> Self {
        Self {
            name: name.into(),
            format,
            extent,
            kind: ResourceKind::Color,
            usage: vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
        }
    }

    pub fn depth(name: impl Into<String>, format: vk::Format, extent: ResourceExtent) -> Self {
        Self {
            name: name.into(),
            format,
            extent,
            kind: ResourceKind::Depth,
            usage: vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
        }
    }

    pub fn with_usage(mut self, flags: vk::ImageUsageFlags) -> Self {
        self.usage |= flags;
        self
    }
}

pub struct TransientImage {
    pub image: vk::Image,
    pub view: vk::ImageView,
    pub memory: vk::DeviceMemory,
    pub format: vk::Format,
    pub extent: vk::Extent2D,
    pub kind: ResourceKind,
    pub name: String,
    device: ash::Device,
}

impl TransientImage {
    fn new(
        device: &ash::Device,
        physical_device: vk::PhysicalDevice,
        instance: &ash::Instance,
        desc: &ResourceDesc,
        width: u32,
        height: u32,
    ) -> anyhow::Result<Self> {
        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(desc.format)
            .extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(desc.usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);

        let image = unsafe { device.create_image(&image_info, None)? };
        let req = unsafe { device.get_image_memory_requirements(image) };

        let mem_type = find_memory_type(
            instance,
            physical_device,
            req.memory_type_bits,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )?;

        let memory = unsafe {
            device.allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(req.size)
                    .memory_type_index(mem_type),
                None,
            )?
        };
        unsafe { device.bind_image_memory(image, memory, 0)? };

        let view = unsafe {
            device.create_image_view(
                &vk::ImageViewCreateInfo::default()
                    .image(image)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(desc.format)
                    .subresource_range(vk::ImageSubresourceRange {
                        aspect_mask: desc.kind.aspect_mask(),
                        base_mip_level: 0,
                        level_count: 1,
                        base_array_layer: 0,
                        layer_count: 1,
                    }),
                None,
            )?
        };

        log::debug!(
            "TransientImage '{}': {}x{} {:?}",
            desc.name,
            width,
            height,
            desc.format
        );

        Ok(Self {
            image,
            view,
            memory,
            format: desc.format,
            extent: vk::Extent2D { width, height },
            kind: desc.kind,
            name: desc.name.clone(),
            device: device.clone(),
        })
    }
}

impl Drop for TransientImage {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_image_view(self.view, None);
            self.device.destroy_image(self.image, None);
            self.device.free_memory(self.memory, None);
        }
        log::debug!("TransientImage '{}' уничтожен", self.name);
    }
}

pub struct ResourcePool {
    descs: Vec<ResourceDesc>,
    images: Vec<Option<TransientImage>>,

    device: ash::Device,
    physical_device: vk::PhysicalDevice,
    instance: ash::Instance,
}

impl ResourcePool {
    pub fn new(
        device: ash::Device,
        physical_device: vk::PhysicalDevice,
        instance: ash::Instance,
    ) -> Self {
        Self {
            descs: Vec::new(),
            images: Vec::new(),
            device,
            physical_device,
            instance,
        }
    }

    pub fn register(&mut self, desc: ResourceDesc) -> ResourceHandle {
        let handle = ResourceHandle(self.descs.len() as u32);
        self.descs.push(desc);
        self.images.push(None);
        handle
    }

    pub fn add_usage(&mut self, handle: ResourceHandle, flags: vk::ImageUsageFlags) {
        self.descs[handle.0 as usize].usage |= flags;
    }

    pub fn allocate(&mut self, internal: (u32, u32), output: (u32, u32)) -> anyhow::Result<()> {
        for (i, desc) in self.descs.iter().enumerate() {
            if self.images[i].is_none() {
                let (w, h) = desc.extent.resolve(internal, output);
                self.images[i] = Some(TransientImage::new(
                    &self.device,
                    self.physical_device,
                    &self.instance,
                    desc,
                    w,
                    h,
                )?);
            }
        }
        Ok(())
    }

    pub fn resize_output(
        &mut self,
        internal: (u32, u32),
        new_output: (u32, u32),
    ) -> anyhow::Result<()> {
        for (i, desc) in self.descs.iter().enumerate() {
            if matches!(desc.extent, ResourceExtent::ScaleOutput(_)) {
                self.images[i] = None;
                let (w, h) = desc.extent.resolve(internal, new_output);
                self.images[i] = Some(TransientImage::new(
                    &self.device,
                    self.physical_device,
                    &self.instance,
                    desc,
                    w,
                    h,
                )?);
            }
        }
        Ok(())
    }

    pub fn resize_internal(
        &mut self,
        new_internal: (u32, u32),
        output: (u32, u32),
    ) -> anyhow::Result<()> {
        for (i, desc) in self.descs.iter().enumerate() {
            if matches!(desc.extent, ResourceExtent::ScaleInternal(_)) {
                self.images[i] = None;
                let (w, h) = desc.extent.resolve(new_internal, output);
                self.images[i] = Some(TransientImage::new(
                    &self.device,
                    self.physical_device,
                    &self.instance,
                    desc,
                    w,
                    h,
                )?);
            }
        }
        Ok(())
    }

    pub fn image(&self, handle: ResourceHandle) -> &TransientImage {
        self.images[handle.0 as usize]
            .as_ref()
            .unwrap_or_else(|| panic!("ResourcePool: ресурс {:?} не выделен", handle))
    }

    pub fn desc(&self, handle: ResourceHandle) -> &ResourceDesc {
        &self.descs[handle.0 as usize]
    }

    pub fn internal_handles(&self) -> impl Iterator<Item = ResourceHandle> + '_ {
        self.descs
            .iter()
            .enumerate()
            .filter(|(_, d)| matches!(d.extent, ResourceExtent::ScaleInternal(_)))
            .map(|(i, _)| ResourceHandle(i as u32))
    }

    pub fn output_handles(&self) -> impl Iterator<Item = ResourceHandle> + '_ {
        self.descs
            .iter()
            .enumerate()
            .filter(|(_, d)| matches!(d.extent, ResourceExtent::ScaleOutput(_)))
            .map(|(i, _)| ResourceHandle(i as u32))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DescriptorImageType {
    CombinedImageSampler(vk::Sampler),
    SampledImage,
}

#[derive(Debug, Clone)]
pub struct DescriptorBinding {
    pub resource: ResourceHandle,
    pub set: vk::DescriptorSet,
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
        Self {
            bindings: Vec::new(),
            device,
        }
    }

    pub fn register(&mut self, binding: DescriptorBinding) {
        self.bindings.push(binding);
    }

    pub fn flush(&self, pool: &ResourcePool, affected: &[ResourceHandle]) {
        let affected_set: std::collections::HashSet<ResourceHandle> =
            affected.iter().copied().collect();

        let relevant: Vec<&DescriptorBinding> = self
            .bindings
            .iter()
            .filter(|b| affected_set.contains(&b.resource))
            .collect();

        if relevant.is_empty() {
            return;
        }

        let image_infos: Vec<vk::DescriptorImageInfo> = relevant
            .iter()
            .map(|b| {
                let view = pool.image(b.resource).view;
                let sampler = match b.image_type {
                    DescriptorImageType::CombinedImageSampler(s) => s,
                    DescriptorImageType::SampledImage => vk::Sampler::null(),
                };
                vk::DescriptorImageInfo::default()
                    .image_view(view)
                    .image_layout(b.image_layout)
                    .sampler(sampler)
            })
            .collect();

        let writes: Vec<vk::WriteDescriptorSet> = relevant
            .iter()
            .zip(image_infos.iter())
            .map(|(b, info)| {
                let desc_type = match b.image_type {
                    DescriptorImageType::CombinedImageSampler(_) => {
                        vk::DescriptorType::COMBINED_IMAGE_SAMPLER
                    }
                    DescriptorImageType::SampledImage => vk::DescriptorType::SAMPLED_IMAGE,
                };
                vk::WriteDescriptorSet::default()
                    .dst_set(b.set)
                    .dst_binding(b.binding)
                    .dst_array_element(b.array_element)
                    .descriptor_type(desc_type)
                    .image_info(std::slice::from_ref(info))
            })
            .collect();

        unsafe { self.device.update_descriptor_sets(&writes, &[]) };
        log::debug!(
            "DescriptorBindingRegistry: переписано {} дескрипторов после resize",
            writes.len()
        );
    }

    pub fn flush_all(&self, pool: &ResourcePool) {
        let all: Vec<ResourceHandle> = self.bindings.iter().map(|b| b.resource).collect();
        self.flush(pool, &all);
    }
}

pub struct LayoutTracker {
    layouts: HashMap<ResourceHandle, vk::ImageLayout>,
}

impl LayoutTracker {
    pub fn new() -> Self {
        Self {
            layouts: HashMap::new(),
        }
    }

    pub fn reset(&mut self) {
        for v in self.layouts.values_mut() {
            *v = vk::ImageLayout::UNDEFINED;
        }
    }

    pub fn current(&self, handle: ResourceHandle) -> vk::ImageLayout {
        self.layouts
            .get(&handle)
            .copied()
            .unwrap_or(vk::ImageLayout::UNDEFINED)
    }

    pub fn set(&mut self, handle: ResourceHandle, layout: vk::ImageLayout) {
        self.layouts.insert(handle, layout);
    }

    pub fn transition(
        &mut self,
        device: &ash::Device,
        cmd: vk::CommandBuffer,
        pool: &ResourcePool,
        transitions: &[(ResourceHandle, vk::ImageLayout)],
    ) -> bool {
        let barriers: Vec<vk::ImageMemoryBarrier2> = transitions
            .iter()
            .filter_map(|&(handle, new_layout)| {
                let old_layout = self.current(handle);
                if old_layout == new_layout {
                    return None;
                }
                let img = pool.image(handle);
                Some(make_barrier(img, old_layout, new_layout))
            })
            .collect();

        if barriers.is_empty() {
            return false;
        }

        unsafe {
            device.cmd_pipeline_barrier2(
                cmd,
                &vk::DependencyInfo::default().image_memory_barriers(&barriers),
            );
        }

        for &(handle, new_layout) in transitions {
            self.set(handle, new_layout);
        }

        true
    }
}

impl Default for LayoutTracker {
    fn default() -> Self {
        Self::new()
    }
}

fn make_barrier(
    img: &TransientImage,
    old_layout: vk::ImageLayout,
    new_layout: vk::ImageLayout,
) -> vk::ImageMemoryBarrier2<'_> {
    let (src_stage, src_access, dst_stage, dst_access) =
        layout_transition_masks(old_layout, new_layout, img.kind);

    vk::ImageMemoryBarrier2::default()
        .src_stage_mask(src_stage)
        .src_access_mask(src_access)
        .dst_stage_mask(dst_stage)
        .dst_access_mask(dst_access)
        .old_layout(old_layout)
        .new_layout(new_layout)
        .image(img.image)
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask: img.kind.aspect_mask(),
            base_mip_level: 0,
            level_count: 1,
            base_array_layer: 0,
            layer_count: 1,
        })
}

fn layout_transition_masks(
    from: vk::ImageLayout,
    to: vk::ImageLayout,
    kind: ResourceKind,
) -> (
    vk::PipelineStageFlags2,
    vk::AccessFlags2,
    vk::PipelineStageFlags2,
    vk::AccessFlags2,
) {
    use vk::AccessFlags2 as A;
    use vk::ImageLayout as L;
    use vk::PipelineStageFlags2 as S;

    match (from, to) {
        (L::UNDEFINED, L::COLOR_ATTACHMENT_OPTIMAL) => (
            S::TOP_OF_PIPE,
            A::empty(),
            S::COLOR_ATTACHMENT_OUTPUT,
            A::COLOR_ATTACHMENT_WRITE,
        ),
        (L::UNDEFINED, L::DEPTH_ATTACHMENT_OPTIMAL) => (
            S::TOP_OF_PIPE,
            A::empty(),
            S::EARLY_FRAGMENT_TESTS,
            A::DEPTH_STENCIL_ATTACHMENT_READ | A::DEPTH_STENCIL_ATTACHMENT_WRITE,
        ),
        (L::UNDEFINED, L::SHADER_READ_ONLY_OPTIMAL) => (
            S::TOP_OF_PIPE,
            A::empty(),
            S::FRAGMENT_SHADER,
            A::SHADER_READ,
        ),

        (L::COLOR_ATTACHMENT_OPTIMAL, L::SHADER_READ_ONLY_OPTIMAL) => (
            S::COLOR_ATTACHMENT_OUTPUT,
            A::COLOR_ATTACHMENT_WRITE,
            S::FRAGMENT_SHADER,
            A::SHADER_READ,
        ),
        (L::DEPTH_ATTACHMENT_OPTIMAL, L::SHADER_READ_ONLY_OPTIMAL) => (
            S::LATE_FRAGMENT_TESTS,
            A::DEPTH_STENCIL_ATTACHMENT_WRITE,
            S::FRAGMENT_SHADER,
            A::SHADER_READ,
        ),

        (L::SHADER_READ_ONLY_OPTIMAL, L::COLOR_ATTACHMENT_OPTIMAL) => (
            S::FRAGMENT_SHADER,
            A::SHADER_READ,
            S::COLOR_ATTACHMENT_OUTPUT,
            A::COLOR_ATTACHMENT_WRITE,
        ),
        (L::SHADER_READ_ONLY_OPTIMAL, L::DEPTH_ATTACHMENT_OPTIMAL) => (
            S::FRAGMENT_SHADER,
            A::SHADER_READ,
            S::EARLY_FRAGMENT_TESTS,
            A::DEPTH_STENCIL_ATTACHMENT_READ | A::DEPTH_STENCIL_ATTACHMENT_WRITE,
        ),

        (L::DEPTH_ATTACHMENT_OPTIMAL, L::DEPTH_ATTACHMENT_OPTIMAL) => (
            S::LATE_FRAGMENT_TESTS,
            A::DEPTH_STENCIL_ATTACHMENT_WRITE,
            S::EARLY_FRAGMENT_TESTS,
            A::DEPTH_STENCIL_ATTACHMENT_READ | A::DEPTH_STENCIL_ATTACHMENT_WRITE,
        ),

        (L::UNDEFINED, L::PRESENT_SRC_KHR) => {
            (S::TOP_OF_PIPE, A::empty(), S::BOTTOM_OF_PIPE, A::empty())
        }
        (L::COLOR_ATTACHMENT_OPTIMAL, L::PRESENT_SRC_KHR) => (
            S::COLOR_ATTACHMENT_OUTPUT,
            A::COLOR_ATTACHMENT_WRITE,
            S::BOTTOM_OF_PIPE,
            A::empty(),
        ),
        (L::PRESENT_SRC_KHR, L::COLOR_ATTACHMENT_OPTIMAL) => (
            S::BOTTOM_OF_PIPE,
            A::empty(),
            S::COLOR_ATTACHMENT_OUTPUT,
            A::COLOR_ATTACHMENT_WRITE,
        ),

        other => panic!(
            "layout_transition_masks: неизвестная пара {:?} (kind={:?})",
            other, kind
        ),
    }
}
