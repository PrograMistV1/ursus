use crate::assets::mesh::{Aabb, CpuMesh};
use crate::vulkan::core::memory::alloc_buffer;
use crate::vulkan::core::DeviceContext;
use ash::vk;
use std::collections::HashMap;
use ursus_ecs::components::mesh::MeshHandle;

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

pub struct GpuMesh {
    pub vertex_buffer: vk::Buffer,
    pub index_buffer: vk::Buffer,
    pub vertex_memory: vk::DeviceMemory,
    pub index_memory: vk::DeviceMemory,
    pub index_count: u32,
    pub vertex_count: u32,
    pub name: String,
    pub aabb: Aabb,
    device: ash::Device,
}

impl GpuMesh {
    pub fn upload(
        device: &ash::Device,
        physical_device: vk::PhysicalDevice,
        instance: &ash::Instance,
        cpu_mesh: &CpuMesh,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
    ) -> anyhow::Result<Self> {
        let vertex_data: &[u8] = bytemuck::cast_slice(&cpu_mesh.vertices);
        let index_data: &[u8] = bytemuck::cast_slice(&cpu_mesh.indices);

        let (vertex_buffer, vertex_memory) = create_buffer_with_data(
            device,
            instance,
            physical_device,
            vertex_data,
            vk::BufferUsageFlags::VERTEX_BUFFER,
            command_pool,
            queue,
        )?;
        let (index_buffer, index_memory) = create_buffer_with_data(
            device,
            instance,
            physical_device,
            index_data,
            vk::BufferUsageFlags::INDEX_BUFFER,
            command_pool,
            queue,
        )?;

        log::debug!("GpuMesh '{}': {} verts, {} idx", cpu_mesh.name, cpu_mesh.vertex_count(), cpu_mesh.index_count());

        Ok(Self {
            vertex_buffer,
            index_buffer,
            vertex_memory,
            index_memory,
            index_count: cpu_mesh.index_count(),
            vertex_count: cpu_mesh.vertex_count(),
            name: cpu_mesh.name.clone(),
            aabb: Aabb::from_vertices(&cpu_mesh.vertices),
            device: device.clone(),
        })
    }
}

impl Drop for GpuMesh {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_buffer(self.vertex_buffer, None);
            self.device.free_memory(self.vertex_memory, None);
            self.device.destroy_buffer(self.index_buffer, None);
            self.device.free_memory(self.index_memory, None);
        }
        log::debug!("GpuMesh '{}' выгружен", self.name);
    }
}

fn copy_buffer(
    device: &ash::Device,
    command_pool: vk::CommandPool,
    queue: vk::Queue,
    src: vk::Buffer,
    dst: vk::Buffer,
    size: vk::DeviceSize,
) -> anyhow::Result<()> {
    let alloc_info = vk::CommandBufferAllocateInfo::default()
        .command_pool(command_pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(1);
    let cmd = unsafe { device.allocate_command_buffers(&alloc_info)?[0] };
    unsafe {
        device.begin_command_buffer(
            cmd,
            &vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
        )?;
        device.cmd_copy_buffer(cmd, src, dst, &[vk::BufferCopy::default().size(size)]);
        device.end_command_buffer(cmd)?;
        device.queue_submit(
            queue,
            &[vk::SubmitInfo::default().command_buffers(std::slice::from_ref(&cmd))],
            vk::Fence::null(),
        )?;
        device.queue_wait_idle(queue)?;
        device.free_command_buffers(command_pool, &[cmd]);
    }
    Ok(())
}

fn create_buffer_with_data(
    device: &ash::Device,
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    data: &[u8],
    usage: vk::BufferUsageFlags,
    command_pool: vk::CommandPool,
    queue: vk::Queue,
) -> anyhow::Result<(vk::Buffer, vk::DeviceMemory)> {
    let size = data.len() as vk::DeviceSize;
    let (staging, staging_mem) = alloc_buffer(
        DeviceContext { device, instance, physical_device },
        size,
        vk::BufferUsageFlags::TRANSFER_SRC,
        vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
    )?;
    unsafe {
        let ptr = device.map_memory(staging_mem, 0, size, vk::MemoryMapFlags::empty())? as *mut u8;
        std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
        device.unmap_memory(staging_mem);
    }
    let (buf, mem) = alloc_buffer(
        DeviceContext { device, instance, physical_device },
        size,
        usage | vk::BufferUsageFlags::TRANSFER_DST,
        vk::MemoryPropertyFlags::DEVICE_LOCAL,
    )?;
    copy_buffer(device, command_pool, queue, staging, buf, size)?;
    unsafe {
        device.destroy_buffer(staging, None);
        device.free_memory(staging_mem, None);
    }
    Ok((buf, mem))
}
