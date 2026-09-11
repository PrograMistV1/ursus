use crate::render::extract::ExtractSystem;
use crate::render::world::{ExtractedCamera, ExtractedRenderSettings, RWorld};
use glam::camera::rh::proj::vulkan::perspective;
use glam::camera::rh::view::look_at_mat4;
use ursus_ecs::components::camera::{ActiveCamera, CameraComponent};
use ursus_ecs::World;

pub struct CameraExtract;
impl ExtractSystem for CameraExtract {
    fn extract(&self, world: &World, r_world: &mut RWorld) {
        let camera = world
            .query::<(&CameraComponent, &ActiveCamera)>()
            .iter()
            .next()
            .map(|(cam, _)| cam.clone())
            .unwrap_or_default();

        let aspect =
            r_world.get::<ExtractedRenderSettings>().map(|s| s.output_size.0 / s.output_size.1).unwrap_or(16.0 / 9.0);

        let view = look_at_mat4(camera.eye, camera.target, camera.up);
        let proj = perspective(camera.fov_y, aspect, camera.z_near, camera.z_far);

        r_world.insert(ExtractedCamera { eye: camera.eye, view, proj, view_proj: proj * view });
    }
    fn name(&self) -> &'static str {
        "extract_camera"
    }
}
