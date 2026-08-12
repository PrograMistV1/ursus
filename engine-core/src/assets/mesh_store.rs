use crate::assets::mesh::{CpuMesh, GpuMesh};
use crate::components::mesh::MeshHandle;
use ash::vk;
use std::collections::HashMap;

enum GpuMeshState {
    Ready(Box<GpuMesh>),
    Failed,
}

/// Owns GPU meshes and the Vulkan resources required to upload them.
pub struct MeshStore {
    meshes: HashMap<MeshHandle, GpuMeshState>,
    device: ash::Device,
    physical_device: vk::PhysicalDevice,
    instance: ash::Instance,
    command_pool: vk::CommandPool,
    queue: vk::Queue,
}

impl MeshStore {
    pub fn new(
        device: ash::Device,
        physical_device: vk::PhysicalDevice,
        instance: ash::Instance,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
    ) -> Self {
        Self { meshes: HashMap::new(), device, physical_device, instance, command_pool, queue }
    }

    pub fn upload(&mut self, handle: MeshHandle, cpu_mesh: &CpuMesh) -> anyhow::Result<()> {
        match GpuMesh::upload(
            &self.device,
            self.physical_device,
            &self.instance,
            cpu_mesh,
            self.command_pool,
            self.queue,
        ) {
            Ok(gpu) => {
                self.meshes.insert(handle, GpuMeshState::Ready(Box::new(gpu)));
                Ok(())
            }
            Err(e) => {
                self.meshes.insert(handle, GpuMeshState::Failed);
                Err(e)
            }
        }
    }

    pub fn get(&self, handle: MeshHandle) -> Option<&GpuMesh> {
        match self.meshes.get(&handle)? {
            GpuMeshState::Ready(gpu) => Some(gpu),
            GpuMeshState::Failed => None,
        }
    }

    pub fn is_ready(&self, handle: MeshHandle) -> bool {
        matches!(self.meshes.get(&handle), Some(GpuMeshState::Ready(_)))
    }
}
