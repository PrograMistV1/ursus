use crate::vulkan::Instance;
use ash::vk;

pub struct Device {
    pub handle: ash::Device,
    pub physical: vk::PhysicalDevice,
    pub physical_props: vk::PhysicalDeviceProperties,
    pub graphics_queue: vk::Queue,
    pub present_queue: vk::Queue,
    pub graphics_family: u32,
    pub present_family: u32,
}

impl Device {
    pub fn new(instance: &Instance, surface: vk::SurfaceKHR) -> anyhow::Result<Self> {
        let (physical, graphics_family, present_family) =
            Self::pick_physical(&instance.handle, &instance.entry, surface)?;

        let physical_props = unsafe { instance.handle.get_physical_device_properties(physical) };
        let name = unsafe { std::ffi::CStr::from_ptr(physical_props.device_name.as_ptr()) };
        log::info!("GPU: {:?}", name);

        let unique_families: Vec<u32> = if graphics_family != present_family {
            vec![graphics_family, present_family]
        } else {
            vec![graphics_family]
        };

        let queue_infos: Vec<_> = unique_families
            .iter()
            .map(|&family| vk::DeviceQueueCreateInfo::default().queue_family_index(family).queue_priorities(&[1.0]))
            .collect();

        let extensions = [ash::khr::swapchain::NAME.as_ptr()];

        let mut features12 = vk::PhysicalDeviceVulkan12Features::default()
            .buffer_device_address(true)
            .descriptor_indexing(true)
            .runtime_descriptor_array(true)
            .shader_sampled_image_array_non_uniform_indexing(true)
            .descriptor_binding_sampled_image_update_after_bind(true)
            .descriptor_binding_partially_bound(true)
            .descriptor_binding_variable_descriptor_count(true);

        let mut features13 =
            vk::PhysicalDeviceVulkan13Features::default().dynamic_rendering(true).synchronization2(true);

        let features10 = vk::PhysicalDeviceFeatures::default().sampler_anisotropy(true);

        let create_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queue_infos)
            .enabled_extension_names(&extensions)
            .enabled_features(&features10)
            .push_next(&mut features12)
            .push_next(&mut features13);

        let handle = unsafe { instance.handle.create_device(physical, &create_info, None)? };

        let graphics_queue = unsafe { handle.get_device_queue(graphics_family, 0) };
        let present_queue = unsafe { handle.get_device_queue(present_family, 0) };

        log::info!("Logical device created");

        Ok(Self { handle, physical, physical_props, graphics_queue, present_queue, graphics_family, present_family })
    }

    fn pick_physical(
        instance: &ash::Instance,
        entry: &ash::Entry,
        surface: vk::SurfaceKHR,
    ) -> anyhow::Result<(vk::PhysicalDevice, u32, u32)> {
        let surface_loader = ash::khr::surface::Instance::new(entry, instance);
        let devices = unsafe { instance.enumerate_physical_devices()? };

        let mut fallback = None;

        for device in devices {
            let props = unsafe { instance.get_physical_device_properties(device) };
            let queues = unsafe { instance.get_physical_device_queue_family_properties(device) };

            let mut graphics_family = None;
            let mut present_family = None;

            for (i, q) in queues.iter().enumerate() {
                let i = i as u32;
                if q.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
                    graphics_family = Some(i);
                }
                let present_support =
                    unsafe { surface_loader.get_physical_device_surface_support(device, i, surface)? };
                if present_support {
                    present_family = Some(i);
                }
            }

            if let (Some(gf), Some(pf)) = (graphics_family, present_family) {
                if props.device_type == vk::PhysicalDeviceType::DISCRETE_GPU {
                    return Ok((device, gf, pf));
                }
                fallback = Some((device, gf, pf));
            }
        }

        fallback.ok_or_else(|| anyhow::anyhow!("Подходящая GPU не найдена"))
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        unsafe { self.handle.destroy_device(None) };
        log::debug!("Device destroyed");
    }
}
