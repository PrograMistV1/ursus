use crate::assets::mesh::Aabb;
use crate::assets::upload::GpuUploadRequest;
use crate::assets::CpuAssetServer;
use crate::components::mesh::{MaterialHandle, MeshHandle};
use crate::components::transform::Transform;
use crate::components::transform_interpolation::TransformInterpolation;
use crate::render::extract::ExtractSystem;
use crate::render::world::{
    ExtractedInstance, ExtractedMeshes, ExtractedRenderSettings, ExtractedShadowMeshes, RenderWorld,
};
use crate::GameWorld;
use std::sync::mpsc::Sender;

pub struct MeshExtract;
impl ExtractSystem for MeshExtract {
    fn extract(
        &self,
        world: &GameWorld,
        rw: &mut RenderWorld,
        _cpu_assets: &mut CpuAssetServer,
        _upload_tx: &Sender<GpuUploadRequest>,
    ) {
        let alpha = rw.get::<ExtractedRenderSettings>().map(|s| s.interpolation_alpha).unwrap_or(1.0);

        let mut meshes = ExtractedMeshes::default();
        let mut shadow_meshes = ExtractedShadowMeshes::default();

        for (mesh, transform, interp, mat, aabb) in world
            .inner
            .query::<(
                &MeshHandle,
                &Transform,
                Option<&TransformInterpolation>,
                Option<&MaterialHandle>,
                Option<&Aabb>,
            )>()
            .iter()
        {
            let model = match interp {
                Some(interp) => interp.interpolate(transform, alpha),
                None => transform.matrix(),
            };

            let instance = ExtractedInstance { mesh: *mesh, material: mat.copied(), model, aabb: aabb.copied() };

            shadow_meshes.instances.push(instance.clone());
            meshes.instances.push(instance);
        }

        rw.insert(meshes);
        rw.insert(shadow_meshes);
    }
    fn name(&self) -> &'static str {
        "extract_meshes"
    }
}
