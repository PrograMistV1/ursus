use ursus_core::assets::mesh::Aabb;
use ursus_core::render::extract::ExtractSystem;
use ursus_core::render::world::{ExtractedInstance, ExtractedRenderSettings, RWorld};
use ursus_ecs::components::mesh::{MeshHandle, TechniqueHandle};
use ursus_ecs::components::transform::Transform;
use ursus_ecs::components::transform_interpolation::TransformInterpolation;
use ursus_ecs::World;
use ursus_materials::MaterialHandle;

#[derive(Default, Clone)]
pub struct ExtractedShadowMeshes {
    pub instances: Vec<ExtractedInstance>,
}

pub struct ShadowExtract;
impl ExtractSystem for ShadowExtract {
    fn extract(&self, world: &World, r_world: &mut RWorld) {
        let alpha = r_world.get::<ExtractedRenderSettings>().map(|s| s.interpolation_alpha).unwrap_or(1.0);

        let mut shadow_meshes = ExtractedShadowMeshes::default();

        for (mesh, transform, interp, mat, technique, aabb) in world
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

        r_world.insert(shadow_meshes);
    }
    fn name(&self) -> &'static str {
        "extract_shadow_meshes"
    }
}
