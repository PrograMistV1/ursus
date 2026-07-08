use ash::vk;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    Rgba8Unorm,
    Rgba8Srgb,
    Bgra8Srgb,
    Rgba16Float,
    R8Unorm,
    Depth32Float,
    R32Float,
    Rg32Float,
    Rgb32Float,
    Rgba32Float,
}

impl Format {
    pub(crate) fn to_vk(self) -> vk::Format {
        match self {
            Self::Rgba8Unorm => vk::Format::R8G8B8A8_UNORM,
            Self::Rgba8Srgb => vk::Format::R8G8B8A8_SRGB,
            Self::Bgra8Srgb => vk::Format::B8G8R8A8_SRGB,
            Self::Rgba16Float => vk::Format::R16G16B16A16_SFLOAT,
            Self::R8Unorm => vk::Format::R8_UNORM,
            Self::Depth32Float => vk::Format::D32_SFLOAT,
            Self::R32Float => vk::Format::R32_SFLOAT,
            Self::Rg32Float => vk::Format::R32G32_SFLOAT,
            Self::Rgb32Float => vk::Format::R32G32B32_SFLOAT,
            Self::Rgba32Float => vk::Format::R32G32B32A32_SFLOAT,
        }
    }

    pub(crate) fn from_vk(f: vk::Format) -> Self {
        match f {
            vk::Format::R8G8B8A8_UNORM => Self::Rgba8Unorm,
            vk::Format::R8G8B8A8_SRGB => Self::Rgba8Srgb,
            vk::Format::B8G8R8A8_SRGB => Self::Bgra8Srgb,
            vk::Format::R16G16B16A16_SFLOAT => Self::Rgba16Float,
            vk::Format::R8_UNORM => Self::R8Unorm,
            vk::Format::D32_SFLOAT => Self::Depth32Float,
            other => panic!("Format::from_vk: unsupported format {other:?}"),
        }
    }
}
