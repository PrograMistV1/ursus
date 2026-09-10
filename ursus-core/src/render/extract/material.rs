use crate::assets::upload::GpuUploadRequest;
use crate::assets::AssetRegistry;
use std::sync::mpsc::Sender;

/// Resolves materials that changed since the last call against their
/// declared shading strategy, packs the result (currently always via
/// `engine_materials::pack::aos`), and queues the bytes for GPU upload.
///
/// Not an `ExtractSystem`: materials have no relationship to ECS entities
/// or `RenderWorld` resources, so there is nothing for this to read from
/// `GameWorld`/`RenderWorld`. It is called directly from
/// `EngineContext::publish_frame`, the same place `AssetRegistry::poll_assets`
/// is driven from.
pub fn extract_dirty_materials(cpu_assets: &mut AssetRegistry, upload_tx: &Sender<GpuUploadRequest>) {
    let (dirty, strategies) = cpu_assets.drain_dirty_materials();

    for (handle, material) in dirty {
        let Some(strategy) = strategies.by_name(material.strategy) else {
            log::error!(
                "extract_dirty_materials: material '{}' references unknown strategy '{}'",
                material.name,
                material.strategy
            );
            continue;
        };

        if let Err(e) = strategy.validate(&material) {
            log::error!("extract_dirty_materials: material '{}' failed validation: {e}", material.name);
            continue;
        }

        let values = match strategy.resolve(&material) {
            Ok(values) => values,
            Err(e) => {
                log::error!("extract_dirty_materials: failed to resolve material '{}': {e}", material.name);
                continue;
            }
        };

        let Some(bytes) = engine_materials::pack::aos::pack(strategy.fields(), &values) else {
            log::error!("extract_dirty_materials: failed to pack material '{}' - value/field mismatch", material.name);
            continue;
        };

        upload_tx.send(GpuUploadRequest::Material { handle, bytes }).ok();
    }
}
