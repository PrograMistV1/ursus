use crate::assets::mesh::Aabb;
use crate::ecs::components::{
    ActiveCamera, CameraComponent, DirectionalLightComponent, MaterialHandle, MeshHandle, PointLightComponent,
    Transform, UiLayout, UiRect, UiText,
};
use crate::ecs::GameWorld;
use crate::lighting::buffer::{GpuPointLight, MAX_POINT_LIGHTS};
use crate::render_world::{
    ExtractedCamera, ExtractedInstance, ExtractedLights, ExtractedMeshes, ExtractedRenderSettings,
    ExtractedShadowMeshes, ExtractedUiRect, ExtractedUiRects, ExtractedUiText, ExtractedUiTexts, RenderWorld,
};
use glam::Vec2;

pub type ExtractFn = Box<dyn Fn(&GameWorld, &mut RenderWorld) + Send + Sync>;

pub struct ExtractSchedule {
    systems: Vec<ExtractFn>,
}

impl ExtractSchedule {
    pub fn new() -> Self {
        Self { systems: Vec::new() }
    }

    pub fn add<F>(&mut self, f: F)
    where
        F: Fn(&GameWorld, &mut RenderWorld) + Send + Sync + 'static,
    {
        self.systems.push(Box::new(f));
    }

    pub fn run(&self, world: &GameWorld, dst: &mut RenderWorld) {
        for system in &self.systems {
            system(world, dst);
        }
    }
}

impl Default for ExtractSchedule {
    fn default() -> Self {
        Self::new()
    }
}

pub fn extract_camera(world: &GameWorld, rw: &mut RenderWorld) {
    let camera = world
        .inner
        .query::<(&CameraComponent, &ActiveCamera)>()
        .iter()
        .next()
        .map(|(cam, _)| cam.clone())
        .unwrap_or_default();

    let aspect = rw.get::<ExtractedRenderSettings>().map(|s| s.output_size.0 / s.output_size.1).unwrap_or(16.0 / 9.0);

    let view = glam::Mat4::look_at_rh(camera.eye, camera.target, camera.up);
    let mut proj = glam::Mat4::perspective_rh(camera.fov_y, aspect, camera.z_near, camera.z_far);
    proj.y_axis.y *= -1.0;

    rw.insert(ExtractedCamera { eye: camera.eye, view, proj, view_proj: proj * view });
}

pub fn extract_meshes(world: &GameWorld, rw: &mut RenderWorld) {
    let mut meshes = ExtractedMeshes::default();
    let mut shadow_meshes = ExtractedShadowMeshes::default();

    for (mesh, transform, mat, aabb) in
        world.inner.query::<(&MeshHandle, &Transform, Option<&MaterialHandle>, Option<&Aabb>)>().iter()
    {
        let model = transform.matrix();
        let instance = ExtractedInstance { mesh: *mesh, material: mat.copied(), model, aabb: aabb.copied() };

        shadow_meshes.instances.push(instance.clone());
        meshes.instances.push(instance);
    }

    rw.insert(meshes);
    rw.insert(shadow_meshes);
}

pub fn extract_lights(world: &GameWorld, rw: &mut RenderWorld) {
    let directional = world
        .inner
        .query::<&DirectionalLightComponent>()
        .iter()
        .next()
        .map(|light| crate::lighting::buffer::DirectionalLight {
            direction: [light.direction.x, light.direction.y, light.direction.z, 0.0],
            color: light.color,
        })
        .unwrap_or(crate::lighting::buffer::DirectionalLight {
            direction: [-0.3, -1.0, -0.2, 0.0],
            color: [1.0, 0.95, 0.85, 2.0],
        });

    let mut point_lights = [GpuPointLight { position: [0.0; 4], color: [0.0; 4] }; MAX_POINT_LIGHTS];
    let mut point_light_count = 0u32;

    for light in world.inner.query::<&PointLightComponent>().iter() {
        if point_light_count as usize >= MAX_POINT_LIGHTS {
            break;
        }
        point_lights[point_light_count as usize] = GpuPointLight {
            position: [light.position.x, light.position.y, light.position.z, light.radius],
            color: light.color,
        };
        point_light_count += 1;
    }

    let light_dir = glam::Vec3::new(directional.direction[0], directional.direction[1], directional.direction[2]);
    let light_view_proj =
        crate::lighting::compute_light_view_proj(light_dir.into(), glam::Vec3::new(0.0, 2.0, 0.0), 20.0);

    rw.insert(ExtractedLights { directional, point_lights, point_light_count, light_view_proj });
}

pub fn extract_ui(world: &GameWorld, rw: &mut RenderWorld) {
    let (screen_w, screen_h) = rw.get::<ExtractedRenderSettings>().map(|s| s.output_size).unwrap_or((1280.0, 720.0));

    let mut ui_rects = ExtractedUiRects::default();
    let mut ui_texts = ExtractedUiTexts::default();

    for (layout, rect) in world.inner.query::<(&UiLayout, &UiRect)>().iter() {
        let pos = Vec2::new(
            layout.anchor.x * screen_w + layout.offset.x - layout.pivot.x * rect.size.x,
            layout.anchor.y * screen_h + layout.offset.y - layout.pivot.y * rect.size.y,
        );
        ui_rects.rects.push(ExtractedUiRect { pos, size: rect.size, color: rect.color });
    }

    for (layout, text) in world.inner.query::<(&UiLayout, &UiText)>().iter() {
        let line_height = text.font_size * 1.2;
        let pos = Vec2::new(
            layout.anchor.x * screen_w + layout.offset.x,
            layout.anchor.y * screen_h + layout.offset.y - layout.pivot.y * line_height,
        );
        ui_texts.texts.push(ExtractedUiText {
            pos,
            text: text.text.clone(),
            font_size: text.font_size,
            color: text.color,
        });
    }

    rw.insert(ui_rects);
    rw.insert(ui_texts);
}

pub fn default_extract_schedule() -> ExtractSchedule {
    let mut schedule = ExtractSchedule::new();
    schedule.add(extract_camera);
    schedule.add(extract_meshes);
    schedule.add(extract_lights);
    schedule.add(extract_ui);
    schedule
}
