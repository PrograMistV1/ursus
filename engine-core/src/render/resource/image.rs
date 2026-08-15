use crate::render::resource::desc::{ExternalImageDesc, ResourceDesc, ResourceKind};
use crate::vulkan::core::memory::destroy_image_resources;
use crate::vulkan::core::{memory, DeviceContext};
use ash::vk;
use memory::ImageDesc;

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
    pub(crate) fn new(ctx: DeviceContext, desc: &ResourceDesc, width: u32, height: u32) -> anyhow::Result<Self> {
        let img_desc = ImageDesc {
            format: desc.format.to_vk(),
            width,
            height,
            usage: desc.usage.to_vk(),
            aspect_mask: desc.kind.aspect_mask(),
            mip_levels: 1,
        };
        let img = memory::alloc_image(ctx, &img_desc)?;

        log::debug!("TransientImage '{}': {}x{} {:?}", desc.name, width, height, desc.format);

        Ok(Self {
            image: img.image,
            view: img.view,
            memory: img.memory,
            format: desc.format.to_vk(),
            extent: vk::Extent2D { width, height },
            kind: desc.kind,
            name: desc.name.clone(),
            device: ctx.device.clone(),
        })
    }
}

impl Drop for TransientImage {
    fn drop(&mut self) {
        unsafe { destroy_image_resources(&self.device, self.image, self.view, self.memory) }
    }
}

pub(crate) struct ExternalSlot {
    pub desc: ExternalImageDesc,
    pub image: vk::Image,
    pub view: vk::ImageView,
    pub extent: vk::Extent2D,
}

pub(crate) enum ResourceEntry {
    Transient {
        desc: ResourceDesc,
        image: Box<Option<TransientImage>>,
    },
    External(ExternalSlot),
}

pub struct ImageRef<'a> {
    pub image: vk::Image,
    pub view: vk::ImageView,
    pub format: vk::Format,
    pub extent: vk::Extent2D,
    pub kind: ResourceKind,
    pub name: &'a str,
}

pub trait GpuImage {
    fn image(&self) -> vk::Image;
    fn view(&self) -> vk::ImageView;
    fn extent(&self) -> vk::Extent2D;
    fn format(&self) -> vk::Format;
    fn kind(&self) -> ResourceKind;
}

impl GpuImage for TransientImage {
    fn image(&self) -> vk::Image {
        self.image
    }
    fn view(&self) -> vk::ImageView {
        self.view
    }
    fn extent(&self) -> vk::Extent2D {
        self.extent
    }
    fn format(&self) -> vk::Format {
        self.format
    }
    fn kind(&self) -> ResourceKind {
        self.kind
    }
}

impl<'a> GpuImage for ImageRef<'a> {
    fn image(&self) -> vk::Image {
        self.image
    }
    fn view(&self) -> vk::ImageView {
        self.view
    }
    fn extent(&self) -> vk::Extent2D {
        self.extent
    }
    fn format(&self) -> vk::Format {
        self.format
    }
    fn kind(&self) -> ResourceKind {
        self.kind
    }
}
