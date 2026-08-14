pub mod barrier;
pub mod desc;
mod descriptor_binding;
pub mod image;
mod layout_tracker;
pub mod pool;

pub use barrier::make_barrier;
pub use desc::{ExternalImageDesc, ResourceDesc, ResourceExtent, ResourceHandle, ResourceKind};
pub use descriptor_binding::{DescriptorBinding, DescriptorBindingRegistry, DescriptorImageType, FlushReason};
pub use image::{GpuImage, ImageRef, TransientImage};
pub use layout_tracker::LayoutTracker;
pub use pool::{ResourceDescRef, ResourcePool};
