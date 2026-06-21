use crate::assets::mesh::Aabb;
use crate::components::mesh::{MaterialHandle, MeshHandle};
use crate::components::transform::Transform;
use crate::render::extract::ExtractSystem;
use crate::render::world::{ExtractedInstance, ExtractedMeshes, ExtractedShadowMeshes, RenderWorld};
use crate::GameWorld;

pub struct MeshExtract;
impl ExtractSystem for MeshExtract {
    fn extract(&self, world: &GameWorld, rw: &mut RenderWorld) {
        let mut meshes = ExtractedMeshes::default();
        let mut shadow_meshes = ExtractedShadowMeshes::default();

        for (mesh, transform, mat, aabb) in
            world.inner.query::<(&MeshHandle, &Transform, Option<&MaterialHandle>, Option<&Aabb>)>().iter()
        {
            let model = transform.matrix();
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
