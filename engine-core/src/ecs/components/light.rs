use crate::ecs::Component;
use glam::Vec3;

pub const DEFAULT_LIGHT_DIRECTION: Vec3 = Vec3::new(-0.3, -1.0, -0.2);
pub const DEFAULT_LIGHT_COLOR: [f32; 4] = [1.0, 0.95, 0.85, 2.0];

#[derive(Debug, Clone, Copy)]
pub struct DirectionalLightComponent {
    pub direction: Vec3,
    pub color: [f32; 4], // rgb + intensity в alpha
}

impl Component for DirectionalLightComponent {}
impl Default for DirectionalLightComponent {
    fn default() -> Self {
        Self { direction: DEFAULT_LIGHT_DIRECTION, color: DEFAULT_LIGHT_COLOR }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PointLightComponent {
    pub position: Vec3,
    pub radius: f32,
    pub color: [f32; 4], // rgb + intensity
}
