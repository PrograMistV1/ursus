use crate::render::gfx::types::ShaderStage;
use ash::vk;
use std::ops::{BitOr, BitOrAssign};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingKind {
    CombinedImageSampler,
    UniformBuffer { size: u64 },
    StorageBuffer { size: u64 },
    Sampler,
    SampledImageArray,
}

#[derive(Debug, Clone, Copy)]
pub struct DescriptorBindingDesc {
    pub binding: u32,
    pub kind: BindingKind,
    pub stage: ShaderStage,
    pub count: u32,
    pub bindless: bool,
    pub immutable_sampler: Option<vk::Sampler>,
}

#[derive(Debug, Clone, Default)]
pub struct DescriptorSetDesc {
    pub bindings: Vec<DescriptorBindingDesc>,
}

impl DescriptorSetDesc {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_sampled_image(mut self, binding: u32, stage: ShaderStage) -> Self {
        self.bindings.push(DescriptorBindingDesc {
            binding,
            kind: BindingKind::CombinedImageSampler,
            stage,
            count: 1,
            bindless: false,
            immutable_sampler: None,
        });
        self
    }

    pub fn with_uniform_buffer<T>(mut self, binding: u32, stage: ShaderStage) -> Self {
        self.bindings.push(DescriptorBindingDesc {
            binding,
            kind: BindingKind::UniformBuffer { size: size_of::<T>() as u64 },
            stage,
            count: 1,
            bindless: false,
            immutable_sampler: None,
        });
        self
    }

    pub fn with_storage_buffer<T>(mut self, binding: u32, stage: ShaderStage) -> Self {
        self.bindings.push(DescriptorBindingDesc {
            binding,
            kind: BindingKind::StorageBuffer { size: size_of::<T>() as u64 },
            stage,
            count: 1,
            bindless: false,
            immutable_sampler: None,
        });
        self
    }

    pub fn with_immutable_sampler(mut self, binding: u32, stage: ShaderStage, sampler: vk::Sampler) -> Self {
        self.bindings.push(DescriptorBindingDesc {
            binding,
            kind: BindingKind::Sampler,
            stage,
            count: 1,
            bindless: false,
            immutable_sampler: Some(sampler),
        });
        self
    }

    pub fn with_bindless_sampled_images(mut self, binding: u32, stage: ShaderStage, max_count: u32) -> Self {
        self.bindings.push(DescriptorBindingDesc {
            binding,
            kind: BindingKind::SampledImageArray,
            stage,
            count: max_count,
            bindless: true,
            immutable_sampler: None,
        });
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageUsage(u32);

impl ImageUsage {
    pub const COLOR_ATTACHMENT: Self = Self(1 << 0);
    pub const DEPTH_ATTACHMENT: Self = Self(1 << 1);
    pub const SAMPLED: Self = Self(1 << 2);
    pub const TRANSFER_SRC: Self = Self(1 << 3);
    pub const TRANSFER_DST: Self = Self(1 << 4);

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    pub(crate) fn to_vk(self) -> vk::ImageUsageFlags {
        let mut out = vk::ImageUsageFlags::empty();
        if self.contains(Self::COLOR_ATTACHMENT) {
            out |= vk::ImageUsageFlags::COLOR_ATTACHMENT;
        }
        if self.contains(Self::DEPTH_ATTACHMENT) {
            out |= vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT;
        }
        if self.contains(Self::SAMPLED) {
            out |= vk::ImageUsageFlags::SAMPLED;
        }
        if self.contains(Self::TRANSFER_SRC) {
            out |= vk::ImageUsageFlags::TRANSFER_SRC;
        }
        if self.contains(Self::TRANSFER_DST) {
            out |= vk::ImageUsageFlags::TRANSFER_DST;
        }
        out
    }
}

impl BitOr for ImageUsage {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for ImageUsage {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}
