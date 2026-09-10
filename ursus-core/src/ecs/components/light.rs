use engine_macros::Component;
use glam::Vec3;

pub const DEFAULT_LIGHT_DIRECTION: Vec3 = Vec3::new(-0.3, -1.0, -0.2);
pub const DEFAULT_LIGHT_COLOR: [f32; 4] = [1.0, 0.95, 0.85, 2.0];

#[derive(Debug, Clone, Copy, Component)]
pub struct DirectionalLightComponent {
    pub direction: Vec3,
    pub color: [f32; 4], // rgb + intensity в alpha
}

impl Default for DirectionalLightComponent {
    fn default() -> Self {
        Self { direction: DEFAULT_LIGHT_DIRECTION, color: DEFAULT_LIGHT_COLOR }
    }
}

#[derive(Debug, Clone, Copy, Component)]
pub struct PointLightComponent {
    pub position: Vec3,
    pub radius: f32,
    pub color: [f32; 4], // rgb + intensity
}

impl Default for PointLightComponent {
    fn default() -> Self {
        PointLightComponent { position: Vec3::ZERO, radius: 1.0, color: [1.0; 4] }
    }
}
