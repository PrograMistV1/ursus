use crate::assets::cpu_server::TextureHandle;
use crate::assets::material::MaterialData;
use crate::assets::mesh::{CpuMesh, GpuMesh};
use crate::assets::shader_registry::TextureSlot;
use crate::assets::ui::{FontAtlas, DEFAULT_CHARSET, DEFAULT_FONT_SIZES};
use crate::assets::upload::GpuUploadRequest;
use crate::assets::{builtin_shaders, ShaderRegistry};
use crate::ecs::components::{MaterialHandle, MeshHandle};
use crate::render_world::RenderWorld;
use crate::vulkan::{BindlessSet, GpuTexture, MaterialBuffer};
use ash::vk;
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, TryRecvError};

const DEFAULT_FONT_BYTES: &[u8] = include_bytes!("../../../assets/fonts/RobotoMono.ttf");

pub const BINDLESS_SLOT_WHITE: u32 = 0;
pub const BINDLESS_SLOT_FONT_ATLAS: u32 = 1;

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

    pub font_atlas: Option<FontAtlas>,

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
        assert_eq!(bindless.next_slot(), 1);

        let material_buffer = MaterialBuffer::new(&device, physical_device, &instance)?;

        let mut shaders = ShaderRegistry::empty();
        builtin_shaders::register_builtin(&mut shaders);

        let mut font_atlas = FontAtlas::new(DEFAULT_FONT_BYTES, DEFAULT_CHARSET, DEFAULT_FONT_SIZES)
            .map_err(|e| anyhow::anyhow!("Не удалось создать FontAtlas: {}", e))?;

        let font_tex = GpuTexture::upload(
            &device,
            physical_device,
            &instance,
            command_pool,
            queue,
            &font_atlas.pixels,
            font_atlas.atlas_width,
            font_atlas.atlas_height,
            vk::Format::R8G8B8A8_UNORM,
            "font_atlas",
        )?;
        let font_slot = bindless.alloc_slot(font_tex.view);
        assert_eq!(font_slot, BINDLESS_SLOT_FONT_ATLAS);
        font_atlas.dirty = false;

        let mut gpu_textures = HashMap::new();
        gpu_textures.insert(font_slot, font_tex);

        log::info!("GpuAssetServer: white=slot0, font_atlas=slot1, следующий свободный={}", bindless.next_slot());

        Ok(Self {
            gpu_meshes: HashMap::new(),
            texture_slots: HashMap::new(),
            gpu_textures,
            materials: Vec::new(),
            shaders,
            material_buffer,
            bindless,
            font_atlas: Some(font_atlas),
            device,
            physical_device,
            instance,
            command_pool,
            queue,
        })
    }

    pub fn font_atlas_slot(&self) -> u32 {
        BINDLESS_SLOT_FONT_ATLAS
    }

    pub fn flush_font_atlas_if_dirty(&mut self) -> anyhow::Result<()> {
        let dirty = self.font_atlas.as_ref().map(|a| a.dirty).unwrap_or(false);
        if !dirty {
            return Ok(());
        }
        let atlas = self.font_atlas.as_mut().unwrap();
        let new_tex = GpuTexture::upload(
            &self.device,
            self.physical_device,
            &self.instance,
            self.command_pool,
            self.queue,
            &atlas.pixels,
            atlas.atlas_width,
            atlas.atlas_height,
            vk::Format::R8G8B8A8_UNORM,
            "font_atlas",
        )?;
        self.bindless.update_slot(BINDLESS_SLOT_FONT_ATLAS, new_tex.view);
        self.gpu_textures.insert(BINDLESS_SLOT_FONT_ATLAS, new_tex);
        atlas.dirty = false;
        log::debug!("FontAtlas перезалит (слот {})", BINDLESS_SLOT_FONT_ATLAS);
        Ok(())
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
                if let Err(e) = self.upload_texture(handle, &pixels, width, height, format, &name) {
                    log::error!("GPU upload texture failed: {e}");
                }
            }
            GpuUploadRequest::Material { handle, base_color, metallic, roughness, emissive, texture_slots, name } => {
                self.register_material_gpu(handle, base_color, metallic, roughness, emissive, texture_slots, name);
            }
            GpuUploadRequest::FontAtlas { .. } => {}
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
        log::debug!("Текстура '{}': handle={} -> slot={}", name, handle.0, slot);
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

    pub fn is_mesh_ready(&self, handle: MeshHandle) -> bool {
        matches!(self.gpu_meshes.get(&handle), Some(GpuMeshState::Ready(_)))
    }
}
