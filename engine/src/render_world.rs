use crate::ecs::components::{MaterialHandle, MeshHandle};
use crate::lighting::buffer::{DirectionalLight, GpuPointLight, MAX_POINT_LIGHTS};
use glam::{Mat4, Vec2, Vec3};

pub struct RenderInstance {
    pub mesh: MeshHandle,
    pub material: Option<MaterialHandle>,
    pub model: Mat4,
}

pub struct RenderCamera {
    pub eye: Vec3,
    pub view: Mat4,
    pub proj: Mat4,
    pub view_proj: Mat4,
}

impl Default for RenderCamera {
    fn default() -> Self {
        Self { eye: Vec3::ZERO, view: Mat4::IDENTITY, proj: Mat4::IDENTITY, view_proj: Mat4::IDENTITY }
    }
}

pub struct RenderLighting {
    pub directional: DirectionalLight,
    pub point_lights: [GpuPointLight; MAX_POINT_LIGHTS],
    pub point_light_count: u32,
}

impl Default for RenderLighting {
    fn default() -> Self {
        Self {
            directional: DirectionalLight { direction: [-0.3, -1.0, -0.2, 0.0], color: [1.0, 0.95, 0.85, 2.0] },
            point_lights: [GpuPointLight { position: [0.0; 4], color: [0.0; 4] }; MAX_POINT_LIGHTS],
            point_light_count: 0,
        }
    }
}

pub struct RenderUiRect {
    pub pos: Vec2,
    pub size: Vec2,
    pub color: [f32; 4],
}

pub struct RenderUiText {
    pub pos: Vec2,
    pub text: String,
    pub font_size: f32,
    pub color: [f32; 4],
}

/// Плоский снэпшот сцены для одного кадра рендера.
/// Не содержит ссылок на GameWorld/hecs — годен для передачи в другой поток.
pub struct RenderWorld {
    pub instances: Vec<RenderInstance>,
    pub shadow_instances: Vec<RenderInstance>,
    pub camera: RenderCamera,
    pub light_view_proj: Mat4,
    pub lighting: RenderLighting,
    pub ui_rects: Vec<RenderUiRect>,
    pub ui_texts: Vec<RenderUiText>,
}

impl Default for RenderWorld {
    fn default() -> Self {
        Self {
            instances: Vec::new(),
            shadow_instances: Vec::new(),
            camera: RenderCamera::default(),
            light_view_proj: Mat4::IDENTITY,
            lighting: RenderLighting::default(),
            ui_rects: Vec::new(),
            ui_texts: Vec::new(),
        }
    }
}
