use crate::render::gfx::ShaderStage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingKind {
    CombinedImageSampler,
    UniformBuffer { size: u64 },
}

#[derive(Debug, Clone, Copy)]
pub struct DescriptorBindingDesc {
    pub binding: u32,
    pub kind: BindingKind,
    pub stage: ShaderStage,
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
        self.bindings.push(DescriptorBindingDesc { binding, kind: BindingKind::CombinedImageSampler, stage });
        self
    }

    pub fn with_uniform_buffer<T>(mut self, binding: u32, stage: ShaderStage) -> Self {
        self.bindings.push(DescriptorBindingDesc {
            binding,
            kind: BindingKind::UniformBuffer { size: size_of::<T>() as u64 },
            stage,
        });
        self
    }
}
