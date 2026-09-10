use crate::render::gfx::descriptor::ImageUsage;
use crate::render::gfx::types::Format;
use crate::render::resource::desc::{ExternalImageDesc, ResourceDesc, ResourceExtent, ResourceHandle, ResourceKind};
use crate::render::resource::image::{ExternalSlot, ImageRef, ResourceEntry, TransientImage};
use crate::vulkan::core::debug::set_object_name;
use crate::vulkan::core::DeviceContext;
use ash::ext::debug_utils;
use ash::vk;
use std::sync::Arc;

pub struct ResourcePool {
    entries: Vec<ResourceEntry>,
    device: ash::Device,
    physical_device: vk::PhysicalDevice,
    instance: ash::Instance,
    debug_utils: Option<Arc<debug_utils::Device>>,
}

impl ResourcePool {
    pub fn new(
        device: ash::Device,
        physical_device: vk::PhysicalDevice,
        instance: ash::Instance,
        debug_utils: Option<Arc<debug_utils::Device>>,
    ) -> Self {
        Self { entries: Vec::new(), device, physical_device, instance, debug_utils }
    }

    pub fn register(&mut self, desc: ResourceDesc) -> ResourceHandle {
        let handle = ResourceHandle(self.entries.len() as u32);
        self.entries.push(ResourceEntry::Transient { desc, image: Box::new(None) });
        handle
    }

    pub fn register_external(&mut self, desc: ExternalImageDesc) -> ResourceHandle {
        let handle = ResourceHandle(self.entries.len() as u32);
        self.entries.push(ResourceEntry::External(ExternalSlot {
            desc,
            image: vk::Image::null(),
            view: vk::ImageView::null(),
            extent: vk::Extent2D::default(),
        }));
        handle
    }

    pub fn register_swapchain_external(&mut self, format: Format) -> ResourceHandle {
        self.register_external(ExternalImageDesc {
            name: "swapchain".into(),
            format: format.to_vk(),
            kind: ResourceKind::Color,
            initial_layout: vk::ImageLayout::UNDEFINED,
            final_layout: vk::ImageLayout::PRESENT_SRC_KHR,
        })
    }

    pub fn update_external(
        &mut self,
        handle: ResourceHandle,
        image: vk::Image,
        view: vk::ImageView,
        extent: vk::Extent2D,
    ) {
        match &mut self.entries[handle.0 as usize] {
            ResourceEntry::External(slot) => {
                slot.image = image;
                slot.view = view;
                slot.extent = extent;
            }
            ResourceEntry::Transient { .. } => {
                panic!("update_external called for a transient resource {:?}", handle);
            }
        }
    }

    pub fn add_usage(&mut self, handle: ResourceHandle, flags: ImageUsage) {
        if let ResourceEntry::Transient { desc, .. } = &mut self.entries[handle.0 as usize] {
            desc.usage |= flags;
        }
    }

    pub fn allocate(&mut self, internal: (u32, u32), output: (u32, u32)) -> anyhow::Result<()> {
        let device = &self.device;
        let physical_device = self.physical_device;
        let instance = &self.instance;
        let debug_utils = &self.debug_utils;

        for entry in &mut self.entries {
            if let ResourceEntry::Transient { desc, image } = entry {
                if image.is_none() {
                    let (w, h) = desc.extent.resolve(internal, output);
                    let ti = TransientImage::new(DeviceContext { device, physical_device, instance }, desc, w, h)?;
                    debug_name(debug_utils.as_deref(), &ti, desc);
                    **image = Some(ti);
                }
            }
        }
        Ok(())
    }

    pub fn resize_output(&mut self, internal: (u32, u32), new_output: (u32, u32)) -> anyhow::Result<()> {
        for entry in &mut self.entries {
            if let ResourceEntry::Transient { desc, image } = entry {
                if matches!(desc.extent, ResourceExtent::ScaleOutput(_)) {
                    **image = None;
                    let (w, h) = desc.extent.resolve(internal, new_output);
                    **image = Some(TransientImage::new(
                        DeviceContext {
                            device: &self.device,
                            physical_device: self.physical_device,
                            instance: &self.instance,
                        },
                        desc,
                        w,
                        h,
                    )?);
                }
            }
        }
        Ok(())
    }

    pub fn resize_internal(&mut self, new_internal: (u32, u32), output: (u32, u32)) -> anyhow::Result<()> {
        for entry in &mut self.entries {
            if let ResourceEntry::Transient { desc, image } = entry {
                if matches!(desc.extent, ResourceExtent::ScaleInternal(_)) {
                    **image = None;
                    let (w, h) = desc.extent.resolve(new_internal, output);
                    **image = Some(TransientImage::new(
                        DeviceContext {
                            device: &self.device,
                            physical_device: self.physical_device,
                            instance: &self.instance,
                        },
                        desc,
                        w,
                        h,
                    )?);
                }
            }
        }
        Ok(())
    }

    pub fn image(&self, handle: ResourceHandle) -> ImageRef<'_> {
        match &self.entries[handle.0 as usize] {
            ResourceEntry::Transient { desc, image } => {
                let ti = (**image)
                    .as_ref()
                    .unwrap_or_else(|| panic!("ResourcePool: transient resource '{}' is not allocated", desc.name));
                ImageRef {
                    image: ti.image,
                    view: ti.view,
                    format: ti.format,
                    extent: ti.extent,
                    kind: ti.kind,
                    name: &ti.name,
                }
            }
            ResourceEntry::External(slot) => ImageRef {
                image: slot.image,
                view: slot.view,
                format: slot.desc.format,
                extent: slot.extent,
                kind: slot.desc.kind,
                name: &slot.desc.name,
            },
        }
    }

    pub fn desc(&self, handle: ResourceHandle) -> ResourceDescRef<'_> {
        match &self.entries[handle.0 as usize] {
            ResourceEntry::Transient { desc, .. } => ResourceDescRef::Transient(desc),
            ResourceEntry::External(slot) => ResourceDescRef::External(&slot.desc),
        }
    }

    pub fn external_initial_layout(&self, handle: ResourceHandle) -> Option<vk::ImageLayout> {
        match &self.entries[handle.0 as usize] {
            ResourceEntry::External(slot) => Some(slot.desc.initial_layout),
            _ => None,
        }
    }

    pub fn external_final_layout(&self, handle: ResourceHandle) -> Option<vk::ImageLayout> {
        match &self.entries[handle.0 as usize] {
            ResourceEntry::External(slot) => Some(slot.desc.final_layout),
            _ => None,
        }
    }

    pub fn internal_handles(&self) -> impl Iterator<Item = ResourceHandle> + '_ {
        self.entries.iter().enumerate().filter_map(|(i, e)| {
            if let ResourceEntry::Transient { desc, .. } = e {
                if matches!(desc.extent, ResourceExtent::ScaleInternal(_)) {
                    return Some(ResourceHandle(i as u32));
                }
            }
            None
        })
    }

    pub fn output_handles(&self) -> impl Iterator<Item = ResourceHandle> + '_ {
        self.entries.iter().enumerate().filter_map(|(i, e)| {
            if let ResourceEntry::Transient { desc, .. } = e {
                if matches!(desc.extent, ResourceExtent::ScaleOutput(_)) {
                    return Some(ResourceHandle(i as u32));
                }
            }
            None
        })
    }

    pub fn external_handles(&self) -> impl Iterator<Item = ResourceHandle> + '_ {
        self.entries.iter().enumerate().filter_map(|(i, e)| {
            if matches!(e, ResourceEntry::External(_)) {
                Some(ResourceHandle(i as u32))
            } else {
                None
            }
        })
    }

    pub fn handle_by_name(&self, name: &str) -> ResourceHandle {
        self.entries
            .iter()
            .position(|e| match e {
                ResourceEntry::Transient { desc, .. } => desc.name == name,
                ResourceEntry::External(slot) => slot.desc.name == name,
            })
            .map(|i| ResourceHandle(i as u32))
            .expect("resource not found")
    }
}

pub enum ResourceDescRef<'a> {
    Transient(&'a ResourceDesc),
    External(&'a ExternalImageDesc),
}

fn debug_name(debug_utils: Option<&debug_utils::Device>, ti: &TransientImage, desc: &ResourceDesc) {
    if let Some(du) = debug_utils {
        set_object_name(du, ti.image, &desc.name);
        set_object_name(du, ti.view, &format!("{}_view", desc.name));
        set_object_name(du, ti.memory, &format!("{}_memory", desc.name));
    }
}
