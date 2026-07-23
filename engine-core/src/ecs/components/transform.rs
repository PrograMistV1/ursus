use engine_macros::Component;
use glam::{Mat4, Quat, Vec3};

#[derive(Debug, Copy, Clone, Component)]
pub struct Transform {
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Transform {
    pub fn identity() -> Self {
        Self { position: Vec3::ZERO, rotation: Quat::IDENTITY, scale: Vec3::ONE }
    }

    pub fn at(x: f32, y: f32, z: f32) -> Self {
        Self { position: Vec3::new(x, y, z), ..Self::identity() }
    }

    pub fn with_scale(mut self, s: f32) -> Self {
        self.scale = Vec3::splat(s);
        self
    }

    pub fn with_rotation(mut self, rotation: Quat) -> Self {
        self.rotation = rotation;
        self
    }

    pub fn matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.position)
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::identity()
    }
}
