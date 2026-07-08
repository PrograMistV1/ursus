use crate::render::gfx::{Format, VertexAttribute, VertexFormat, VertexLayout};
use crate::vulkan::core::memory::alloc_buffer;
use ash::vk;
use glam::{Vec2, Vec3};

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub tangent: [f32; 4],
}
unsafe impl bytemuck::Pod for Vertex {}
unsafe impl bytemuck::Zeroable for Vertex {}

impl VertexFormat for Vertex {
    fn layout() -> VertexLayout {
        VertexLayout {
            stride: size_of::<Self>() as u32,
            attributes: vec![
                VertexAttribute { location: 0, format: Format::Rgb32Float, offset: 0 }, // position
                VertexAttribute { location: 1, format: Format::Rgb32Float, offset: 12 }, // normal
                VertexAttribute { location: 2, format: Format::Rg32Float, offset: 24 }, // uv
                VertexAttribute { location: 3, format: Format::Rgba32Float, offset: 32 }, // tangent
            ],
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn from_vertices(vertices: &[Vertex]) -> Self {
        let mut min = Vec3::splat(f32::MAX);
        let mut max = Vec3::splat(f32::MIN);
        for v in vertices {
            let p = Vec3::from(v.position);
            min = min.min(p);
            max = max.max(p);
        }
        Self { min, max }
    }

    pub fn intersects_frustum(&self, planes: &[glam::Vec4; 6]) -> bool {
        for plane in planes {
            let normal = Vec3::new(plane.x, plane.y, plane.z);
            let p = Vec3::new(
                if normal.x >= 0.0 { self.max.x } else { self.min.x },
                if normal.y >= 0.0 { self.max.y } else { self.min.y },
                if normal.z >= 0.0 { self.max.z } else { self.min.z },
            );
            if normal.dot(p) + plane.w < 0.0 {
                return false;
            }
        }
        true
    }
}

impl Vertex {
    pub fn new(position: Vec3, normal: Vec3, uv: Vec2) -> Self {
        Self { position: position.into(), normal: normal.into(), uv: uv.into(), tangent: [1.0, 0.0, 0.0, 1.0] }
    }
}

#[derive(Debug, Clone)]
pub struct CpuMesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub name: String,
}

impl CpuMesh {
    pub fn new(name: impl Into<String>, vertices: Vec<Vertex>, indices: Vec<u32>) -> Self {
        Self { vertices, indices, name: name.into() }
    }

    pub fn vertex_count(&self) -> u32 {
        self.vertices.len() as u32
    }
    pub fn index_count(&self) -> u32 {
        self.indices.len() as u32
    }

    pub fn triangle() -> Self {
        let vertices = vec![
            Vertex::new(Vec3::new(0.0, 0.5, 0.0), Vec3::Z, Vec2::new(0.5, 0.0)),
            Vertex::new(Vec3::new(-0.5, -0.5, 0.0), Vec3::Z, Vec2::new(0.0, 1.0)),
            Vertex::new(Vec3::new(0.5, -0.5, 0.0), Vec3::Z, Vec2::new(1.0, 1.0)),
        ];
        Self::new("triangle", vertices, vec![0, 1, 2])
    }

    pub fn cube() -> Self {
        let faces: &[(Vec3, Vec3, Vec3)] = &[
            (Vec3::Z, Vec3::X, Vec3::Y),         // front
            (Vec3::NEG_Z, Vec3::NEG_X, Vec3::Y), // back
            (Vec3::X, Vec3::NEG_Z, Vec3::Y),     // right
            (Vec3::NEG_X, Vec3::Z, Vec3::Y),     // left
            (Vec3::Y, Vec3::X, Vec3::NEG_Z),     // top
            (Vec3::NEG_Y, Vec3::X, Vec3::Z),     // bottom
        ];

        let uvs = [
            Vec2::new(0.0, 1.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(0.0, 0.0),
        ];

        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        for (normal, right, up) in faces {
            let base = vertices.len() as u32;
            let center = *normal * 0.5;
            let corners = [
                center - *right * 0.5 - *up * 0.5,
                center + *right * 0.5 - *up * 0.5,
                center + *right * 0.5 + *up * 0.5,
                center - *right * 0.5 + *up * 0.5,
            ];
            for (pos, uv) in corners.iter().zip(uvs.iter()) {
                vertices.push(Vertex::new(*pos, *normal, *uv));
            }
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }

        Self::new("cube", vertices, indices)
    }

    pub fn plane(size: f32, subdivisions: u32) -> Self {
        let n = subdivisions + 1;
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        for z in 0..n {
            for x in 0..n {
                let fx = x as f32 / subdivisions as f32;
                let fz = z as f32 / subdivisions as f32;
                vertices.push(Vertex::new(
                    Vec3::new((fx - 0.5) * size, 0.0, (fz - 0.5) * size),
                    Vec3::Y,
                    Vec2::new(fx, fz),
                ));
            }
        }

        for z in 0..subdivisions {
            for x in 0..subdivisions {
                let i = z * n + x;
                indices.extend_from_slice(&[i, i + n, i + 1, i + 1, i + n, i + n + 1]);
            }
        }

        Self::new(format!("plane_{size}"), vertices, indices)
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
        device,
        instance,
        physical_device,
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
        device,
        instance,
        physical_device,
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
