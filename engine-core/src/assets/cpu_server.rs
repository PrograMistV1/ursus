use crate::assets::loader_job::{BackgroundLoader, LoaderMessage, MeshSource};
use crate::assets::loader_registry::{AssetLoader, LoaderRegistry};
use crate::assets::material::MaterialPayload;
use crate::assets::mesh::{Aabb, CpuMesh};
use crate::assets::text::{FontId, TextRenderer};
use crate::assets::upload::GpuUploadRequest;
use crate::components::mesh::{MaterialHandle, MeshHandle};
use crate::components::transform::Transform;
use crate::render::gfx::Format;
use crate::render::world::PreparedUiDrawList;
use cosmic_text::fontdb::Family;
use glam::Vec2;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
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

pub type MeshInstance = (MeshHandle, Option<MaterialHandle>, Transform, Aabb);
type MeshPathCache = Arc<Mutex<HashMap<PathBuf, Vec<MeshInstance>>>>;

pub struct CpuAssetServer {
    // ==================== Internal state ====================
    cpu_meshes: Vec<CpuMesh>,
    next_material_handle: u32,

    mesh_path_cache: MeshPathCache,

    load_progress: LoadProgress,

    pending_uploads: Vec<GpuUploadRequest>,
    pub(crate) next_texture_handle: u32,
    texture_dedup: HashMap<TextureContentKey, TextureHandle>,

    loader: BackgroundLoader,
    pending_paths: HashMap<PathBuf, ()>,

    text_renderer: TextRenderer,
    default_font: FontId,
}

impl CpuAssetServer {
    pub(crate) fn new(registry: LoaderRegistry) -> Self {
        let text_renderer = TextRenderer::new();
        let default_font = text_renderer
            .find_system_font(Family::Monospace)
            .or_else(|| text_renderer.find_system_font(Family::SansSerif))
            .expect("Не найден ни один системный шрифт (Monospace/SansSerif) - установите шрифты в системе");
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

    // ==================== Public API ====================
    // Intended for `App` implementors, called from the game thread only.

    pub fn register_mesh(&mut self, mesh: CpuMesh) -> MeshHandle {
        let id = self.cpu_meshes.len() as u32;
        self.cpu_meshes.push(mesh);
        MeshHandle(id)
    }

    pub fn register_and_upload_mesh(&mut self, mesh: CpuMesh) -> MeshHandle {
        let name = mesh.name.clone();
        let vertices = mesh.vertices.clone();
        let indices = mesh.indices.clone();

        let handle = self.register_mesh(mesh);

        self.pending_uploads.push(GpuUploadRequest::Mesh { handle, vertices, indices, name });

        handle
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

    pub fn get_mesh_instances(&self, handle: &AsyncMeshHandle) -> Option<Vec<MeshInstance>> {
        self.mesh_path_cache.lock().unwrap().get(&handle.0).cloned()
    }

    pub fn load_progress(&self) -> &LoadProgress {
        &self.load_progress
    }

    pub fn default_font(&self) -> FontId {
        self.default_font
    }

    pub fn measure_text(&mut self, text: &str, px: f32) -> Vec2 {
        self.text_renderer.measure(self.default_font, text, px)
    }

    pub fn upload_texture_rgba8(
        &mut self,
        pixels: Vec<u8>,
        width: u32,
        height: u32,
        format: Format,
        name: impl Into<String>,
    ) -> TextureHandle {
        self.dedup_or_upload_texture(pixels, width, height, format, name.into())
    }

    pub fn register_material(
        &mut self,
        payload: Box<dyn MaterialPayload>,
        texture_slots: Vec<(String, TextureHandle)>,
    ) -> MaterialHandle {
        let handle = MaterialHandle(self.next_material_handle);
        self.next_material_handle += 1;

        self.pending_uploads.push(GpuUploadRequest::Material { handle, payload, texture_slots });

        handle
    }

    // ==================== Crate-internal API ====================
    // Used by other engine-core modules (extract systems, EngineContext, etc.), never by
    // App implementors directly.

    pub(crate) fn register_loader(&self, loader: impl AssetLoader + 'static) {
        self.loader.register_loader(Arc::new(loader));
    }

    pub(crate) fn register_loader_arc(&self, loader: Arc<dyn AssetLoader>) {
        self.loader.register_loader(loader);
    }

    pub(crate) fn get_cpu_mesh(&self, handle: MeshHandle) -> Option<&CpuMesh> {
        self.cpu_meshes.get(handle.0 as usize)
    }

    pub(crate) fn poll_loader(&mut self) {
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

    pub(crate) fn flush_uploads_cpu(&mut self, tx: &Sender<GpuUploadRequest>) {
        for req in self.pending_uploads.drain(..) {
            let _ = tx.send(req);
        }
    }

    pub(crate) fn flush_text_atlas(&mut self, upload_tx: &Sender<GpuUploadRequest>) {
        self.text_renderer.flush_atlas_to_channel(&mut self.next_texture_handle, upload_tx);
    }

    pub(crate) fn prepare_text(
        &mut self,
        text: &str,
        font_size: f32,
        pos: Vec2,
        color: [f32; 4],
        out: &mut PreparedUiDrawList,
    ) {
        let font = self.default_font;
        self.text_renderer.prepare_text(font, text, font_size, pos, color, None, out);
    }

    // ==================== Private helpers ====================

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

    fn build_instances_and_queue_uploads(&mut self, source: MeshSource) -> Vec<MeshInstance> {
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

                self.register_material(loaded_material.payload, texture_slots)
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
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AsyncMeshHandle(pub PathBuf);

impl AsyncMeshHandle {
    pub fn path(&self) -> &Path {
        &self.0
    }
}
