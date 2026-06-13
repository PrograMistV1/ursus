use crate::assets::loader_job::{BackgroundLoader, LoaderMessage, MeshSource};
use crate::assets::material::MaterialDef;
use crate::assets::mesh::CpuMesh;
use crate::assets::pending::{PendingMaterial, PendingMesh, PendingTexture, PendingUpload};
use crate::assets::shader_registry::{ShaderRegistry, TextureSlot};
use crate::components::Transform;
use crate::ecs::components::{MaterialHandle, MeshHandle};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Default)]
pub struct LoadProgress {
    pub total: usize,
    pub completed: usize,
    pub current: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureHandle(pub u32);

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

pub struct CpuAssetServer {
    pub cpu_meshes: Vec<CpuMesh>,
    pub cpu_materials: Vec<MaterialDef>,
    pub material_name_cache: HashMap<String, MaterialHandle>,

    pub mesh_path_cache: Arc<Mutex<HashMap<PathBuf, Vec<(MeshHandle, Option<MaterialHandle>, Transform)>>>>,

    pub shaders: ShaderRegistry,
    pub load_progress: LoadProgress,

    pub upload_queue: Arc<Mutex<Vec<PendingUpload>>>,

    loader: BackgroundLoader,
    pending_paths: HashMap<PathBuf, ()>,
}

impl CpuAssetServer {
    pub fn new(upload_queue: Arc<Mutex<Vec<PendingUpload>>>) -> Self {
        Self {
            cpu_meshes: Vec::new(),
            cpu_materials: Vec::new(),
            material_name_cache: HashMap::new(),
            mesh_path_cache: Arc::new(Mutex::new(HashMap::new())),
            shaders: ShaderRegistry::new(),
            load_progress: LoadProgress::default(),
            upload_queue,
            loader: BackgroundLoader::new(),
            pending_paths: HashMap::new(),
        }
    }

    pub fn register_mesh(&mut self, mesh: CpuMesh) -> MeshHandle {
        let id = self.cpu_meshes.len() as u32;
        self.cpu_meshes.push(mesh);
        MeshHandle(id)
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

    pub fn get_cpu_mesh(&self, handle: MeshHandle) -> Option<&CpuMesh> {
        self.cpu_meshes.get(handle.0 as usize)
    }

    pub fn is_loading(&self) -> bool {
        !self.load_progress.is_done()
    }

    pub fn load_mesh_async(&mut self, path: impl AsRef<Path>) -> AsyncMeshHandle {
        let path = path.as_ref().to_path_buf();
        if self.pending_paths.contains_key(&path) {
            return AsyncMeshHandle(path);
        }
        if self.mesh_path_cache.lock().unwrap().contains_key(&path) {
            return AsyncMeshHandle(path);
        }
        log::info!("load_mesh_async: {:?}", path);
        self.loader.request_mesh(path.clone());
        self.pending_paths.insert(path.clone(), ());
        self.load_progress.total += 1;
        self.load_progress.current = path.to_string_lossy().to_string();
        AsyncMeshHandle(path)
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

    fn apply_message(&mut self, msg: LoaderMessage) {
        match msg {
            LoaderMessage::MeshReady { path, source } => {
                self.load_progress.current = path.to_string_lossy().to_string();
                let pending = self.build_pending_mesh(source);
                self.upload_queue.lock().unwrap().push(PendingUpload::Mesh { path: path.clone(), meshes: pending });
                self.pending_paths.remove(&path);
                self.load_progress.completed += 1;
            }

            LoaderMessage::TextureReady { path, source } => {
                self.load_progress.current = path.to_string_lossy().to_string();
                self.upload_queue.lock().unwrap().push(PendingUpload::Texture {
                    path: path.clone(),
                    pixels: source.pixels,
                    width: source.width,
                    height: source.height,
                    format: ash::vk::Format::R8G8B8A8_SRGB,
                    name: path.file_name().unwrap_or_default().to_string_lossy().to_string(),
                });
                self.pending_paths.remove(&path);
                self.load_progress.completed += 1;
            }

            LoaderMessage::Error { path, error } => {
                log::error!("Ошибка загрузки {:?}: {}", path, error);
                self.pending_paths.remove(&path);
                self.load_progress.completed += 1;
            }
        }
    }

    fn build_pending_mesh(&self, source: MeshSource) -> Vec<PendingMesh> {
        source
            .primitives
            .into_iter()
            .map(|prim| {
                let material = prim.material.map(|m| {
                    let textures = prim
                        .textures
                        .into_iter()
                        .map(|(slot, pixels, width, height, name, image_index)| {
                            let format = match slot {
                                TextureSlot::Normal | TextureSlot::MetallicRoughness | TextureSlot::Occlusion => {
                                    ash::vk::Format::R8G8B8A8_UNORM
                                }
                                TextureSlot::Diffuse | TextureSlot::Emissive => ash::vk::Format::R8G8B8A8_SRGB,
                            };
                            PendingTexture { slot, pixels, width, height, format, name, image_index }
                        })
                        .collect();

                    PendingMaterial {
                        name: m.name,
                        shader_name: "diffuse".to_string(),
                        base_color: m.base_color,
                        metallic: m.metallic,
                        roughness: m.roughness,
                        textures,
                    }
                });

                let transform = Transform {
                    position: glam::Vec3::from(prim.node_translation),
                    rotation: glam::Quat::from_array(prim.node_rotation),
                    scale: glam::Vec3::from(prim.node_scale),
                };

                PendingMesh { cpu_mesh: prim.mesh, transform, material }
            })
            .collect()
    }

    pub fn get_mesh_instances(
        &self,
        handle: &AsyncMeshHandle,
    ) -> Option<Vec<(MeshHandle, Option<MaterialHandle>, Transform)>> {
        self.mesh_path_cache.lock().unwrap().get(&handle.0).cloned()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AsyncMeshHandle(pub PathBuf);

impl AsyncMeshHandle {
    pub fn path(&self) -> &Path {
        &self.0
    }
}
