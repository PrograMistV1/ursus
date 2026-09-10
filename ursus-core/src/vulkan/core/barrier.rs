use ash::vk;

pub fn make_barrier_range(
    image: vk::Image,
    aspect_mask: vk::ImageAspectFlags,
    base_mip_level: u32,
    level_count: u32,
    old_layout: vk::ImageLayout,
    new_layout: vk::ImageLayout,
) -> vk::ImageMemoryBarrier2<'static> {
    let (src_stage, src_access, dst_stage, dst_access) = layout_transition_masks(old_layout, new_layout);

    vk::ImageMemoryBarrier2::default()
        .src_stage_mask(src_stage)
        .src_access_mask(src_access)
        .dst_stage_mask(dst_stage)
        .dst_access_mask(dst_access)
        .old_layout(old_layout)
        .new_layout(new_layout)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(vk::ImageSubresourceRange {
            aspect_mask,
            base_mip_level,
            level_count,
            base_array_layer: 0,
            layer_count: 1,
        })
}

/// Returns (stage, access) for transitioning from/to the given layout.
/// `is_src` determines whether the layout is interpreted as "from" (true)
/// or "to" (false) - the only asymmetry in Vulkan semantics here
/// is UNDEFINED (valid only as src) and the difference between EARLY/LATE fragment
/// tests for depth attachments (see the comment in the branch).
fn stage_access(layout: vk::ImageLayout, is_src: bool) -> (vk::PipelineStageFlags2, vk::AccessFlags2) {
    use vk::AccessFlags2 as A;
    use vk::ImageLayout as L;
    use vk::PipelineStageFlags2 as S;

    match layout {
        L::UNDEFINED => {
            assert!(is_src, "UNDEFINED is only valid as the source (src) layout of a transition");
            (S::TOP_OF_PIPE, A::empty())
        }
        L::PRESENT_SRC_KHR => {
            if is_src {
                (S::TOP_OF_PIPE, A::empty())
            } else {
                (S::BOTTOM_OF_PIPE, A::empty())
            }
        }
        L::COLOR_ATTACHMENT_OPTIMAL => (S::COLOR_ATTACHMENT_OUTPUT, A::COLOR_ATTACHMENT_WRITE),
        L::DEPTH_ATTACHMENT_OPTIMAL => {
            // src: the previous write must be visible (LATE = after blending/writes).
            // dst: subsequent read/write operations can begin already at EARLY tests.
            if is_src {
                (S::LATE_FRAGMENT_TESTS, A::DEPTH_STENCIL_ATTACHMENT_WRITE)
            } else {
                (S::EARLY_FRAGMENT_TESTS, A::DEPTH_STENCIL_ATTACHMENT_READ | A::DEPTH_STENCIL_ATTACHMENT_WRITE)
            }
        }
        L::SHADER_READ_ONLY_OPTIMAL => (S::FRAGMENT_SHADER, A::SHADER_READ),
        L::TRANSFER_SRC_OPTIMAL => (S::TRANSFER, A::TRANSFER_READ),
        L::TRANSFER_DST_OPTIMAL => (S::TRANSFER, A::TRANSFER_WRITE),
        other => panic!("stage_access: unsupported layout {:?}", other),
    }
}

pub(crate) fn layout_transition_masks(
    from: vk::ImageLayout,
    to: vk::ImageLayout,
) -> (vk::PipelineStageFlags2, vk::AccessFlags2, vk::PipelineStageFlags2, vk::AccessFlags2) {
    let (src_stage, src_access) = stage_access(from, true);
    let (dst_stage, dst_access) = stage_access(to, false);
    (src_stage, src_access, dst_stage, dst_access)
}
