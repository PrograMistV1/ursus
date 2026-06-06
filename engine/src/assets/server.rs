use super::loader_job::{BackgroundLoader, LoaderMessage, MeshSource, TextureSource};
use super::loaders;
use super::material::MaterialDef;
use super::mesh::{CpuMesh, GpuMesh};
use super::shader_registry::{ShaderHandle, ShaderRegistry};
use crate::components::Transform;
use crate::ecs::components::{MaterialHandle, MeshHandle};
use crate::vulkan::MaterialBuffer;
use crate::vulkan::{BindlessSet, GpuTexture};
use ash::vk;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureHandle(pub u32);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AsyncMeshHandle(pub PathBuf);

impl AsyncMeshHandle {
    pub fn path(&self) -> &Path {
        &self.0
    }
}

#[derive(Debug, Clone, Default)]
pub struct LoadProgress {
    pub total: usize,
    pub completed: usize,
    pub current: String,
}

impl LoadProgress {
    pub fn fraction(&self) -> f32 {
        if self.total == 0 {
            1.0
        } else {
            self.completed as f32 / self.total as f32
        }
    }

    pub fn is_done(&self) -> bool {
        self.total == 0 || self.completed >= self.total
    }
}

enum GpuMeshState {
    Pending,
    Ready(GpuMesh),
    Failed,
}

pub struct AssetServer {
    cpu_meshes: Vec<CpuMesh>,
    gpu_meshes: Vec<GpuMeshState>,

    mesh_path_cache: HashMap<PathBuf, Vec<(MeshHandle, Option<MaterialHandle>, Transform)>>,

    cpu_materials: Vec<MaterialDef>,
    material_name_cache: HashMap<String, MaterialHandle>,

    gpu_textures: Vec<GpuTexture>,
    texture_path_cache: HashMap<PathBuf, TextureHandle>,

    pub material_buffer: MaterialBuffer,
    pub shaders: ShaderRegistry,
    pub bindless: BindlessSet,

    loader: BackgroundLoader,
    pending_paths: HashMap<PathBuf, PendingKind>,
    pub load_progress: LoadProgress,

    device: ash::Device,
    physical_device: vk::PhysicalDevice,
    instance: ash::Instance,
    command_pool: vk::CommandPool,
    queue: vk::Queue,
}

#[derive(Debug, Clone, Copy)]
enum PendingKind {
    Mesh,
    Texture,
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
            loader: BackgroundLoader::new(),
            pending_paths: HashMap::new(),
            load_progress: LoadProgress::default(),
            device,
            physical_device,
            instance,
            command_pool,
            queue,
        };

        let tri = server.register_mesh(CpuMesh::triangle());
        let cube = server.register_mesh(CpuMesh::cube());
        let plane = server.register_mesh(CpuMesh::plane(10.0, 10));
        server.upload_mesh(tri)?;
        server.upload_mesh(cube)?;
        server.upload_mesh(plane)?;

        Ok(server)
    }

    pub fn load_mesh_async(&mut self, path: impl AsRef<Path>) -> AsyncMeshHandle {
        let path = path.as_ref().to_path_buf();

        if self.mesh_path_cache.contains_key(&path) {
            return AsyncMeshHandle(path);
        }

        if self.pending_paths.contains_key(&path) {
            return AsyncMeshHandle(path);
        }

        log::info!("load_mesh_async: {:?}", path);
        self.loader.request_mesh(path.clone());
        self.pending_paths.insert(path.clone(), PendingKind::Mesh);
        self.load_progress.total += 1;
        self.load_progress.current = path.to_string_lossy().to_string();

        AsyncMeshHandle(path)
    }

    pub fn load_texture_async(&mut self, path: impl AsRef<Path>) -> TextureHandle {
        let path = path.as_ref().to_path_buf();

        if let Some(&handle) = self.texture_path_cache.get(&path) {
            return handle;
        }

        if self.pending_paths.contains_key(&path) {
            return TextureHandle(0);
        }

        log::info!("load_texture_async: {:?}", path);
        self.loader.request_texture(path.clone());
        self.pending_paths
            .insert(path.clone(), PendingKind::Texture);
        self.load_progress.total += 1;
        self.load_progress.current = path.to_string_lossy().to_string();

        TextureHandle(0)
    }

    pub fn get_mesh_instances(
        &self,
        handle: &AsyncMeshHandle,
    ) -> Option<&Vec<(MeshHandle, Option<MaterialHandle>, Transform)>> {
        self.mesh_path_cache.get(&handle.0)
    }

    pub fn poll_loader(&mut self) {
        loop {
            match self.loader.msg_rx.try_recv() {
                Ok(msg) => self.apply_message(msg),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    log::warn!("asset-loader thread отключился");
                    break;
                }
            }
        }
    }

    pub fn is_loading(&self) -> bool {
        !self.load_progress.is_done()
    }

    fn apply_message(&mut self, msg: LoaderMessage) {
        match msg {
            LoaderMessage::MeshReady { path, source } => {
                self.load_progress.current = path.to_string_lossy().to_string();
                match self.apply_mesh_source(source) {
                    Ok(instances) => {
                        self.mesh_path_cache.insert(path.clone(), instances);
                    }
                    Err(e) => {
                        log::error!("Ошибка GPU-загрузки меша {:?}: {}", path, e);
                    }
                }
                self.pending_paths.remove(&path);
                self.load_progress.completed += 1;
                log::info!(
                    "Меш готов: {:?} ({}/{})",
                    path,
                    self.load_progress.completed,
                    self.load_progress.total,
                );
            }

            LoaderMessage::TextureReady { path, source } => {
                self.load_progress.current = path.to_string_lossy().to_string();
                match self.apply_texture_source(&path, source) {
                    Ok(_) => {}
                    Err(e) => {
                        log::error!("Ошибка GPU-загрузки текстуры {:?}: {}", path, e);
                    }
                }
                self.pending_paths.remove(&path);
                self.load_progress.completed += 1;
                log::info!(
                    "Текстура готова: {:?} ({}/{})",
                    path,
                    self.load_progress.completed,
                    self.load_progress.total,
                );
            }

            LoaderMessage::Error { path, error } => {
                log::error!("Ошибка загрузки {:?}: {}", path, error);
                self.pending_paths.remove(&path);
                self.load_progress.completed += 1;
            }
        }
    }

    fn apply_mesh_source(
        &mut self,
        source: MeshSource,
    ) -> anyhow::Result<Vec<(MeshHandle, Option<MaterialHandle>, Transform)>> {
        let mut image_cache: HashMap<usize, TextureHandle> = HashMap::new();
        let mut instances = Vec::new();

        for primitive in source.primitives {
            let mut mat_def = primitive.material.map(|m| {
                MaterialDef::new(&m.name, self.shaders.diffuse())
                    .with_color(
                        m.base_color[0],
                        m.base_color[1],
                        m.base_color[2],
                        m.base_color[3],
                    )
                    .with_metallic(m.metallic)
                    .with_roughness(m.roughness)
            });

            for (slot, bytes, width, height, name, image_index) in primitive.textures {
                let handle = if let Some(&cached) = image_cache.get(&image_index) {
                    cached
                } else {
                    match self.upload_texture_raw(
                        &bytes,
                        width,
                        height,
                        vk::Format::R8G8B8A8_SRGB,
                        &name,
                    ) {
                        Ok(h) => {
                            image_cache.insert(image_index, h);
                            h
                        }
                        Err(e) => {
                            log::error!("Ошибка загрузки текстуры '{}': {}", name, e);
                            continue;
                        }
                    }
                };
                if let Some(ref mut mat) = mat_def {
                    mat.set_texture(slot, handle);
                }
            }

            let transform = Transform {
                position: glam::Vec3::from(primitive.node_translation),
                rotation: glam::Quat::from_array(primitive.node_rotation),
                scale: glam::Vec3::from(primitive.node_scale),
            };

            let mh = self.register_mesh(primitive.mesh);
            self.upload_mesh(mh)?;
            let mat_handle = mat_def.map(|mat| self.register_material(mat));
            instances.push((mh, mat_handle, transform));
        }

        Ok(instances)
    }

    fn apply_texture_source(
        &mut self,
        path: &Path,
        source: TextureSource,
    ) -> anyhow::Result<TextureHandle> {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let handle = self.upload_texture_raw(
            &source.pixels,
            source.width,
            source.height,
            vk::Format::R8G8B8A8_SRGB,
            &name,
        )?;

        self.texture_path_cache.insert(path.to_path_buf(), handle);
        Ok(handle)
    }

    pub fn upload_materials(&self) {
        let data: Vec<_> = self.cpu_materials.iter().map(|m| m.to_gpu_data()).collect();
        if !data.is_empty() {
            self.material_buffer.upload(&data);
        }
    }

    pub fn material_buffer_set(&self) -> vk::DescriptorSet {
        self.material_buffer.set
    }

    pub fn load_mesh(
        &mut self,
        path: impl AsRef<Path>,
    ) -> anyhow::Result<Vec<(MeshHandle, Option<MaterialHandle>, Transform)>> {
        let path = path.as_ref();
        let canonical = path.to_path_buf();

        if let Some(cached) = self.mesh_path_cache.get(&canonical) {
            return Ok(cached.clone());
        }

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        match ext.as_str() {
            "obj" => {
                let mesh = loaders::load_obj(path)?;
                let handle = self.register_mesh(mesh);
                self.upload_mesh(handle)?;
                let result = vec![(handle, None, Transform::identity())];
                self.mesh_path_cache.insert(canonical, result.clone());
                Ok(result)
            }
            "gltf" | "glb" => {
                let result = loaders::load_gltf(path)?;
                let mut all = Vec::new();
                let mut image_cache: HashMap<usize, TextureHandle> = HashMap::new();

                for primitive in result.into_iter() {
                    let mut mat_def = primitive.material.map(|m| {
                        MaterialDef::new(&m.name, self.shaders.diffuse())
                            .with_color(
                                m.base_color[0],
                                m.base_color[1],
                                m.base_color[2],
                                m.base_color[3],
                            )
                            .with_metallic(m.metallic)
                            .with_roughness(m.roughness)
                    });

                    for (slot, bytes, width, height, name, image_index) in primitive.textures {
                        let handle = if let Some(&cached) = image_cache.get(&image_index) {
                            cached
                        } else {
                            match self.upload_texture_raw(
                                &bytes,
                                width,
                                height,
                                vk::Format::R8G8B8A8_SRGB,
                                &name,
                            ) {
                                Ok(h) => {
                                    image_cache.insert(image_index, h);
                                    h
                                }
                                Err(e) => {
                                    log::error!("Ошибка загрузки текстуры '{}': {}", name, e);
                                    continue;
                                }
                            }
                        };
                        if let Some(ref mut mat) = mat_def {
                            mat.set_texture(slot, handle);
                        }
                    }

                    let node_transform = Transform {
                        position: glam::Vec3::from(primitive.node_translation),
                        rotation: glam::Quat::from_array(primitive.node_rotation),
                        scale: glam::Vec3::from(primitive.node_scale),
                    };

                    let mh = self.register_mesh(primitive.mesh);
                    self.upload_mesh(mh)?;
                    let mat_handle = mat_def.map(|mat| self.register_material(mat));
                    all.push((mh, mat_handle, node_transform));
                }

                self.mesh_path_cache.insert(canonical, all.clone());
                Ok(all)
            }
            _ => anyhow::bail!("Неизвестный формат меша: {:?}", path),
        }
    }

    pub fn register_mesh(&mut self, mesh: CpuMesh) -> MeshHandle {
        let id = self.cpu_meshes.len() as u32;
        self.cpu_meshes.push(mesh);
        self.gpu_meshes.push(GpuMeshState::Pending);
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

        let img = image::open(path)
            .map_err(|e| anyhow::anyhow!("Не удалось загрузить текстуру {:?}: {}", path, e))?
            .into_rgba8();
        let (width, height) = img.dimensions();
        let pixels = img.into_raw();

        let handle = self.upload_texture_raw(
            &pixels,
            width,
            height,
            vk::Format::R8G8B8A8_SRGB,
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .as_ref(),
        )?;

        self.texture_path_cache.insert(canonical, handle);
        Ok(handle)
    }

    pub fn load_texture_bytes(
        &mut self,
        data: &[u8],
        name: impl Into<String>,
    ) -> anyhow::Result<TextureHandle> {
        let name = name.into();
        let img = image::load_from_memory(data)
            .map_err(|e| anyhow::anyhow!("Не удалось загрузить текстуру '{}': {}", name, e))?
            .into_rgba8();
        let (width, height) = img.dimensions();
        let pixels = img.into_raw();
        self.upload_texture_raw(&pixels, width, height, vk::Format::R8G8B8A8_SRGB, name)
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

    pub fn create_material(
        &mut self,
        name: impl Into<String>,
        shader: ShaderHandle,
    ) -> MaterialHandle {
        self.register_material(MaterialDef::new(name, shader))
    }

    pub fn upload_all_meshes(&mut self) -> anyhow::Result<()> {
        for i in 0..self.cpu_meshes.len() {
            if matches!(self.gpu_meshes[i], GpuMeshState::Pending) {
                self.upload_mesh(MeshHandle(i as u32))?;
            }
        }
        Ok(())
    }

    pub fn upload_mesh(&mut self, handle: MeshHandle) -> anyhow::Result<()> {
        let cpu = &self.cpu_meshes[handle.0 as usize];
        match GpuMesh::upload(
            &self.device,
            self.physical_device,
            &self.instance,
            cpu,
            self.command_pool,
            self.queue,
        ) {
            Ok(gpu) => {
                self.gpu_meshes[handle.0 as usize] = GpuMeshState::Ready(gpu);
                Ok(())
            }
            Err(e) => {
                self.gpu_meshes[handle.0 as usize] = GpuMeshState::Failed;
                Err(e)
            }
        }
    }

    pub fn get_cpu_mesh(&self, handle: MeshHandle) -> Option<&CpuMesh> {
        self.cpu_meshes.get(handle.0 as usize)
    }

    pub fn get_gpu_mesh(&self, handle: MeshHandle) -> Option<&GpuMesh> {
        match self.gpu_meshes.get(handle.0 as usize)? {
            GpuMeshState::Ready(gpu) => Some(gpu),
            _ => None,
        }
    }

    pub fn is_mesh_ready(&self, handle: MeshHandle) -> bool {
        matches!(
            self.gpu_meshes.get(handle.0 as usize),
            Some(GpuMeshState::Ready(_))
        )
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
