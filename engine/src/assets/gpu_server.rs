use crate::assets::cpu_server::CpuAssetServer;
use crate::assets::material::MaterialDef;
use crate::assets::mesh::{CpuMesh, GpuMesh};
use crate::assets::pending::PendingUpload;
use crate::assets::ui::FontAtlas;
use crate::assets::TextureHandle;
use crate::components::Transform;
use crate::ecs::components::{MaterialHandle, MeshHandle};
use crate::vulkan::{BindlessSet, GpuTexture, MaterialBuffer};
use ash::vk;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

enum GpuMeshState {
    Ready(GpuMesh),
    Failed,
}

pub struct GpuAssetServer {
    gpu_meshes: HashMap<MeshHandle, GpuMeshState>,
    gpu_textures: Vec<GpuTexture>,
    texture_path_cache: HashMap<PathBuf, TextureHandle>,
    image_index_cache: HashMap<usize, TextureHandle>,

    pub material_buffer: MaterialBuffer,
    pub bindless: BindlessSet,

    pub font_atlas: Option<FontAtlas>,
    pub font_atlas_texture: Option<TextureHandle>,

    upload_queue: Arc<Mutex<Vec<PendingUpload>>>,
    mesh_path_cache: Arc<Mutex<HashMap<PathBuf, Vec<(MeshHandle, Option<MaterialHandle>, Transform)>>>>,

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
        upload_queue: Arc<Mutex<Vec<PendingUpload>>>,
        mesh_path_cache: Arc<Mutex<HashMap<PathBuf, Vec<(MeshHandle, Option<MaterialHandle>, Transform)>>>>,
    ) -> anyhow::Result<Self> {
        let bindless = BindlessSet::new(&device, physical_device, &instance, command_pool, queue)?;
        let material_buffer = MaterialBuffer::new(&device, physical_device, &instance)?;

        Ok(Self {
            gpu_meshes: HashMap::new(),
            gpu_textures: Vec::new(),
            texture_path_cache: HashMap::new(),
            image_index_cache: HashMap::new(),
            material_buffer,
            bindless,
            font_atlas: None,
            font_atlas_texture: None,
            upload_queue,
            mesh_path_cache,
            device,
            physical_device,
            instance,
            command_pool,
            queue,
        })
    }

    pub fn flush_uploads(&mut self, cpu: &mut CpuAssetServer) -> anyhow::Result<()> {
        let pending: Vec<PendingUpload> = {
            let mut q = self.upload_queue.lock().unwrap();
            std::mem::take(&mut *q)
        };

        for upload in pending {
            match upload {
                PendingUpload::Mesh { path, meshes } => {
                    let mut instances = Vec::new();
                    self.image_index_cache.clear();

                    for pending_mesh in meshes {
                        let handle = MeshHandle(cpu.cpu_meshes.len() as u32);
                        cpu.cpu_meshes.push(pending_mesh.cpu_mesh.clone());

                        match GpuMesh::upload(
                            &self.device,
                            self.physical_device,
                            &self.instance,
                            &pending_mesh.cpu_mesh,
                            self.command_pool,
                            self.queue,
                        ) {
                            Ok(gpu) => {
                                self.gpu_meshes.insert(handle, GpuMeshState::Ready(gpu));
                            }
                            Err(e) => {
                                log::error!("GPU upload меша failed: {e}");
                                self.gpu_meshes.insert(handle, GpuMeshState::Failed);
                            }
                        }

                        let mat_handle = if let Some(mat) = pending_mesh.material {
                            let tex_handles = self.upload_pending_textures(&mat.textures)?;
                            let mut mat_def = MaterialDef::new(&mat.name, cpu.shaders.by_name("diffuse").unwrap())
                                .with_color(mat.base_color[0], mat.base_color[1], mat.base_color[2], mat.base_color[3])
                                .with_metallic(mat.metallic)
                                .with_roughness(mat.roughness);
                            for (slot, tex_handle) in tex_handles {
                                mat_def.set_texture(slot, tex_handle);
                            }
                            Some(cpu.register_material(mat_def))
                        } else {
                            None
                        };

                        instances.push((handle, mat_handle, pending_mesh.transform));
                    }

                    self.mesh_path_cache.lock().unwrap().insert(path, instances);
                }

                PendingUpload::Texture { path, pixels, width, height, format, name } => {
                    match self.upload_texture_raw(&pixels, width, height, format, &name) {
                        Ok(handle) => {
                            self.texture_path_cache.insert(path, handle);
                        }
                        Err(e) => log::error!("GPU upload текстуры failed: {e}"),
                    }
                }
            }
        }

        Ok(())
    }

    fn upload_pending_textures(
        &mut self,
        textures: &[crate::assets::pending::PendingTexture],
    ) -> anyhow::Result<Vec<(crate::assets::shader_registry::TextureSlot, TextureHandle)>> {
        let mut result = Vec::new();
        for tex in textures {
            let handle = if let Some(&cached) = self.image_index_cache.get(&tex.image_index) {
                cached
            } else {
                let h = self.upload_texture_raw(&tex.pixels, tex.width, tex.height, tex.format, &tex.name)?;
                self.image_index_cache.insert(tex.image_index, h);
                h
            };
            result.push((tex.slot, handle));
        }
        Ok(result)
    }

    pub fn upload_texture_raw(
        &mut self,
        pixels: &[u8],
        width: u32,
        height: u32,
        format: vk::Format,
        name: impl Into<String>,
    ) -> anyhow::Result<TextureHandle> {
        let name = name.into();
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
            &name,
        )?;
        let slot = self.bindless.register_view(tex.view);
        self.gpu_textures.push(tex);
        Ok(TextureHandle(slot))
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

    pub fn upload_font_atlas(&mut self, atlas: FontAtlas) -> anyhow::Result<()> {
        let handle = self.upload_texture_raw(
            &atlas.pixels,
            atlas.atlas_width,
            atlas.atlas_height,
            vk::Format::R8G8B8A8_UNORM,
            "font_atlas",
        )?;
        self.font_atlas_texture = Some(handle);
        self.font_atlas = Some(atlas);
        Ok(())
    }

    pub fn get_gpu_mesh(&self, handle: MeshHandle) -> Option<&GpuMesh> {
        match self.gpu_meshes.get(&handle)? {
            GpuMeshState::Ready(gpu) => Some(gpu),
            GpuMeshState::Failed => None,
        }
    }

    pub fn upload_materials(&self, cpu: &CpuAssetServer) {
        let data: Vec<_> = cpu.cpu_materials.iter().map(|m| m.to_gpu_data()).collect();
        if !data.is_empty() {
            self.material_buffer.upload(&data);
        }
    }

    pub fn is_mesh_ready(&self, handle: MeshHandle) -> bool {
        matches!(self.gpu_meshes.get(&handle), Some(GpuMeshState::Ready(_)))
    }
}
