pub mod allocator;
pub mod desc;

pub use allocator::DescriptorAllocator;
pub use desc::{BindingKind, DescriptorBindingDesc, DescriptorSetDesc, ImageUsage};
