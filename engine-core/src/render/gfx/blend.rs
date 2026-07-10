use ash::vk;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendFactor {
    Zero,
    One,
    SrcAlpha,
    OneMinusSrcAlpha,
}

impl BlendFactor {
    pub(crate) fn to_vk(self) -> vk::BlendFactor {
        match self {
            Self::Zero => vk::BlendFactor::ZERO,
            Self::One => vk::BlendFactor::ONE,
            Self::SrcAlpha => vk::BlendFactor::SRC_ALPHA,
            Self::OneMinusSrcAlpha => vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BlendState {
    pub enabled: bool,
    pub src_color: BlendFactor,
    pub dst_color: BlendFactor,
    pub src_alpha: BlendFactor,
    pub dst_alpha: BlendFactor,
}

impl BlendState {
    pub fn alpha_blend() -> Self {
        Self {
            enabled: true,
            src_color: BlendFactor::SrcAlpha,
            dst_color: BlendFactor::OneMinusSrcAlpha,
            src_alpha: BlendFactor::One,
            dst_alpha: BlendFactor::OneMinusSrcAlpha,
        }
    }

    pub(crate) fn to_vk(self) -> vk::PipelineColorBlendAttachmentState {
        vk::PipelineColorBlendAttachmentState::default()
            .blend_enable(self.enabled)
            .src_color_blend_factor(self.src_color.to_vk())
            .dst_color_blend_factor(self.dst_color.to_vk())
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(self.src_alpha.to_vk())
            .dst_alpha_blend_factor(self.dst_alpha.to_vk())
            .alpha_blend_op(vk::BlendOp::ADD)
            .color_write_mask(vk::ColorComponentFlags::RGBA)
    }
}
