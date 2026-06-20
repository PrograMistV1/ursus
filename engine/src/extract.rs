use crate::assets::gpu_server::GpuAssetServer;
use crate::ecs::components::{
    ActiveCamera, CameraComponent, DirectionalLightComponent, MaterialHandle, MeshHandle, PointLightComponent,
    Transform, UiLayout, UiRect, UiText,
};
use crate::ecs::GameWorld;
use crate::lighting::buffer::{GpuPointLight, MAX_POINT_LIGHTS};
use crate::math::frustum::{extract_planes, transform_aabb};
use crate::render_world::{
    ExtractedCamera, ExtractedInstance, ExtractedLights, ExtractedMeshes, ExtractedRenderSettings,
    ExtractedShadowMeshes, ExtractedUiRect, ExtractedUiRects, ExtractedUiText, ExtractedUiTexts, RenderWorld,
};
use glam::Vec2;

/// Функция извлечения данных из ECS в RenderWorld.
///
/// Читает `GameWorld`, пишет в `RenderWorld`. Не имеет собственного состояния —
/// всё состояние хранится в компонентах `GameWorld`.
pub type ExtractFn = Box<dyn Fn(&GameWorld, &GpuAssetServer, &mut RenderWorld) + Send + Sync>;

/// Список систем извлечения. Выполняется целиком в главном потоке раз за кадр.
///
/// # Порядок вызова
///
/// ```ignore
/// render_world.clear();
/// extract_schedule.run(&game_world, &gpu_assets, render_world);
/// triple_buffer.publish();
/// ```
///
/// # Расширяемость
///
/// Пользователь регистрирует свои системы через [`ExtractSchedule::add`]:
///
/// ```ignore
/// ctx.extract_schedule.add(|world, _gpu, rw| {
///     let particles = world.inner.query::<&ParticleEmitter>()
///         .iter()
///         .map(|e| ExtractedParticle::from(e))
///         .collect();
///     rw.insert(ExtractedParticles(particles));
/// });
/// ```
pub struct ExtractSchedule {
    systems: Vec<ExtractFn>,
}

impl ExtractSchedule {
    pub fn new() -> Self {
        Self { systems: Vec::new() }
    }

    /// Зарегистрировать extract систему.
    /// Системы выполняются в порядке регистрации.
    pub fn add<F>(&mut self, f: F)
    where
        F: Fn(&GameWorld, &GpuAssetServer, &mut RenderWorld) + Send + Sync + 'static,
    {
        self.systems.push(Box::new(f));
    }

    /// Выполнить все системы последовательно.
    pub fn run(&self, world: &GameWorld, gpu_assets: &GpuAssetServer, dst: &mut RenderWorld) {
        for system in &self.systems {
            system(world, gpu_assets, dst);
        }
    }
}

impl Default for ExtractSchedule {
    fn default() -> Self {
        Self::new()
    }
}

// ── Базовые extract системы ──────────────────────────────────────────────────
//
// Регистрируются автоматически при создании EngineContext.
// Пользовательские системы добавляются поверх через ExtractSchedule::add.

/// Извлечь камеру.
pub fn extract_camera(world: &GameWorld, _gpu: &GpuAssetServer, rw: &mut RenderWorld) {
    let camera = world
        .inner
        .query::<(&CameraComponent, &ActiveCamera)>()
        .iter()
        .next()
        .map(|(cam, _)| cam.clone())
        .unwrap_or_default();

    // Aspect пересчитывается из output_size если он уже вставлен,
    // иначе используем дефолт 16/9.
    let aspect = rw.get::<ExtractedRenderSettings>().map(|s| s.output_size.0 / s.output_size.1).unwrap_or(16.0 / 9.0);

    let view = glam::Mat4::look_at_rh(camera.eye, camera.target, camera.up);
    let mut proj = glam::Mat4::perspective_rh(camera.fov_y, aspect, camera.z_near, camera.z_far);
    proj.y_axis.y *= -1.0;

    rw.insert(ExtractedCamera { eye: camera.eye, view, proj, view_proj: proj * view });
}

/// Извлечь меши — с frustum culling для основного прохода и без для теней.
pub fn extract_meshes(world: &GameWorld, gpu: &GpuAssetServer, rw: &mut RenderWorld) {
    let view_proj = rw.get::<ExtractedCamera>().map(|c| c.view_proj).unwrap_or(glam::Mat4::IDENTITY);

    let frustum = extract_planes(view_proj);

    let mut meshes = ExtractedMeshes::default();
    let mut shadow_meshes = ExtractedShadowMeshes::default();

    for (mesh, transform, mat) in world.inner.query::<(&MeshHandle, &Transform, Option<&MaterialHandle>)>().iter() {
        let Some(gpu_mesh) = gpu.get_gpu_mesh(*mesh) else {
            continue;
        };
        let model = transform.matrix();

        let instance = ExtractedInstance { mesh: *mesh, material: mat.copied(), model };

        shadow_meshes.instances.push(instance.clone());

        if transform_aabb(&gpu_mesh.aabb, model).intersects_frustum(&frustum) {
            meshes.instances.push(instance);
        }
    }

    rw.insert(meshes);
    rw.insert(shadow_meshes);
}

/// Извлечь освещение.
pub fn extract_lights(world: &GameWorld, _gpu: &GpuAssetServer, rw: &mut RenderWorld) {
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

/// Извлечь UI элементы.
pub fn extract_ui(world: &GameWorld, gpu: &GpuAssetServer, rw: &mut RenderWorld) {
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

    let font_atlas = gpu.font_atlas.as_ref();
    for (layout, text) in world.inner.query::<(&UiLayout, &UiText)>().iter() {
        let (text_width, line_height) = font_atlas
            .map(|a| (a.measure_text(&text.text, text.font_size as u32), a.line_height(text.font_size as u32)))
            .unwrap_or((0.0, text.font_size));

        let pos = Vec2::new(
            layout.anchor.x * screen_w + layout.offset.x - layout.pivot.x * text_width,
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

/// Создать `ExtractSchedule` с базовыми системами движка.
///
/// Порядок важен: `extract_camera` должна быть первой так как
/// `extract_meshes` и `extract_ui` читают `ExtractedCamera` и
/// `ExtractedRenderSettings` из `RenderWorld`.
pub fn default_extract_schedule() -> ExtractSchedule {
    let mut schedule = ExtractSchedule::new();
    schedule.add(extract_camera);
    schedule.add(extract_meshes);
    schedule.add(extract_lights);
    schedule.add(extract_ui);
    schedule
}
