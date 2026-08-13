use crate::render::gfx::sampler::desc;
use crate::render::gfx::sampler::desc::SamplerDesc;
use crate::render::gfx::types::SamplerId;
use ash::vk;

struct StoredSampler {
    handle: vk::Sampler,
}

pub struct SamplerStore {
    samplers: Vec<StoredSampler>,
    device: ash::Device,
}

impl SamplerStore {
    pub fn new(device: ash::Device) -> Self {
        Self { samplers: Vec::new(), device }
    }

    pub fn create(&mut self, desc: SamplerDesc) -> anyhow::Result<SamplerId> {
        let handle = desc::create_from_desc(&self.device, desc)?;
        let id = SamplerId(self.samplers.len() as u32);
        self.samplers.push(StoredSampler { handle });
        Ok(id)
    }

    pub(crate) fn handle(&self, id: SamplerId) -> vk::Sampler {
        self.samplers[id.0 as usize].handle
    }
}

impl Drop for SamplerStore {
    fn drop(&mut self) {
        unsafe {
            for s in &self.samplers {
                self.device.destroy_sampler(s.handle, None);
            }
        }
    }
}
