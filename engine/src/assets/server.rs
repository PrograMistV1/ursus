use ash::vk;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::loaders;
use super::material::MaterialDef;
use super::mesh::{CpuMesh, GpuMesh};
use crate::ecs::components::{MaterialHandle, MeshHandle};

pub struct AssetServer {
    cpu_meshes: Vec<CpuMesh>,
    cpu_materials: Vec<MaterialDef>,

    gpu_meshes: Vec<Option<GpuMesh>>,

    mesh_path_cache: HashMap<PathBuf, MeshHandle>,
    material_name_cache: HashMap<String, MaterialHandle>,

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
    ) -> Self {
        let mut server = Self {
            cpu_meshes: Vec::new(),
            cpu_materials: Vec::new(),
            gpu_meshes: Vec::new(),
            mesh_path_cache: HashMap::new(),
            material_name_cache: HashMap::new(),
            device,
            physical_device,
            instance,
            command_pool,
            queue,
        };

        server.register_mesh(CpuMesh::triangle());
        server.register_mesh(CpuMesh::cube());
        server.register_mesh(CpuMesh::plane(10.0, 10));

        server
    }

    pub fn load_mesh(&mut self, path: impl AsRef<Path>) -> anyhow::Result<MeshHandle> {
        let path = path.as_ref();
        println!("Loading mesh: {}", path.display());
        let canonical = path.to_path_buf();

        if let Some(&handle) = self.mesh_path_cache.get(&canonical) {
            return Ok(handle);
        }

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let cpu_mesh = match ext.as_str() {
            "obj" => loaders::load_obj(path)?,
            "gltf" | "glb" => {
                let meshes = loaders::load_gltf(path)?;
                let mut first = None;
                for (i, m) in meshes.into_iter().enumerate() {
                    let h = self.register_mesh(m);
                    if i == 0 {
                        first = Some(h);
                    }
                }
                let handle = first.unwrap();
                self.mesh_path_cache.insert(canonical, handle);
                return Ok(handle);
            }
            _ => anyhow::bail!("Неизвестный формат меша: {:?}", path),
        };

        let handle = self.register_mesh(cpu_mesh);
        self.mesh_path_cache.insert(canonical, handle);
        Ok(handle)
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

    pub fn register_material(&mut self, mat: MaterialDef) -> MaterialHandle {
        let name = mat.name.clone();
        let id = self.cpu_materials.len() as u32;
        self.cpu_materials.push(mat);
        let handle = MaterialHandle(id);
        self.material_name_cache.insert(name, handle);
        handle
    }

    pub fn material_from_shader(&mut self, shader: impl Into<String>) -> MaterialHandle {
        let shader = shader.into();
        if let Some(&h) = self.material_name_cache.get(&shader) {
            return h;
        }
        self.register_material(MaterialDef::new(&shader).with_shader(&shader))
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
        let gpu = GpuMesh::upload(
            &self.device,
            self.physical_device,
            &self.instance,
            cpu,
            self.command_pool,
            self.queue,
        )?;
        self.gpu_meshes[handle.0 as usize] = Some(gpu);
        Ok(())
    }

    pub fn get_cpu_mesh(&self, handle: MeshHandle) -> Option<&CpuMesh> {
        self.cpu_meshes.get(handle.0 as usize)
    }

    pub fn get_gpu_mesh(&self, handle: MeshHandle) -> Option<&GpuMesh> {
        self.gpu_meshes.get(handle.0 as usize)?.as_ref()
    }

    pub fn get_material(&self, handle: MaterialHandle) -> Option<&MaterialDef> {
        self.cpu_materials.get(handle.0 as usize)
    }

    pub fn mesh_count(&self) -> usize {
        self.cpu_meshes.len()
    }
    pub fn material_count(&self) -> usize {
        self.cpu_materials.len()
    }
}
