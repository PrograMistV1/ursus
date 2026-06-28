use crate::assets::upload::GpuUploadRequest;
use crate::assets::CpuAssetServer;
use crate::components::camera::{ActiveCamera, CameraComponent};
use crate::render::extract::ExtractSystem;
use crate::render::world::{ExtractedCamera, ExtractedRenderSettings, RenderWorld};
use crate::GameWorld;
use std::sync::mpsc::Sender;

pub struct CameraExtract;
impl ExtractSystem for CameraExtract {
    fn extract(
        &self,
        world: &GameWorld,
        rw: &mut RenderWorld,
        _cpu_assets: &mut CpuAssetServer,
        _upload_tx: &Sender<GpuUploadRequest>,
    ) {
        let camera = world
            .inner
            .query::<(&CameraComponent, &ActiveCamera)>()
            .iter()
            .next()
            .map(|(cam, _)| cam.clone())
            .unwrap_or_default();

        let aspect =
            rw.get::<ExtractedRenderSettings>().map(|s| s.output_size.0 / s.output_size.1).unwrap_or(16.0 / 9.0);

        let view = glam::Mat4::look_at_rh(camera.eye, camera.target, camera.up);
        let mut proj = glam::Mat4::perspective_rh(camera.fov_y, aspect, camera.z_near, camera.z_far);
        proj.y_axis.y *= -1.0;

        rw.insert(ExtractedCamera { eye: camera.eye, view, proj, view_proj: proj * view });
    }
    fn name(&self) -> &'static str {
        "extract_camera"
    }
}
