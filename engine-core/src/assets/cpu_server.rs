use crate::assets::loader_job::{BackgroundLoader, LoaderMessage, MeshSource};
use crate::assets::loader_registry::{AssetLoader, LoaderRegistry};
use crate::assets::mesh::{Aabb, CpuMesh};
use crate::assets::text::{FontId, TextRenderer};
use crate::assets::upload::GpuUploadRequest;
use crate::components::mesh::{MaterialHandle, MeshHandle};
use crate::components::transform::Transform;
use crate::render::gfx::Format;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};

const DEFAULT_FONT_BYTES: &[u8] = include_bytes!("../../../assets/fonts/RobotoMono.ttf");

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

const TEXTURE_HASH_SAMPLE_COUNT: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct TextureContentKey(u64, usize, u32, u32, Format);

fn hash_texture(pixels: &[u8], width: u32, height: u32, format: Format) -> TextureContentKey {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();

    let len = pixels.len();
    if len <= TEXTURE_HASH_SAMPLE_COUNT * 2 {
        pixels.hash(&mut hasher);
    } else {
        let step = len / TEXTURE_HASH_SAMPLE_COUNT;
        let mut i = 0;
        while i < len {
            hasher.write_u8(pixels[i]);
            i += step;
        }
        hasher.write(&pixels[..32.min(len)]);
        hasher.write(&pixels[len - 32.min(len)..]);
    }

    TextureContentKey(hasher.finish(), len, width, height, format)
}

pub struct CpuAssetServer {
    pub cpu_meshes: Vec<CpuMesh>,
    next_material_handle: u32,

    pub mesh_path_cache: Arc<Mutex<HashMap<PathBuf, Vec<(MeshHandle, Option<MaterialHandle>, Transform, Aabb)>>>>,

    pub load_progress: LoadProgress,

    pending_uploads: Vec<GpuUploadRequest>,
    pub(crate) next_texture_handle: u32,
    texture_dedup: HashMap<TextureContentKey, TextureHandle>,

    loader: BackgroundLoader,
    pending_paths: HashMap<PathBuf, ()>,

    pub text_renderer: TextRenderer,
    pub default_font: FontId,
}

impl CpuAssetServer {
    pub fn new(registry: LoaderRegistry) -> Self {
        let mut text_renderer = TextRenderer::new();
        let default_font = text_renderer.load_font(DEFAULT_FONT_BYTES);
        Self {
            cpu_meshes: Vec::new(),
            next_material_handle: 0,
            mesh_path_cache: Arc::new(Mutex::new(HashMap::new())),
            load_progress: LoadProgress::default(),
            pending_uploads: Vec::new(),
            next_texture_handle: 1,
            texture_dedup: HashMap::new(),
            loader: BackgroundLoader::new(registry),
            pending_paths: HashMap::new(),
            text_renderer,
            default_font,
        }
    }

    pub fn register_loader(&self, loader: impl AssetLoader + 'static) {
        self.loader.register_loader(Arc::new(loader));
    }

    pub fn register_loader_arc(&self, loader: Arc<dyn AssetLoader>) {
        self.loader.register_loader(loader);
    }

    fn alloc_texture_handle(&mut self) -> TextureHandle {
        let h = TextureHandle(self.next_texture_handle);
        self.next_texture_handle += 1;
        h
    }

    fn dedup_or_upload_texture(
        &mut self,
        pixels: Vec<u8>,
        width: u32,
        height: u32,
        format: Format,
        name: String,
    ) -> TextureHandle {
        let key = hash_texture(&pixels, width, height, format);
        if let Some(&handle) = self.texture_dedup.get(&key) {
            return handle;
        }
        let handle = self.alloc_texture_handle();
        self.pending_uploads.push(GpuUploadRequest::Texture { handle, pixels, width, height, format, name });
        self.texture_dedup.insert(key, handle);
        handle
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

        for prim in source.primitives {
            let aabb = Aabb::from_vertices(&prim.mesh.vertices);
            let name = prim.mesh.name.clone();
            let vertices = prim.mesh.vertices.clone();
            let indices = prim.mesh.indices.clone();
            let mesh_handle = self.register_mesh(prim.mesh);

            self.pending_uploads.push(GpuUploadRequest::Mesh {
                handle: mesh_handle,
                vertices,
                indices,
                name: name.clone(),
            });

            let material_handle = prim.material.map(|loaded_material| {
                let mut texture_slots = Vec::new();
                for (role, tex) in loaded_material.textures {
                    let tex_name = format!("{}_{}", name, role);
                    let tex_handle =
                        self.dedup_or_upload_texture(tex.pixels, tex.width, tex.height, tex.format, tex_name);
                    texture_slots.push((role, tex_handle));
                }

                let handle = MaterialHandle(self.next_material_handle);
                self.next_material_handle += 1;

                self.pending_uploads.push(GpuUploadRequest::Material {
                    handle,
                    payload: loaded_material.payload,
                    texture_slots,
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

    pub fn flush_text_atlas(&mut self, upload_tx: &Sender<GpuUploadRequest>) {
        self.text_renderer.flush_atlas_to_channel(&mut self.next_texture_handle, upload_tx);
    }

    pub fn get_mesh_instances(
        &self,
        handle: &AsyncMeshHandle,
    ) -> Option<Vec<(MeshHandle, Option<MaterialHandle>, Transform, Aabb)>> {
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
