use glam::{Mat4, Quat, Vec2, Vec3};

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MeshHandle(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MaterialHandle(pub u32);

#[derive(Debug, Clone)]
pub struct Name(pub String);

impl Name {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

#[derive(Debug, Clone)]
pub struct UiLayout {
    pub anchor: Vec2,
    pub pivot: Vec2,
    pub offset: Vec2,
}

impl UiLayout {
    pub fn top_left(offset: Vec2) -> Self {
        Self { anchor: Vec2::ZERO, pivot: Vec2::ZERO, offset }
    }
    pub fn center() -> Self {
        Self { anchor: Vec2::splat(0.5), pivot: Vec2::splat(0.5), offset: Vec2::ZERO }
    }
    pub fn top_right(offset: Vec2) -> Self {
        Self { anchor: Vec2::new(1.0, 0.0), pivot: Vec2::new(1.0, 0.0), offset }
    }
}

#[derive(Debug, Clone)]
pub struct UiText {
    pub text: String,
    pub font_size: f32,
    pub color: [f32; 4],
}

impl UiText {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into(), font_size: 14.0, color: [1.0; 4] }
    }
    pub fn with_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }
    pub fn with_color(mut self, color: [f32; 4]) -> Self {
        self.color = color;
        self
    }
}

#[derive(Debug, Clone)]
pub struct UiRect {
    pub size: Vec2,
    pub color: [f32; 4],
    pub border_radius: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct ActiveCamera;

#[derive(Debug, Clone)]
pub struct CameraComponent {
    pub eye: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fov_y: f32,
    pub z_near: f32,
    pub z_far: f32,
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

impl CameraComponent {
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let view = Mat4::look_at_rh(self.eye, self.target, self.up);
        let mut proj = Mat4::perspective_rh(self.fov_y, aspect, self.z_near, self.z_far);
        proj.y_axis.y *= -1.0;
        proj * view
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DirectionalLightComponent {
    pub direction: Vec3,
    pub color: [f32; 4], // rgb + intensity в alpha
}

impl Default for DirectionalLightComponent {
    fn default() -> Self {
        Self { direction: Vec3::new(-0.3, -1.0, -0.2), color: [1.0, 0.95, 0.85, 2.0] }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PointLightComponent {
    pub position: Vec3,
    pub radius: f32,
    pub color: [f32; 4], // rgb + intensity
}
