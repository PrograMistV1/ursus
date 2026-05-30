use ash::vk;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::loaders;
use super::material::MaterialDef;
use super::mesh::{CpuMesh, GpuMesh};
use super::shader_registry::{ShaderHandle, ShaderRegistry};
use crate::ecs::components::{MaterialHandle, MeshHandle};
use crate::vulkan::MaterialBuffer;
use crate::vulkan::{BindlessSet, GpuTexture};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureHandle(pub u32);

pub struct AssetServer {
    cpu_meshes: Vec<CpuMesh>,
    gpu_meshes: Vec<Option<GpuMesh>>,
    mesh_path_cache: HashMap<PathBuf, MeshHandle>,

    cpu_materials: Vec<MaterialDef>,
    material_name_cache: HashMap<String, MaterialHandle>,

    gpu_textures: Vec<GpuTexture>,
    texture_path_cache: HashMap<PathBuf, TextureHandle>,

    pub material_buffer: MaterialBuffer,

    pub shaders: ShaderRegistry,

    pub bindless: BindlessSet,

    device: ash::Device,
    physical_device: vk::PhysicalDevice,
    instance: ash::Instance,
    command_pool: vk::CommandPool,
    queue: vk::Queue,
}

impl AssetServer {
    pub fn new(
        device: ash::Device,
        physical_device: vk::PhysicalDevice,
        instance: ash::Instance,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
    ) -> anyhow::Result<Self> {
        let bindless = BindlessSet::new(&device, physical_device, &instance, command_pool, queue)?;
        let material_buffer = MaterialBuffer::new(&device, physical_device, &instance)?;

        let mut server = Self {
            cpu_meshes: Vec::new(),
            gpu_meshes: Vec::new(),
            mesh_path_cache: HashMap::new(),
            cpu_materials: Vec::new(),
            material_name_cache: HashMap::new(),
            gpu_textures: Vec::new(),
            texture_path_cache: HashMap::new(),
            shaders: ShaderRegistry::new(),
            bindless,
            material_buffer,
            device,
            physical_device,
            instance,
            command_pool,
            queue,
        };

        server.register_mesh(CpuMesh::triangle());
        server.register_mesh(CpuMesh::cube());
        server.register_mesh(CpuMesh::plane(10.0, 10));

        Ok(server)
    }

    pub fn upload_materials(&self) {
        let data: Vec<_> = self.cpu_materials.iter()
            .map(|m| m.to_gpu_data())
            .collect();
        if !data.is_empty() {
            self.material_buffer.upload(&data);
        }
    }

    pub fn material_buffer_set(&self) -> vk::DescriptorSet {
        self.material_buffer.set
    }

    pub fn load_mesh(&mut self, path: impl AsRef<Path>) -> anyhow::Result<Vec<(MeshHandle, Option<MaterialHandle>)>> {
        let path = path.as_ref();
        let canonical = path.to_path_buf();

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();

        match ext.as_str() {
            "obj" => {
                let mesh = loaders::load_obj(path)?;
                let handle = self.register_mesh(mesh);
                Ok(vec![(handle, None)])  // один элемент
            }
            "gltf" | "glb" => {
                let result = loaders::load_gltf(path)?;
                let mut all = Vec::new();

                for primitive in result.into_iter() {
                    let mut mat_def = primitive.material.map(|m| {
                        MaterialDef::new(&m.name, self.shaders.diffuse()).with_color(
                            m.base_color[0],
                            m.base_color[1],
                            m.base_color[2],
                            m.base_color[3],
                        ).with_metallic(m.metallic).with_roughness(m.roughness)
                    });

                    for (slot, bytes, width, height, name) in primitive.textures {
                        match self.upload_texture_raw(&bytes, width, height, vk::Format::R8G8B8A8_SRGB, &name) {
                            Ok(handle) => {
                                if let Some(ref mut mat) = mat_def {
                                    mat.set_texture(slot, handle);
                                }
                            }
                            Err(e) => log::error!("Ошибка загрузки текстуры '{}': {}", name, e),
                        }
                    }

                    let mh = self.register_mesh(primitive.mesh);
                    let mat_handle = mat_def.map(|mat| self.register_material(mat));
                    all.push((mh, mat_handle));
                }

                Ok(all)
            }
            _ => anyhow::bail!("Неизвестный формат меша: {:?}", path),
        }
    }

    pub fn register_mesh(&mut self, mesh: CpuMesh) -> MeshHandle {
        let id = self.cpu_meshes.len() as u32;
        self.cpu_meshes.push(mesh);
        self.gpu_meshes.push(None);
        MeshHandle(id)
    }

    pub fn mesh_triangle(&self) -> MeshHandle {
        MeshHandle(0)
    }
    pub fn mesh_cube(&self) -> MeshHandle {
        MeshHandle(1)
    }
    pub fn mesh_plane(&self) -> MeshHandle {
        MeshHandle(2)
    }

    pub fn load_texture(&mut self, path: impl AsRef<Path>) -> anyhow::Result<TextureHandle> {
        let path = path.as_ref();
        let canonical = path.to_path_buf();

        if let Some(&handle) = self.texture_path_cache.get(&canonical) {
            return Ok(handle);
        }

        let img = image::open(path).map_err(|e| anyhow::anyhow!("Не удалось загрузить текстуру {:?}: {}", path, e))?.into_rgba8();

        let (width, height) = img.dimensions();
        let pixels = img.into_raw();

        let handle = self.upload_texture_raw(
            &pixels,
            width,
            height,
            vk::Format::R8G8B8A8_SRGB,
            path.file_name().unwrap_or_default().to_string_lossy().as_ref(),
        )?;

        self.texture_path_cache.insert(canonical, handle);
        Ok(handle)
    }

    pub fn load_texture_bytes(&mut self, data: &[u8], name: impl Into<String>) -> anyhow::Result<TextureHandle> {
        let name = name.into();
        let img = image::load_from_memory(data)
            .map_err(|e| anyhow::anyhow!("Не удалось загрузить текстуру '{}': {}", name, e))?
            .into_rgba8();
        let (width, height) = img.dimensions();
        let pixels = img.into_raw();
        self.upload_texture_raw(&pixels, width, height, vk::Format::R8G8B8A8_SRGB, name)
    }
    pub fn upload_texture_raw(&mut self, pixels: &[u8], width: u32, height: u32, format: vk::Format, name: impl Into<String>) -> anyhow::Result<TextureHandle> {
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

    pub fn get_texture(&self, handle: TextureHandle) -> Option<&GpuTexture> {
        let idx = handle.0 as usize;
        if idx == 0 {
            return None;
        }
        self.gpu_textures.get(idx - 1)
    }

    pub fn register_material(&mut self, mat: MaterialDef) -> MaterialHandle {
        let name = mat.name.clone();
        let id = self.cpu_materials.len() as u32;
        self.cpu_materials.push(mat);
        let handle = MaterialHandle(id);
        self.material_name_cache.insert(name, handle);
        handle
    }

    pub fn get_material(&self, handle: MaterialHandle) -> Option<&MaterialDef> {
        self.cpu_materials.get(handle.0 as usize)
    }
    pub fn create_material(&mut self, name: impl Into<String>, shader: ShaderHandle) -> MaterialHandle {
        self.register_material(MaterialDef::new(name, shader))
    }

    pub fn upload_all_meshes(&mut self) -> anyhow::Result<()> {
        for i in 0..self.cpu_meshes.len() {
            if self.gpu_meshes[i].is_none() {
                self.upload_mesh(MeshHandle(i as u32))?;
            }
        }
        Ok(())
    }

    pub fn upload_mesh(&mut self, handle: MeshHandle) -> anyhow::Result<()> {
        let cpu = &self.cpu_meshes[handle.0 as usize];
        let gpu = GpuMesh::upload(&self.device, self.physical_device, &self.instance, cpu, self.command_pool, self.queue)?;
        self.gpu_meshes[handle.0 as usize] = Some(gpu);
        Ok(())
    }

    pub fn get_cpu_mesh(&self, handle: MeshHandle) -> Option<&CpuMesh> {
        self.cpu_meshes.get(handle.0 as usize)
    }

    pub fn get_gpu_mesh(&self, handle: MeshHandle) -> Option<&GpuMesh> {
        self.gpu_meshes.get(handle.0 as usize)?.as_ref()
    }

    pub fn mesh_count(&self) -> usize {
        self.cpu_meshes.len()
    }
    pub fn material_count(&self) -> usize {
        self.cpu_materials.len()
    }
    pub fn texture_count(&self) -> usize {
        self.gpu_textures.len()
    }
}
