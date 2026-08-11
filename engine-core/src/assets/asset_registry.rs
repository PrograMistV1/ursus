use crate::assets::material::MaterialPayload;
use crate::assets::material_handle_allocator::MaterialHandleAllocator;
use crate::assets::mesh::{Aabb, CpuMesh};
use crate::assets::mesh_handle_allocator::MeshHandleAllocator;
use crate::assets::text::FontId;
use crate::assets::text_service::TextService;
use crate::assets::texture_handle_allocator::TextureHandleAllocator;
use crate::assets::texture_store::{TextureRegistration, TextureStore};
use crate::assets::upload::GpuUploadRequest;
use crate::assets::upload_queue::UploadQueue;
use crate::components::mesh::{MaterialHandle, MeshHandle};
use crate::components::transform::Transform;
use crate::render::gfx::Format;
use crate::render::world::PreparedUiDrawList;
use glam::Vec2;
use std::hash::Hash;
use std::sync::mpsc::Sender;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureHandle(pub u32);

pub type MeshInstance = (MeshHandle, Option<MaterialHandle>, Transform, Aabb);

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
    meshes: MeshHandleAllocator,
    texture_handles: TextureHandleAllocator,
    textures: TextureStore,
    material_handles: MaterialHandleAllocator,
    text: TextService,
    upload_queue: UploadQueue,
}

impl AssetRegistry {
    pub(crate) fn new() -> Self {
        Self {
            meshes: MeshHandleAllocator::new(),
            material_handles: MaterialHandleAllocator::new(),
            upload_queue: UploadQueue::new(),
            texture_handles: TextureHandleAllocator::new(),
            textures: TextureStore::new(),
            text: TextService::new(),
        }
    }

    // ==================== Public API ====================
    // Intended for `App` implementors, called from the game thread only.
    /// Registers a CPU-side mesh and queues it for GPU upload.
    ///
    /// Game thread only. Synchronous registration; the actual GPU upload happens later on
    /// the render thread once [`Self::flush_uploads_cpu`] drains `pending_uploads`.
    pub fn upload_mesh(&mut self, mesh: CpuMesh) -> MeshHandle {
        let handle = self.meshes.alloc();
        self.upload_queue.push(GpuUploadRequest::Mesh {
            handle,
            vertices: mesh.vertices,
            indices: mesh.indices,
            name: mesh.name,
        });
        handle
    }

    /// The engine's default UI font, picked at startup from installed system fonts
    /// (Monospace, falling back to SansSerif).
    pub fn default_font(&self) -> FontId {
        self.text.default_font()
    }

    /// Measures the on-screen size (in pixels) that `text` would occupy at font size `px`,
    /// using the default font. Game thread only; synchronous, does no GPU work.
    pub fn measure_text(&mut self, text: &str, px: f32) -> Vec2 {
        self.text.measure(text, px)
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
        let handle = self.material_handles.alloc();

        self.upload_queue.push(GpuUploadRequest::Material { handle, payload, texture_slots });

        handle
    }

    // ==================== Crate-internal API ====================
    // Used by other engine-core modules (extract systems, EngineContext, etc.), never by
    // App implementors directly.

    pub(crate) fn flush_uploads_cpu(&mut self, tx: &Sender<GpuUploadRequest>) {
        self.upload_queue.drain_to(tx);
    }

    pub(crate) fn flush_text_atlas(&mut self, upload_tx: &Sender<GpuUploadRequest>) {
        self.text.flush_atlas(&mut self.texture_handles, upload_tx);
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
        self.text.prepare(text, font_size, pos, color, out);
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
}
