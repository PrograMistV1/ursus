use engine_materials::MaterialHandle;
use std::sync::mpsc::Sender;
use ursus_core::assets::mesh::Aabb;
use ursus_core::assets::upload::GpuUploadRequest;
use ursus_core::assets::AssetRegistry;
use ursus_core::components::mesh::{MeshHandle, TechniqueHandle};
use ursus_core::components::transform::Transform;
use ursus_core::components::transform_interpolation::TransformInterpolation;
use ursus_core::render::extract::ExtractSystem;
use ursus_core::render::world::{ExtractedInstance, ExtractedRenderSettings, RenderWorld};
use ursus_core::GameWorld;

#[derive(Default, Clone)]
pub struct ExtractedShadowMeshes {
    pub instances: Vec<ExtractedInstance>,
}

pub struct ShadowExtract;
impl ExtractSystem for ShadowExtract {
    fn extract(
        &self,
        world: &GameWorld,
        rw: &mut RenderWorld,
        _cpu_assets: &mut AssetRegistry,
        _upload_tx: &Sender<GpuUploadRequest>,
    ) {
        let alpha = rw.get::<ExtractedRenderSettings>().map(|s| s.interpolation_alpha).unwrap_or(1.0);

        let mut shadow_meshes = ExtractedShadowMeshes::default();

        for (mesh, transform, interp, mat, technique, aabb) in world
            .inner
            .query::<(
                &MeshHandle,
                &Transform,
                Option<&TransformInterpolation>,
                Option<&MaterialHandle>,
                Option<&TechniqueHandle>,
                Option<&Aabb>,
            )>()
            .iter()
        {
            let model = match interp {
                Some(interp) => interp.interpolate(transform, alpha),
                None => transform.matrix(),
            };

            shadow_meshes.instances.push(ExtractedInstance {
                mesh: *mesh,
                material: mat.copied(),
                technique: technique.map(|t| t.0.clone()),
                model,
                aabb: aabb.copied(),
            });
        }

        rw.insert(shadow_meshes);
    }
    fn name(&self) -> &'static str {
        "extract_shadow_meshes"
    }
}
