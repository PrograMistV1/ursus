use ash::vk;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    Nearest,
    Linear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressMode {
    ClampToEdge,
    ClampToBorderOpaqueWhite,
}

#[derive(Debug, Clone, Copy)]
pub struct SamplerDesc {
    pub filter: Filter,
    pub address_mode: AddressMode,
    pub compare_less_or_equal: bool,
}

impl SamplerDesc {
    pub fn linear_clamp() -> Self {
        Self { filter: Filter::Linear, address_mode: AddressMode::ClampToEdge, compare_less_or_equal: false }
    }
    pub fn nearest_clamp() -> Self {
        Self { filter: Filter::Nearest, address_mode: AddressMode::ClampToEdge, compare_less_or_equal: false }
    }
    pub fn shadow_compare() -> Self {
        Self {
            filter: Filter::Linear,
            address_mode: AddressMode::ClampToBorderOpaqueWhite,
            compare_less_or_equal: true,
        }
    }
}

pub fn create_from_desc(device: &ash::Device, desc: SamplerDesc) -> anyhow::Result<vk::Sampler> {
    let filter = match desc.filter {
        Filter::Nearest => vk::Filter::NEAREST,
        Filter::Linear => vk::Filter::LINEAR,
    };
    let address_mode = match desc.address_mode {
        AddressMode::ClampToEdge => vk::SamplerAddressMode::CLAMP_TO_EDGE,
        AddressMode::ClampToBorderOpaqueWhite => vk::SamplerAddressMode::CLAMP_TO_BORDER,
    };

    let mut info = vk::SamplerCreateInfo::default()
        .mag_filter(filter)
        .min_filter(filter)
        .address_mode_u(address_mode)
        .address_mode_v(address_mode);

    if matches!(desc.address_mode, AddressMode::ClampToBorderOpaqueWhite) {
        info = info.border_color(vk::BorderColor::FLOAT_OPAQUE_WHITE);
    }

    if desc.compare_less_or_equal {
        info = info.compare_enable(true).compare_op(vk::CompareOp::LESS_OR_EQUAL);
    }

    Ok(unsafe { device.create_sampler(&info, None)? })
}
