use ash::vk;

pub const MAX_POINT_LIGHTS: usize = 16;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DirectionalLight {
    pub direction: [f32; 4],
    pub color: [f32; 4],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct GpuPointLight {
    pub position: [f32; 4], // xyz = pos, w = radius
    pub color: [f32; 4],    // rgb = color, a = intensity
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LightingUbo {
    pub directional: DirectionalLight,
    pub point_lights: [GpuPointLight; MAX_POINT_LIGHTS],
    pub point_light_count: u32,
    pub _pad: [u32; 3],
    pub light_space_matrix: [[f32; 4]; 4],
}

impl Default for LightingUbo {
    fn default() -> Self {
        Self {
            directional: DirectionalLight {
                direction: [-0.3, -1.0, -0.5, 0.0],
                color: [1.0, 0.95, 0.85, 2.0],
            },
            point_lights: [GpuPointLight {
                position: [0.0; 4],
                color: [0.0; 4],
            }; MAX_POINT_LIGHTS],
            point_light_count: 0,
            _pad: [0; 3],
            light_space_matrix: glam::Mat4::IDENTITY.to_cols_array_2d(),
        }
    }
}

pub struct LightBuffer {
    pub buffer: vk::Buffer,
    pub memory: vk::DeviceMemory,
    pub mapped: *mut LightingUbo,
    device: ash::Device,
}

unsafe impl Send for LightBuffer {}
unsafe impl Sync for LightBuffer {}

impl LightBuffer {
    pub fn new(
        device: &ash::Device,
        physical_device: vk::PhysicalDevice,
        instance: &ash::Instance,
    ) -> anyhow::Result<Self> {
        let size = size_of::<LightingUbo>() as vk::DeviceSize;

        let buf_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(vk::BufferUsageFlags::UNIFORM_BUFFER)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let buffer = unsafe { device.create_buffer(&buf_info, None)? };

        let req = unsafe { device.get_buffer_memory_requirements(buffer) };
        let mem_props = unsafe { instance.get_physical_device_memory_properties(physical_device) };
        let mem_type = find_memory_type(
            &mem_props,
            req.memory_type_bits,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;

        let memory = unsafe {
            device.allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(req.size)
                    .memory_type_index(mem_type),
                None,
            )?
        };
        unsafe { device.bind_buffer_memory(buffer, memory, 0)? };

        let mapped = unsafe {
            device.map_memory(memory, 0, size, vk::MemoryMapFlags::empty())? as *mut LightingUbo
        };

        unsafe { std::ptr::write(mapped, LightingUbo::default()) };

        Ok(Self {
            buffer,
            memory,
            mapped,
            device: device.clone(),
        })
    }

    pub fn upload(&self, data: &LightingUbo) {
        unsafe { std::ptr::write(self.mapped, *data) };
    }
}

impl Drop for LightBuffer {
    fn drop(&mut self) {
        unsafe {
            self.device.unmap_memory(self.memory);
            self.device.destroy_buffer(self.buffer, None);
            self.device.free_memory(self.memory, None);
        }
    }
}

fn find_memory_type(
    props: &vk::PhysicalDeviceMemoryProperties,
    type_filter: u32,
    required: vk::MemoryPropertyFlags,
) -> anyhow::Result<u32> {
    for i in 0..props.memory_type_count {
        if (type_filter & (1 << i)) != 0
            && props.memory_types[i as usize]
                .property_flags
                .contains(required)
        {
            return Ok(i);
        }
    }
    anyhow::bail!("Не найден подходящий тип памяти для LightBuffer")
}

pub fn compute_light_view_proj(
    direction: [f32; 3],
    scene_center: glam::Vec3,
    scene_radius: f32,
) -> glam::Mat4 {
    let dir = glam::Vec3::from(direction).normalize();
    let light_pos = scene_center - dir * scene_radius;
    let view = glam::Mat4::look_at_rh(light_pos, scene_center, glam::Vec3::Y);
    let ortho = glam::Mat4::orthographic_rh(
        -scene_radius,
        scene_radius,
        -scene_radius,
        scene_radius,
        0.1,
        scene_radius * 2.0,
    );
    ortho * view
}
