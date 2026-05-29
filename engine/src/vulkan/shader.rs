use ash::vk;
use std::path::Path;

pub struct ShaderModule {
    pub handle: vk::ShaderModule,
    device: ash::Device,
}

impl ShaderModule {
    pub fn from_file(device: &ash::Device, path: &Path) -> anyhow::Result<Self> {
        let bytes = std::fs::read(path)
            .map_err(|e| anyhow::anyhow!("Не удалось прочитать шейдер {:?}: {}", path, e))?;
        Self::from_bytes(device, &bytes)
    }

    pub fn from_bytes(device: &ash::Device, bytes: &[u8]) -> anyhow::Result<Self> {
        let code = ash::util::read_spv(&mut std::io::Cursor::new(bytes))?;
        let create_info = vk::ShaderModuleCreateInfo::default().code(&code);
        let handle = unsafe { device.create_shader_module(&create_info, None)? };
        Ok(Self {
            handle,
            device: device.clone(),
        })
    }
}

impl Drop for ShaderModule {
    fn drop(&mut self) {
        unsafe { self.device.destroy_shader_module(self.handle, None) };
    }
}
