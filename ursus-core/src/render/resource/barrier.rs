use crate::render::resource::desc::ResourceKind;
use crate::vulkan::core::barrier::make_barrier_range;
use ash::vk;

pub fn make_barrier(
    image: vk::Image,
    kind: ResourceKind,
    old_layout: vk::ImageLayout,
    new_layout: vk::ImageLayout,
) -> vk::ImageMemoryBarrier2<'static> {
    make_barrier_range(image, kind.aspect_mask(), 0, 1, old_layout, new_layout)
}
