use crate::assets::cpu_server::TextureHandle;
use crate::assets::material::MaterialData;
use crate::assets::mesh::{CpuMesh, GpuMesh};
use crate::assets::shader_registry::TextureSlot;
use crate::assets::ui::FontAtlas;
use crate::assets::upload::GpuUploadRequest;
use crate::ecs::components::{MaterialHandle, MeshHandle};
use crate::render_world::RenderWorld;
use crate::vulkan::{BindlessSet, GpuTexture, MaterialBuffer};
use ash::vk;
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, TryRecvError};

enum GpuMeshState {
    Ready(GpuMesh),
    Failed,
}

pub struct GpuAssetServer {
    gpu_meshes: HashMap<MeshHandle, GpuMeshState>,
    gpu_textures: HashMap<TextureHandle, GpuTexture>,
    materials: Vec<MaterialData>,

    pub shaders: crate::assets::shader_registry::ShaderRegistry,

    pub material_buffer: MaterialBuffer,
    pub bindless: BindlessSet,

    pub font_atlas: Option<FontAtlas>,
    pub font_atlas_texture: Option<TextureHandle>,

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
        let bindless = BindlessSet::new(&device, physical_device, &instance, command_pool, queue)?;
        let material_buffer = MaterialBuffer::new(&device, physical_device, &instance)?;

        Ok(Self {
            gpu_meshes: HashMap::new(),
            gpu_textures: HashMap::new(),
            materials: Vec::new(),
            material_buffer,
            bindless,
            font_atlas: None,
            font_atlas_texture: None,
            device,
            physical_device,
            instance,
            command_pool,
            queue,
        })
    }

    pub fn flush_uploads_gpu(&mut self, rx: &Receiver<GpuUploadRequest>) -> anyhow::Result<()> {
        loop {
            match rx.try_recv() {
                Ok(req) => self.apply_upload(req)?,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }
        Ok(())
    }

    fn apply_upload(&mut self, req: GpuUploadRequest) -> anyhow::Result<()> {
        match req {
            GpuUploadRequest::Mesh { handle, vertices, indices, name } => {
                let cpu = CpuMesh::new(name, vertices, indices);
                if let Err(e) = self.upload_mesh(handle, &cpu) {
                    log::error!("GPU upload mesh failed: {e}");
                }
            }
            GpuUploadRequest::Texture { handle, pixels, width, height, format, name } => {
                if let Err(e) = self.upload_texture_at(handle, &pixels, width, height, format, &name) {
                    log::error!("GPU upload texture failed: {e}");
                }
            }
            GpuUploadRequest::Material { handle, base_color, metallic, roughness, emissive, texture_slots, name } => {
                self.register_material_gpu(handle, base_color, metallic, roughness, emissive, texture_slots, name);
            }
            GpuUploadRequest::FontAtlas { pixels, width, height } => {
                if let Err(e) = self.upload_font_atlas_raw(pixels, width, height) {
                    log::error!("GPU upload font atlas failed: {e}");
                }
            }
        }
        Ok(())
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

    pub fn upload_texture_at(
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
        // Слот в bindless-массиве должен совпадать с TextureHandle, выданным
        // на CPU-стороне (material_id ссылается на него до того, как GPU upload завершится).
        self.bindless.register_view_at(handle.0, tex.view);
        self.gpu_textures.insert(handle, tex);
        Ok(())
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
        for (slot, tex) in texture_slots {
            match slot.index() {
                i @ 0..=3 => tex0[i] = tex.0,
                _ => tex1[0] = tex.0,
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

    pub fn upload_font_atlas_raw(&mut self, pixels: Vec<u8>, width: u32, height: u32) -> anyhow::Result<()> {
        // TODO: хэндл шрифтового атласа должен выделяться на CPU-стороне так же,
        // как текстуры материалов, и приходить как GpuUploadRequest::FontAtlas{handle,...}.
        // Сейчас оставлено как заглушка под единственный font atlas без явного handle.
        let tex = GpuTexture::upload(
            &self.device,
            self.physical_device,
            &self.instance,
            self.command_pool,
            self.queue,
            &pixels,
            width,
            height,
            vk::Format::R8G8B8A8_UNORM,
            "font_atlas",
        )?;
        let slot = self.bindless.register_view(tex.view);
        self.font_atlas_texture = Some(TextureHandle(slot));
        self.gpu_textures.insert(TextureHandle(slot), tex);
        Ok(())
    }

    pub fn get_gpu_mesh(&self, handle: MeshHandle) -> Option<&GpuMesh> {
        match self.gpu_meshes.get(&handle)? {
            GpuMeshState::Ready(gpu) => Some(gpu),
            GpuMeshState::Failed => None,
        }
    }

    pub fn is_mesh_ready(&self, handle: MeshHandle) -> bool {
        matches!(self.gpu_meshes.get(&handle), Some(GpuMeshState::Ready(_)))
    }
}
