use engine_macros::Component;
use glam::camera::rh::proj::directx::perspective;
use glam::camera::rh::view::look_at_mat4;
use glam::{Mat4, Vec3};

#[derive(Debug, Clone, Copy, Component)]
pub struct ActiveCamera; // todo components cannot be added or removed at runtime, move as a field CameraComponent

impl Default for ActiveCamera {
    fn default() -> Self {
        ActiveCamera
    }
}

#[derive(Debug, Clone, Component)]
pub struct CameraComponent {
    pub eye: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fov_y: f32,
    pub z_near: f32,
    pub z_far: f32,
}

impl CameraComponent {
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let view = look_at_mat4(self.eye, self.target, self.up);
        let mut proj = perspective(self.fov_y, aspect, self.z_near, self.z_far);
        proj.y_axis.y *= -1.0;
        proj * view
    }
}

impl Default for CameraComponent {
    fn default() -> Self {
        Self {
            eye: Vec3::new(2.0, 2.0, 3.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            fov_y: 60_f32.to_radians(),
            z_near: 0.1,
            z_far: 100.0,
        }
    }
}
