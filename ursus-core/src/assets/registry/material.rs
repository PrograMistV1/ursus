use crate::assets::upload::GpuUploadRequest;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Sender;
use ursus_materials::pack::aos::pack;
use ursus_materials::strategies::{PbrStrategy, UnlitStrategy};
use ursus_materials::{Material, MaterialHandle, ShadingStrategy, StrategyRegistry};

/// The only source of `MaterialHandle` in the system.
#[derive(Default)]
pub(crate) struct MaterialHandleAllocator {
    next: u32,
}

impl MaterialHandleAllocator {
    pub(crate) fn alloc(&mut self) -> MaterialHandle {
        let id = self.next;
        self.next += 1;
        MaterialHandle(id)
    }
}

/// CPU-side storage for materials, with dirty tracking so unchanged
/// materials aren't re-resolved/re-packed every frame.
///
/// Game thread only. Holds `Material` values (data + declared strategy
/// name); actually resolving a material against a `StrategyRegistry` and
/// packing it into GPU-visible bytes happens later, in the material extract
/// system - `MaterialRegistry` itself doesn't know about strategies or GPU
/// layouts.
#[derive(Default)]
pub struct MaterialRegistry {
    handles: MaterialHandleAllocator,
    materials: HashMap<MaterialHandle, Material>,
    dirty: HashSet<MaterialHandle>,
}

impl MaterialRegistry {
    /// Registers a new material and marks it dirty so it gets packed and
    /// uploaded on the next extract.
    pub fn insert(&mut self, material: Material) -> MaterialHandle {
        let handle = self.handles.alloc();
        self.materials.insert(handle, material);
        self.dirty.insert(handle);
        handle
    }

    /// Mutates a material in place via `f`, then marks it dirty. Returns
    /// `false` if `handle` doesn't exist.
    pub fn modify(&mut self, handle: MaterialHandle, f: impl FnOnce(&mut Material)) -> bool {
        let Some(material) = self.materials.get_mut(&handle) else {
            return false;
        };
        f(material);
        self.dirty.insert(handle);
        true
    }

    pub fn get(&self, handle: MaterialHandle) -> Option<&Material> {
        self.materials.get(&handle)
    }

    /// Drains the set of materials that changed since the last call,
    /// pairing each with its current data. Used by the material extract
    /// system once per frame.
    pub(crate) fn drain_dirty(&mut self) -> Vec<(MaterialHandle, Material)> {
        self.dirty.drain().filter_map(|handle| self.materials.get(&handle).map(|m| (handle, m.clone()))).collect()
    }
}

/// CPU-side material registration + strategy resolution + GPU packing.
pub struct MaterialAssetRegistry {
    materials: MaterialRegistry,
    strategies: StrategyRegistry,
}

impl MaterialAssetRegistry {
    pub(crate) fn new() -> Self {
        let mut strategies = StrategyRegistry::new();
        strategies.register(PbrStrategy::default());
        strategies.register(UnlitStrategy::default());
        Self { materials: MaterialRegistry::default(), strategies }
    }

    pub fn insert(&mut self, material: Material) -> MaterialHandle {
        self.materials.insert(material)
    }

    pub fn modify(&mut self, handle: MaterialHandle, f: impl FnOnce(&mut Material)) -> bool {
        self.materials.modify(handle, f)
    }

    pub fn get(&self, handle: MaterialHandle) -> Option<&Material> {
        self.materials.get(handle)
    }

    pub fn register_strategy(&mut self, strategy: impl ShadingStrategy + 'static) {
        self.strategies.register(strategy);
    }

    /// Resolves and packs materials changed since the last call, queuing
    /// the bytes for GPU upload. Called once per frame.
    pub(crate) fn flush_dirty(&mut self, upload_tx: &Sender<GpuUploadRequest>) {
        for (handle, material) in self.materials.drain_dirty() {
            let Some(strategy) = self.strategies.by_name(material.strategy) else {
                log::error!(
                    "MaterialAssetRegistry: material '{}' references unknown strategy '{}'",
                    material.name,
                    material.strategy
                );
                continue;
            };

            if let Err(e) = strategy.validate(&material) {
                log::error!("MaterialAssetRegistry: material '{}' failed validation: {e}", material.name);
                continue;
            }

            let values = match strategy.resolve(&material) {
                Ok(values) => values,
                Err(e) => {
                    log::error!("MaterialAssetRegistry: failed to resolve material '{}': {e}", material.name);
                    continue;
                }
            };

            let Some(bytes) = pack(strategy.fields(), &values) else {
                log::error!(
                    "MaterialAssetRegistry: failed to pack material '{}' - value/field mismatch",
                    material.name
                );
                continue;
            };

            upload_tx.send(GpuUploadRequest::Material { handle, bytes }).ok();
        }
    }
}
