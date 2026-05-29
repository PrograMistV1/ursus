use super::Instance;
use ash::vk;

pub struct DebugMessenger {
    loader: ash::ext::debug_utils::Instance,
    handle: vk::DebugUtilsMessengerEXT,
}

impl DebugMessenger {
    pub fn new(instance: &Instance) -> anyhow::Result<Self> {
        let loader = ash::ext::debug_utils::Instance::new(
            &instance.entry,
            &instance.handle,
        );

        let create_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
            .message_severity(
                vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                    | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING,
            )
            .message_type(
                vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                    | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                    | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
            )
            .pfn_user_callback(Some(debug_callback));

        let handle = unsafe { loader.create_debug_utils_messenger(&create_info, None)? };
        log::info!("Validation layers активированы");

        Ok(Self { loader, handle })
    }
}

impl Drop for DebugMessenger {
    fn drop(&mut self) {
        unsafe { self.loader.destroy_debug_utils_messenger(self.handle, None) };
    }
}

unsafe extern "system" fn debug_callback(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    _type: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _: *mut std::ffi::c_void,
) -> vk::Bool32 {
    let msg = unsafe { std::ffi::CStr::from_ptr((*data).p_message) };
    match severity {
        vk::DebugUtilsMessageSeverityFlagsEXT::ERROR => log::error!("[Vulkan] {:?}", msg),
        vk::DebugUtilsMessageSeverityFlagsEXT::WARNING => log::warn!("[Vulkan] {:?}", msg),
        _ => log::debug!("[Vulkan] {:?}", msg),
    }
    vk::FALSE
}
