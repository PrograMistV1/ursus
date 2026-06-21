use crate::assets::loader_job::{BackgroundLoader, LoaderMessage, MeshSource};
use crate::assets::mesh::{Aabb, CpuMesh};
use crate::assets::shader_registry::TextureSlot;
use crate::assets::upload::GpuUploadRequest;
use crate::components::mesh::{MaterialHandle, MeshHandle};
use crate::components::transform::Transform;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureHandle(pub u32);

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

pub struct CpuAssetServer {
    pub cpu_meshes: Vec<CpuMesh>,
    next_material_handle: u32,

    pub mesh_path_cache: Arc<Mutex<HashMap<PathBuf, Vec<(MeshHandle, Option<MaterialHandle>, Transform, Aabb)>>>>,

    pub load_progress: LoadProgress,

    pending_uploads: Vec<GpuUploadRequest>,
    next_texture_handle: u32,

    loader: BackgroundLoader,
    pending_paths: HashMap<PathBuf, ()>,
}

impl CpuAssetServer {
    pub fn new() -> Self {
        Self {
            cpu_meshes: Vec::new(),
            next_material_handle: 0,
            mesh_path_cache: Arc::new(Mutex::new(HashMap::new())),
            load_progress: LoadProgress::default(),
            pending_uploads: Vec::new(),
            next_texture_handle: 1,
            loader: BackgroundLoader::new(),
            pending_paths: HashMap::new(),
        }
    }

    fn alloc_texture_handle(&mut self) -> TextureHandle {
        let h = TextureHandle(self.next_texture_handle);
        self.next_texture_handle += 1;
        h
    }

    pub fn register_mesh(&mut self, mesh: CpuMesh) -> MeshHandle {
        let id = self.cpu_meshes.len() as u32;
        self.cpu_meshes.push(mesh);
        MeshHandle(id)
    }
    pub fn get_cpu_mesh(&self, handle: MeshHandle) -> Option<&CpuMesh> {
        self.cpu_meshes.get(handle.0 as usize)
    }
    pub fn is_loading(&self) -> bool {
        !self.load_progress.is_done()
    }

    pub fn load_mesh_async(&mut self, path: impl AsRef<Path>) -> AsyncMeshHandle {
        let path = path.as_ref().to_path_buf();
        if self.pending_paths.contains_key(&path) || self.mesh_path_cache.lock().unwrap().contains_key(&path) {
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
                let instances = self.build_instances_and_queue_uploads(source);
                self.mesh_path_cache.lock().unwrap().insert(path.clone(), instances);
                self.pending_paths.remove(&path);
                self.load_progress.completed += 1;
            }
            LoaderMessage::TextureReady { .. } => {}
            LoaderMessage::Error { path, error } => {
                log::error!("Ошибка загрузки {:?}: {}", path, error);
                self.pending_paths.remove(&path);
                self.load_progress.completed += 1;
            }
        }
    }

    fn build_instances_and_queue_uploads(
        &mut self,
        source: MeshSource,
    ) -> Vec<(MeshHandle, Option<MaterialHandle>, Transform, Aabb)> {
        let mut instances = Vec::new();
        let mut image_index_cache: HashMap<usize, TextureHandle> = HashMap::new();

        for prim in source.primitives {
            let aabb = Aabb::from_vertices(&prim.mesh.vertices);
            let name = prim.mesh.name.clone();
            let vertices = prim.mesh.vertices.clone();
            let indices = prim.mesh.indices.clone();
            let mesh_handle = self.register_mesh(prim.mesh);

            self.pending_uploads.push(GpuUploadRequest::Mesh { handle: mesh_handle, vertices, indices, name });

            let material_handle = prim.material.map(|m| {
                let mut texture_slots = Vec::new();
                for (slot, pixels, width, height, tex_name, image_index) in prim.textures {
                    let tex_handle = *image_index_cache.entry(image_index).or_insert_with(|| {
                        let h = self.alloc_texture_handle();
                        let format = match slot {
                            TextureSlot::Normal | TextureSlot::MetallicRoughness | TextureSlot::Occlusion => {
                                ash::vk::Format::R8G8B8A8_UNORM
                            }
                            TextureSlot::Diffuse | TextureSlot::Emissive => ash::vk::Format::R8G8B8A8_SRGB,
                        };
                        self.pending_uploads.push(GpuUploadRequest::Texture {
                            handle: h,
                            pixels: pixels.clone(),
                            width,
                            height,
                            format,
                            name: tex_name.clone(),
                        });
                        h
                    });
                    texture_slots.push((slot, tex_handle));
                }

                let handle = MaterialHandle(self.next_material_handle);
                self.next_material_handle += 1;

                self.pending_uploads.push(GpuUploadRequest::Material {
                    handle,
                    base_color: m.base_color,
                    metallic: m.metallic,
                    roughness: m.roughness,
                    emissive: [m.emissive[0], m.emissive[1], m.emissive[2], 0.0],
                    texture_slots,
                    name: m.name,
                });

                handle
            });

            let transform = Transform {
                position: glam::Vec3::from(prim.node_translation),
                rotation: glam::Quat::from_array(prim.node_rotation),
                scale: glam::Vec3::from(prim.node_scale),
            };

            instances.push((mesh_handle, material_handle, transform, aabb));
        }

        instances
    }

    pub fn flush_uploads_cpu(&mut self, tx: &Sender<GpuUploadRequest>) {
        for req in self.pending_uploads.drain(..) {
            let _ = tx.send(req);
        }
    }

    pub fn get_mesh_instances(
        &self,
        handle: &AsyncMeshHandle,
    ) -> Option<Vec<(MeshHandle, Option<MaterialHandle>, Transform, Aabb)>> {
        self.mesh_path_cache.lock().unwrap().get(&handle.0).cloned()
    }
}

impl Default for CpuAssetServer {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AsyncMeshHandle(pub PathBuf);

impl AsyncMeshHandle {
    pub fn path(&self) -> &Path {
        &self.0
    }
}
