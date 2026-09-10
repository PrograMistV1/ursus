use crate::assets::material_handle_allocator::MaterialHandleAllocator;
use std::collections::{HashMap, HashSet};
use ursus_materials::{Material, MaterialHandle};

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
    pub(crate) fn new() -> Self {
        Self::default()
    }

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
