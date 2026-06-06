use crate::vulkan::Instance;
use ash::ext::debug_utils;
use ash::vk;

pub struct DebugMessenger {
    loader: debug_utils::Instance,
    handle: vk::DebugUtilsMessengerEXT,
}

impl DebugMessenger {
    pub fn new(instance: &Instance) -> anyhow::Result<Self> {
        let loader = ash::ext::debug_utils::Instance::new(&instance.entry, &instance.handle);

        let create_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
            .message_severity(
                vk::DebugUtilsMessageSeverityFlagsEXT::ERROR | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING,
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

pub fn cmd_begin_label(debug_utils: &debug_utils::Device, cmd: vk::CommandBuffer, name: &str) {
    let name = std::ffi::CString::new(name).unwrap();
    let label = vk::DebugUtilsLabelEXT::default().label_name(&name);
    unsafe { debug_utils.cmd_begin_debug_utils_label(cmd, &label) };
}

pub fn cmd_end_label(debug_utils: &debug_utils::Device, cmd: vk::CommandBuffer) {
    unsafe { debug_utils.cmd_end_debug_utils_label(cmd) };
}

pub fn set_object_name(debug_utils: &ash::ext::debug_utils::Device, object: impl vk::Handle, name: &str) {
    let name = std::ffi::CString::new(name).unwrap();
    let info = vk::DebugUtilsObjectNameInfoEXT::default().object_handle(object).object_name(&name);
    unsafe { debug_utils.set_debug_utils_object_name(&info).ok() };
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
