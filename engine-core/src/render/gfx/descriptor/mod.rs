pub mod desc;
pub mod descriptor_allocator;

pub use desc::{BindingKind, DescriptorBindingDesc, DescriptorSetDesc, ImageUsage};
pub use descriptor_allocator::DescriptorAllocator;
