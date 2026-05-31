use ash::vk;
use raw_window_handle::RawDisplayHandle;

pub struct Instance {
    pub entry: ash::Entry,
    pub handle: ash::Instance,
}

impl Instance {
    pub fn new(display: RawDisplayHandle, validation: bool) -> anyhow::Result<Self> {
        let entry = unsafe { ash::Entry::load()? };

        let mut extensions = ash_window::enumerate_required_extensions(display)?.to_vec();

        if validation {
            extensions.push(ash::ext::debug_utils::NAME.as_ptr());
        }

        let layers: Vec<*const i8> = if validation && Self::has_validation(&entry) {
            log::info!("Validation layers включены");
            vec![c"VK_LAYER_KHRONOS_validation".as_ptr()]
        } else {
            if validation {
                log::warn!("VK_LAYER_KHRONOS_validation не найден — запуск без валидации");
            }
            vec![]
        };

        let app_name = c"engine";
        let engine_name = c"engine";

        let app_info = vk::ApplicationInfo::default()
            .application_name(app_name)
            .engine_name(engine_name)
            .api_version(vk::API_VERSION_1_3);

        let create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_extension_names(&extensions)
            .enabled_layer_names(&layers);

        let handle = unsafe { entry.create_instance(&create_info, None)? };
        log::info!("Vulkan instance создан (API 1.3)");

        Ok(Self { entry, handle })
    }

    fn has_validation(entry: &ash::Entry) -> bool {
        let Ok(layers) = (unsafe { entry.enumerate_instance_layer_properties() }) else {
            return false;
        };
        layers.iter().any(|l| {
            let name = unsafe { std::ffi::CStr::from_ptr(l.layer_name.as_ptr()) };
            name == c"VK_LAYER_KHRONOS_validation"
        })
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        unsafe { self.handle.destroy_instance(None) };
        log::info!("Vulkan instance уничтожен");
    }
}
