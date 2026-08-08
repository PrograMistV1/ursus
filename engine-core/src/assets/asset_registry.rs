use crate::assets::loader_job::{BackgroundLoader, LoaderMessage, MeshSource};
use crate::assets::loader_registry::AssetLoader;
use crate::assets::material::MaterialPayload;
use crate::assets::mesh::{Aabb, CpuMesh};
use crate::assets::mesh_store::MeshStore;
use crate::assets::text::{FontId, TextRenderer};
use crate::assets::texture_handle_allocator::TextureHandleAllocator;
use crate::assets::texture_store::{TextureRegistration, TextureStore};
use crate::assets::upload::GpuUploadRequest;
use crate::assets::upload_queue::UploadQueue;
use crate::components::mesh::{MaterialHandle, MeshHandle};
use crate::components::transform::Transform;
use crate::render::gfx::Format;
use crate::render::world::PreparedUiDrawList;
use cosmic_text::fontdb::Family;
use glam::Vec2;
use std::collections::HashMap;
use std::hash::Hash;
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

pub type MeshInstance = (MeshHandle, Option<MaterialHandle>, Transform, Aabb);
type MeshPathCache = Arc<Mutex<HashMap<PathBuf, Vec<MeshInstance>>>>;

/// CPU-side asset registration and staging surface.
///
/// `AssetRegistry` is the API `App` implementors use from the game thread to register
/// meshes, textures, materials, and fonts. It never touches the GPU directly - every
/// upload is staged into `pending_uploads` and later drained by [`Self::flush_uploads_cpu`]
/// (called once per frame from [`crate::app::EngineContext::poll_assets`]) onto a channel.
/// The render thread receives those requests and performs the actual GPU upload
/// (`flush_uploads_gpu` in `render/thread/mod.rs`), keeping all Vulkan calls off the game
/// thread.
///
/// Mesh/material loading from disk (`.gltf`, `.obj`, ...) happens on a background thread
/// (see `loader_job.rs`); results are polled every frame via [`Self::poll_loader`] and
/// queued onto `pending_uploads` the same way as synchronous registrations.
pub struct AssetRegistry {
    // ==================== Internal state ====================
    meshes: MeshStore,
    next_material_handle: u32,

    mesh_path_cache: MeshPathCache,

    load_progress: LoadProgress,

    upload_queue: UploadQueue,
    texture_handles: TextureHandleAllocator,
    textures: TextureStore,

    loader: BackgroundLoader,
    pending_paths: HashMap<PathBuf, ()>,

    text_renderer: TextRenderer,
    default_font: FontId,
}

impl AssetRegistry {
    pub(crate) fn new() -> Self {
        let text_renderer = TextRenderer::new();
        let default_font = text_renderer
            .find_system_font(Family::Monospace)
            .or_else(|| text_renderer.find_system_font(Family::SansSerif))
            .expect("No system fonts found (Monospace/SansSerif) - install fonts in the system");
        Self {
            meshes: MeshStore::new(),
            next_material_handle: 0,
            mesh_path_cache: Arc::new(Mutex::new(HashMap::new())),
            load_progress: LoadProgress::default(),
            upload_queue: UploadQueue::new(),
            texture_handles: TextureHandleAllocator::new(),
            textures: TextureStore::new(),
            loader: BackgroundLoader::new(),
            pending_paths: HashMap::new(),
            text_renderer,
            default_font,
        }
    }

    // ==================== Public API ====================
    // Intended for `App` implementors, called from the game thread only.

    /// Registers a CPU-side mesh without queuing a GPU upload.
    ///
    /// Use this together with [`Self::register_and_upload_mesh`] when you want to build up
    /// mesh data first and upload it later, or when the mesh is uploaded through some other
    /// path. Synchronous, no channel traffic.
    pub fn register_mesh(&mut self, mesh: CpuMesh) -> MeshHandle {
        self.meshes.register(mesh)
    }

    /// Registers a CPU-side mesh and queues it for GPU upload.
    ///
    /// Game thread only. Synchronous registration; the actual GPU upload happens later on
    /// the render thread once [`Self::flush_uploads_cpu`] drains `pending_uploads`.
    pub fn register_and_upload_mesh(&mut self, mesh: CpuMesh) -> MeshHandle {
        let name = mesh.name.clone();
        let vertices = mesh.vertices.clone();
        let indices = mesh.indices.clone();

        let handle = self.register_mesh(mesh);

        self.upload_queue.push(GpuUploadRequest::Mesh { handle, vertices, indices, name });

        handle
    }

    /// Queues an async load of a mesh file (`.gltf`/`.glb`/`.obj`, depending on registered
    /// loaders) from disk on a background thread.
    ///
    /// Game thread only. Returns immediately with a handle you can poll via
    /// [`Self::get_mesh_instances`] once loading completes (tracked by [`Self::is_loading`]).
    /// Repeated calls with the same path are deduplicated.
    pub fn load_mesh_async(&mut self, path: impl AsRef<Path>) -> AsyncMeshHandle {
        let path = path.as_ref().to_path_buf();
        if self.pending_paths.contains_key(&path) || self.mesh_path_cache.lock().unwrap().contains_key(&path) {
            return AsyncMeshHandle(path);
        }
        log::trace!("load_mesh_async: {:?}", path);
        self.loader.request_mesh(path.clone());
        self.pending_paths.insert(path.clone(), ());
        self.load_progress.total += 1;
        self.load_progress.current = path.to_string_lossy().to_string();
        AsyncMeshHandle(path)
    }

    /// Returns the mesh/material/transform instances produced by a completed
    /// [`Self::load_mesh_async`] call, or `None` if it hasn't finished yet.
    ///
    /// Game thread only. Synchronous, reads from an in-memory cache populated by
    /// [`Self::poll_loader`].
    pub fn get_mesh_instances(&self, handle: &AsyncMeshHandle) -> Option<Vec<MeshInstance>> {
        self.mesh_path_cache.lock().unwrap().get(&handle.0).cloned()
    }

    /// Returns `true` while any [`Self::load_mesh_async`] request is still in flight.
    pub fn is_loading(&self) -> bool {
        !self.load_progress.is_done()
    }

    /// Read-only progress snapshot for in-flight async mesh loads (see
    /// [`Self::load_mesh_async`]). Use [`LoadProgress::fraction`] for a 0.0-1.0 value
    /// suitable for a loading bar.
    pub fn load_progress(&self) -> &LoadProgress {
        &self.load_progress
    }

    /// The engine's default UI font, picked at startup from installed system fonts
    /// (Monospace, falling back to SansSerif).
    pub fn default_font(&self) -> FontId {
        self.default_font
    }

    /// Measures the on-screen size (in pixels) that `text` would occupy at font size `px`,
    /// using the default font. Game thread only; synchronous, does no GPU work.
    pub fn measure_text(&mut self, text: &str, px: f32) -> Vec2 {
        self.text_renderer.measure(self.default_font, text, px)
    }

    /// Registers an RGBA8 texture and queues it for GPU upload.
    ///
    /// Game thread only. Identical pixel content is deduplicated - calling this twice with
    /// the same bytes returns the same [`TextureHandle`] without a second upload.
    /// `pixels` must be tightly packed RGBA8 (4 bytes per pixel, `width * height * 4` bytes
    /// total) unless `format` says otherwise.
    ///
    /// Pairs with [`Self::register_material`] - see that method's docs for a worked example.
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

    /// Registers a material payload and queues its texture bindings.
    ///
    /// Game thread only. `payload` carries whatever material data your render pipeline
    /// expects (e.g. `PbrMetallicRoughness` from `engine-gltf-loader`); `texture_slots` maps
    /// role names (e.g. `"base_color"`, `"normal"`) to texture handles obtained from
    /// [`Self::upload_texture_rgba8`].
    ///
    /// ```ignore
    /// let diffuse = cpu_assets.upload_texture_rgba8(pixels, w, h, Format::Rgba8Srgb, "brick_diffuse");
    /// let material = cpu_assets.register_material(
    ///     Box::new(PbrMetallicRoughness { name: "brick".into(), base_color: [1.0; 4], metallic: 0.0, roughness: 0.8, emissive: [0.0; 3] }),
    ///     vec![("base_color".to_string(), diffuse)],
    /// );
    /// ```
    pub fn register_material(
        &mut self,
        payload: Box<dyn MaterialPayload>,
        texture_slots: Vec<(String, TextureHandle)>,
    ) -> MaterialHandle {
        let handle = MaterialHandle(self.next_material_handle);
        self.next_material_handle += 1;

        self.upload_queue.push(GpuUploadRequest::Material { handle, payload, texture_slots });

        handle
    }

    /// Registers a custom [`AssetLoader`] for background mesh loading.
    ///
    /// It can be called at any point after the engine has started - registration is sent to
    /// the background loading thread over the same channel as load requests, so any
    /// [`Self::load_mesh_async`] call made after this returns is guaranteed to see the loader,
    /// even though registration itself happens asynchronously. Calls made before registration
    /// will fail with an error reporting that no loader is registered for that extension.
    ///
    /// `engine-pipelines` registers its built-in glTF/OBJ loaders this way (see
    /// `register_builtin_loaders` in that crate), typically called once from `App::on_start`.
    pub fn register_loader(&self, loader: impl AssetLoader + 'static) {
        self.loader.register_loader(Arc::new(loader));
    }

    // ==================== Crate-internal API ====================
    // Used by other engine-core modules (extract systems, EngineContext, etc.), never by
    // App implementors directly.

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
        self.upload_queue.drain_to(tx);
    }

    pub(crate) fn flush_text_atlas(&mut self, upload_tx: &Sender<GpuUploadRequest>) {
        self.text_renderer.flush_atlas_to_channel(&mut self.texture_handles, upload_tx);
    }

    /// Shapes and rasterizes `text` into `out`, using the engine's default font. Used by the
    /// built-in UI extract system; not part of the public API since it writes directly into
    /// a [`PreparedUiDrawList`] rather than returning owned data.
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

    fn dedup_or_upload_texture(
        &mut self,
        pixels: Vec<u8>,
        width: u32,
        height: u32,
        format: Format,
        name: String,
    ) -> TextureHandle {
        match self.textures.register(&pixels, width, height, format, &mut self.texture_handles) {
            TextureRegistration::Existing(handle) => handle,
            TextureRegistration::New(handle) => {
                self.upload_queue.push(GpuUploadRequest::Texture { handle, pixels, width, height, format, name });
                handle
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

    fn build_instances_and_queue_uploads(&mut self, source: MeshSource) -> Vec<MeshInstance> {
        let mut instances = Vec::new();

        for prim in source.primitives {
            let aabb = Aabb::from_vertices(&prim.mesh.vertices);
            let name = prim.mesh.name.clone();
            let vertices = prim.mesh.vertices.clone();
            let indices = prim.mesh.indices.clone();
            let mesh_handle = self.register_mesh(prim.mesh);

            self.upload_queue.push(GpuUploadRequest::Mesh {
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
