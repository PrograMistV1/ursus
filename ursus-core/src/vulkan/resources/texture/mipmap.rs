use crate::vulkan::core::make_barrier_range;
use ash::vk;

fn transition(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    image: vk::Image,
    from: vk::ImageLayout,
    to: vk::ImageLayout,
    base_mip: u32,
    level_count: u32,
) {
    let barrier = make_barrier_range(image, vk::ImageAspectFlags::COLOR, base_mip, level_count, from, to);
    unsafe {
        device.cmd_pipeline_barrier2(
            cmd,
            &vk::DependencyInfo::default().image_memory_barriers(std::slice::from_ref(&barrier)),
        );
    }
}

/// Регион копирования из staging-буфера в mip 0 (единственное, что нужно
/// для загрузки исходных пикселей - остальные mip-уровни получаются blit'ом).
fn base_level_copy_region(width: u32, height: u32) -> vk::BufferImageCopy {
    vk::BufferImageCopy::default()
        .buffer_offset(0)
        .buffer_row_length(0)
        .buffer_image_height(0)
        .image_subresource(vk::ImageSubresourceLayers {
            aspect_mask: vk::ImageAspectFlags::COLOR,
            mip_level: 0,
            base_array_layer: 0,
            layer_count: 1,
        })
        .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
        .image_extent(vk::Extent3D { width, height, depth: 1 })
}

fn copy_staging_to_mip0(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    image: vk::Image,
    staging_buffer: vk::Buffer,
    width: u32,
    height: u32,
) {
    let region = base_level_copy_region(width, height);
    unsafe {
        device.cmd_copy_buffer_to_image(
            cmd,
            staging_buffer,
            image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            std::slice::from_ref(&region),
        );
    }
}

/// Copies `staging` to mip 0 and generates the remaining levels using sequential blits.
/// Expects the image to be in the `UNDEFINED` layout on entry (for all mip levels).
/// On exit, all levels are in `SHADER_READ_ONLY_OPTIMAL`.
pub(super) fn upload_with_mipchain(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    image: vk::Image,
    staging_buffer: vk::Buffer,
    width: u32,
    height: u32,
    mip_levels: u32,
) {
    transition(device, cmd, image, vk::ImageLayout::UNDEFINED, vk::ImageLayout::TRANSFER_DST_OPTIMAL, 0, mip_levels);

    copy_staging_to_mip0(device, cmd, image, staging_buffer, width, height);

    let mut mip_w = width as i32;
    let mut mip_h = height as i32;

    for level in 1..mip_levels {
        transition(
            device,
            cmd,
            image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            level - 1,
            1,
        );

        let blit = vk::ImageBlit::default()
            .src_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: level - 1,
                base_array_layer: 0,
                layer_count: 1,
            })
            .src_offsets([
                vk::Offset3D { x: 0, y: 0, z: 0 },
                vk::Offset3D { x: mip_w, y: mip_h, z: 1 },
            ])
            .dst_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                mip_level: level,
                base_array_layer: 0,
                layer_count: 1,
            })
            .dst_offsets([
                vk::Offset3D { x: 0, y: 0, z: 0 },
                vk::Offset3D {
                    x: if mip_w > 1 { mip_w / 2 } else { 1 },
                    y: if mip_h > 1 { mip_h / 2 } else { 1 },
                    z: 1,
                },
            ]);

        unsafe {
            device.cmd_blit_image(
                cmd,
                image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                std::slice::from_ref(&blit),
                vk::Filter::LINEAR,
            );
        }

        transition(
            device,
            cmd,
            image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            level - 1,
            1,
        );

        if mip_w > 1 {
            mip_w /= 2;
        }
        if mip_h > 1 {
            mip_h /= 2;
        }
    }

    transition(
        device,
        cmd,
        image,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        mip_levels - 1,
        1,
    );
}

/// Variant without mip chain generation - simply copies the staging data to the single mip 0.
pub(super) fn upload_single_level(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    image: vk::Image,
    staging_buffer: vk::Buffer,
    width: u32,
    height: u32,
) {
    transition(device, cmd, image, vk::ImageLayout::UNDEFINED, vk::ImageLayout::TRANSFER_DST_OPTIMAL, 0, 1);

    copy_staging_to_mip0(device, cmd, image, staging_buffer, width, height);

    transition(
        device,
        cmd,
        image,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        0,
        1,
    );
}
