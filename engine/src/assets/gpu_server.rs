use crate::assets::cpu_server::TextureHandle;
use crate::assets::material::MaterialData;
use crate::assets::mesh::{CpuMesh, GpuMesh};
use crate::assets::shader_registry::TextureSlot;
use crate::assets::{builtin_shaders, ShaderRegistry};
use crate::components::mesh::{MaterialHandle, MeshHandle};
use crate::render::world::RenderWorld;
use crate::vulkan::{BindlessSet, GpuTexture, MaterialBuffer};
use ash::vk;
use std::collections::HashMap;

pub const BINDLESS_SLOT_WHITE: u32 = 0;

enum GpuMeshState {
    Ready(GpuMesh),
    Failed,
}

pub struct GpuAssetServer {
    gpu_meshes: HashMap<MeshHandle, GpuMeshState>,
    texture_slots: HashMap<TextureHandle, u32>,
    gpu_textures: HashMap<u32, GpuTexture>,
    materials: Vec<MaterialData>,

    pub shaders: ShaderRegistry,
    pub material_buffer: MaterialBuffer,
    pub bindless: BindlessSet,

    device: ash::Device,
    physical_device: vk::PhysicalDevice,
    instance: ash::Instance,
    command_pool: vk::CommandPool,
    queue: vk::Queue,
}

impl GpuAssetServer {
    pub fn new(
        device: ash::Device,
        physical_device: vk::PhysicalDevice,
        instance: ash::Instance,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
    ) -> anyhow::Result<Self> {
        let mut bindless = BindlessSet::new(&device, physical_device, &instance, command_pool, queue)?;
        assert_eq!(bindless.next_slot(), 1, "slot 0 must be white fallback");

        let material_buffer = MaterialBuffer::new(&device, physical_device, &instance)?;

        let mut shaders = ShaderRegistry::empty();
        builtin_shaders::register_builtin(&mut shaders);

        log::info!("GpuAssetServer: white=slot0, text_renderer готов, next_slot={}", bindless.next_slot());

        Ok(Self {
            gpu_meshes: HashMap::new(),
            texture_slots: HashMap::new(),
            gpu_textures: HashMap::new(),
            materials: Vec::new(),
            shaders,
            material_buffer,
            bindless,
            device,
            physical_device,
            instance,
            command_pool,
            queue,
        })
    }

    pub fn upload_mesh(&mut self, handle: MeshHandle, cpu_mesh: &CpuMesh) -> anyhow::Result<()> {
        match GpuMesh::upload(
            &self.device,
            self.physical_device,
            &self.instance,
            cpu_mesh,
            self.command_pool,
            self.queue,
        ) {
            Ok(gpu) => {
                self.gpu_meshes.insert(handle, GpuMeshState::Ready(gpu));
                Ok(())
            }
            Err(e) => {
                self.gpu_meshes.insert(handle, GpuMeshState::Failed);
                Err(e)
            }
        }
    }

    pub fn upload_texture(
        &mut self,
        handle: TextureHandle,
        pixels: &[u8],
        width: u32,
        height: u32,
        format: vk::Format,
        name: &str,
    ) -> anyhow::Result<()> {
        let tex = GpuTexture::upload(
            &self.device,
            self.physical_device,
            &self.instance,
            self.command_pool,
            self.queue,
            pixels,
            width,
            height,
            format,
            name,
        )?;
        let slot = self.bindless.alloc_slot(tex.view);
        self.texture_slots.insert(handle, slot);
        self.gpu_textures.insert(slot, tex);
        log::debug!("Texture '{}': handle={} -> slot={}", name, handle.0, slot);
        Ok(())
    }

    pub fn texture_slot(&self, handle: TextureHandle) -> u32 {
        self.texture_slots.get(&handle).copied().unwrap_or(BINDLESS_SLOT_WHITE)
    }

    pub fn register_material_gpu(
        &mut self,
        handle: MaterialHandle,
        base_color: [f32; 4],
        metallic: f32,
        roughness: f32,
        emissive: [f32; 4],
        texture_slots: Vec<(TextureSlot, TextureHandle)>,
        _name: String,
    ) {
        let idx = handle.0 as usize;
        if self.materials.len() <= idx {
            self.materials.resize(idx + 1, MaterialData::default_white());
        }
        let mut tex0 = [0u32; 4];
        let mut tex1 = [0u32; 4];
        for (slot, tex_handle) in texture_slots {
            let bindless_slot = self.texture_slot(tex_handle);
            match slot.index() {
                i @ 0..=3 => tex0[i] = bindless_slot,
                i => tex1[i - 4] = bindless_slot,
            }
        }
        self.materials[idx] = MaterialData {
            base_color,
            emissive,
            metallic,
            roughness,
            _pad: [0.0; 2],
            tex_indices0: tex0,
            tex_indices1: tex1,
        };
    }

    pub fn upload_materials_from_render_world(&self, _rw: &RenderWorld) {
        if !self.materials.is_empty() {
            self.material_buffer.upload(&self.materials);
        }
    }

    pub fn get_gpu_mesh(&self, handle: MeshHandle) -> Option<&GpuMesh> {
        match self.gpu_meshes.get(&handle)? {
            GpuMeshState::Ready(gpu) => Some(gpu),
            GpuMeshState::Failed => None,
        }
    }

    pub fn device(&self) -> &ash::Device {
        &self.device
    }

    pub fn is_mesh_ready(&self, handle: MeshHandle) -> bool {
        matches!(self.gpu_meshes.get(&handle), Some(GpuMeshState::Ready(_)))
    }

    pub fn command_pool(&self) -> vk::CommandPool {
        self.command_pool
    }
}
