use crate::assets::material_registry::MaterialRegistry;
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
use crate::render::gfx::types::Format;
use crate::render::world::PreparedUiDrawList;
use engine_materials::{Material, StrategyRegistry};
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
    text: TextService,
    upload_queue: UploadQueue,

    // TODO(asset-registry-refactor): AssetRegistry accumulates unrelated
    // domains (meshes, textures, text, and now materials) with no clear
    // ownership boundaries between them - this field pair and the material
    // methods below are a stopgap, not an endorsement of that shape. A
    // future refactor should probably split AssetRegistry into narrower,
    // independently-testable registries (mesh/texture/material/...) with an
    // explicit, thin composition point, rather than growing this struct
    // further.
    materials: MaterialRegistry,
    material_strategies: StrategyRegistry,
}

impl AssetRegistry {
    pub(crate) fn new() -> Self {
        let mut material_strategies = StrategyRegistry::new();
        material_strategies.register(engine_materials::strategies::PbrStrategy::default());
        material_strategies.register(engine_materials::strategies::UnlitStrategy::default());

        Self {
            meshes: MeshHandleAllocator::new(),
            upload_queue: UploadQueue::new(),
            texture_handles: TextureHandleAllocator::new(),
            textures: TextureStore::new(),
            text: TextService::new(),
            materials: MaterialRegistry::new(),
            material_strategies,
        }
    }

    // ==================== Public API ====================
    // Intended for `App` implementors, called from the game thread only.

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

    pub fn default_font(&self) -> FontId {
        self.text.default_font()
    }

    pub fn measure_text(&mut self, text: &str, px: f32) -> Vec2 {
        self.text.measure(text, px)
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

    /// Registers a new material and queues it for GPU upload on the next
    /// frame's extract. Game thread only.
    pub fn insert_material(&mut self, material: Material) -> engine_materials::MaterialHandle {
        self.materials.insert(material)
    }

    /// Mutates a material in place, re-queueing it for GPU upload. Returns
    /// `false` if the handle is unknown. Game thread only.
    pub fn modify_material(&mut self, handle: engine_materials::MaterialHandle, f: impl FnOnce(&mut Material)) -> bool {
        self.materials.modify(handle, f)
    }

    pub fn get_material(&self, handle: engine_materials::MaterialHandle) -> Option<&Material> {
        self.materials.get(handle)
    }

    /// Registers an additional custom shading strategy, making it available
    /// for materials to reference by name via `Material::strategy`.
    pub fn register_material_strategy(&mut self, strategy: impl engine_materials::ShadingStrategy + 'static) {
        self.material_strategies.register(strategy);
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

    /// Drains materials changed since the last call and the strategy
    /// registry needed to resolve them. Used by
    /// `render::extract::material::MaterialExtract`.
    pub(crate) fn drain_dirty_materials(
        &mut self,
    ) -> (Vec<(engine_materials::MaterialHandle, Material)>, &StrategyRegistry) {
        (self.materials.drain_dirty(), &self.material_strategies)
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
