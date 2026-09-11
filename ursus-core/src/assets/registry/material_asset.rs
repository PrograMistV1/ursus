use crate::assets::material_registry::MaterialRegistry as MaterialCpuStore;
use crate::assets::upload::GpuUploadRequest;
use std::sync::mpsc::Sender;
use ursus_materials::pack::aos::pack;
use ursus_materials::{Material, MaterialHandle, ShadingStrategy, StrategyRegistry};

/// CPU-side material registration + strategy resolution + GPU packing.
pub struct MaterialAssetRegistry {
    materials: MaterialCpuStore,
    strategies: StrategyRegistry,
}

impl MaterialAssetRegistry {
    pub(crate) fn new() -> Self {
        let mut strategies = StrategyRegistry::new();
        strategies.register(ursus_materials::strategies::PbrStrategy::default());
        strategies.register(ursus_materials::strategies::UnlitStrategy::default());
        Self { materials: MaterialCpuStore::default(), strategies }
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
