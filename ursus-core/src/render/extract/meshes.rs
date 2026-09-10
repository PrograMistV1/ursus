use crate::assets::upload::GpuUploadRequest;
use crate::assets::AssetRegistry;
use crate::render::extract::ExtractSystem;
use crate::render::world::{ExtractedInstance, ExtractedMeshes, ExtractedRenderSettings, RenderWorld};
use std::sync::mpsc::Sender;
use ursus_ecs::components::mesh::{MeshHandle, TechniqueHandle};
use ursus_ecs::components::transform::Transform;
use ursus_ecs::components::transform_interpolation::TransformInterpolation;
use ursus_ecs::GameWorld;
use ursus_materials::MaterialHandle;

pub struct MeshExtract;
impl ExtractSystem for MeshExtract {
    fn extract(
        &self,
        world: &GameWorld,
        rw: &mut RenderWorld,
        _cpu_assets: &mut AssetRegistry,
        _upload_tx: &Sender<GpuUploadRequest>,
    ) {
        let alpha = rw.get::<ExtractedRenderSettings>().map(|s| s.interpolation_alpha).unwrap_or(1.0);

        let mut meshes = ExtractedMeshes::default();

        for (mesh, transform, interp, mat, technique, aabb) in world
            .inner
            .query::<(
                &MeshHandle,
                &Transform,
                Option<&TransformInterpolation>,
                Option<&MaterialHandle>,
                Option<&TechniqueHandle>,
                Option<&crate::assets::mesh::Aabb>,
            )>()
            .iter()
        {
            let model = match interp {
                Some(interp) => interp.interpolate(transform, alpha),
                None => transform.matrix(),
            };

            meshes.instances.push(ExtractedInstance {
                mesh: *mesh,
                material: mat.copied(),
                technique: technique.map(|t| t.0.clone()),
                model,
                aabb: aabb.copied(),
            });
        }

        rw.insert(meshes);
    }
    fn name(&self) -> &'static str {
        "extract_meshes"
    }
}
