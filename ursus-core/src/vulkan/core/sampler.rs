use ash::vk;

fn base_linear() -> vk::SamplerCreateInfo<'static> {
    vk::SamplerCreateInfo::default().mag_filter(vk::Filter::LINEAR).min_filter(vk::Filter::LINEAR)
}

pub fn create_linear_clamp_sampler(device: &ash::Device) -> anyhow::Result<vk::Sampler> {
    Ok(unsafe {
        device.create_sampler(
            &base_linear()
                .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
                .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE),
            None,
        )?
    })
}

pub fn create_linear_repeat_aniso_sampler(device: &ash::Device, max_anisotropy: f32) -> anyhow::Result<vk::Sampler> {
    Ok(unsafe {
        device.create_sampler(
            &base_linear()
                .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
                .address_mode_u(vk::SamplerAddressMode::REPEAT)
                .address_mode_v(vk::SamplerAddressMode::REPEAT)
                .address_mode_w(vk::SamplerAddressMode::REPEAT)
                .anisotropy_enable(true)
                .max_anisotropy(max_anisotropy)
                .max_lod(vk::LOD_CLAMP_NONE),
            None,
        )?
    })
}
