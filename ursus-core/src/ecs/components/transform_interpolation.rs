use crate::ecs::components::transform::Transform;
use engine_macros::Component;
use glam::{Mat4, Quat, Vec3};

#[derive(Debug, Clone, Copy, Component)]
#[requires(Transform)]
pub struct TransformInterpolation {
    prev_position: Vec3,
    prev_rotation: Quat,
    prev_scale: Vec3,
}

impl TransformInterpolation {
    pub fn sync(&mut self, current: &Transform) {
        self.prev_position = current.position;
        self.prev_rotation = current.rotation;
        self.prev_scale = current.scale;
    }

    pub fn interpolate(&self, current: &Transform, alpha: f32) -> Mat4 {
        let position = self.prev_position.lerp(current.position, alpha);
        let rotation = self.prev_rotation.slerp(current.rotation, alpha);
        let scale = self.prev_scale.lerp(current.scale, alpha);
        Mat4::from_scale_rotation_translation(scale, rotation, position)
    }
}

impl Default for TransformInterpolation {
    fn default() -> Self {
        Self { prev_position: Vec3::ZERO, prev_rotation: Quat::IDENTITY, prev_scale: Vec3::ONE }
    }
}
