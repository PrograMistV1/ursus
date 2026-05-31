use super::components::{MaterialHandle, MeshHandle, Transform};
use super::world::GameWorld;
use crate::assets::shader_registry::ShaderHandle;
use crate::assets::AssetServer;

pub struct DrawCall {
    pub mesh: MeshHandle,
    pub material: Option<MaterialHandle>,
    pub shader: ShaderHandle,
    pub transform: Transform,
}

pub fn collect_draw_calls(world: &mut GameWorld, assets: &AssetServer) -> Vec<DrawCall> {
    let default_shader = assets.shaders.diffuse();

    let mut calls = Vec::new();

    for (mesh, transform, mat) in world
        .inner
        .query_mut::<(&MeshHandle, &Transform, Option<&MaterialHandle>)>()
    {
        let shader = mat
            .and_then(|m| assets.get_material(*m))
            .map(|mat_def| mat_def.shader)
            .unwrap_or(default_shader);

        calls.push(DrawCall {
            mesh: *mesh,
            material: mat.copied(),
            shader,
            transform: transform.clone(),
        });
    }

    calls
}
