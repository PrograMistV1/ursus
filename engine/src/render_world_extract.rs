use crate::assets::gpu_server::GpuAssetServer;
use crate::ecs::components::{
    ActiveCamera, CameraComponent, DirectionalLightComponent, MaterialHandle, MeshHandle, PointLightComponent,
    Transform, UiLayout, UiRect, UiText,
};
use crate::ecs::GameWorld;
use crate::lighting::buffer::{GpuPointLight, MAX_POINT_LIGHTS};
use crate::math::frustum::{extract_planes, transform_aabb};
use crate::render_world::{RenderCamera, RenderInstance, RenderLighting, RenderUiRect, RenderUiText, RenderWorld};
use glam::{Mat4, Vec2};

pub fn extract_render_world(
    world: &GameWorld,
    gpu_assets: &GpuAssetServer,
    output_size: (f32, f32),
    aspect: f32,
) -> RenderWorld {
    let camera_comp = world
        .inner
        .query::<(&CameraComponent, &ActiveCamera)>()
        .iter()
        .next()
        .map(|(cam, _)| cam.clone())
        .unwrap_or_default();

    let view = Mat4::look_at_rh(camera_comp.eye, camera_comp.target, camera_comp.up);
    let mut proj = Mat4::perspective_rh(camera_comp.fov_y, aspect, camera_comp.z_near, camera_comp.z_far);
    proj.y_axis.y *= -1.0;
    let view_proj = proj * view;
    let frustum = extract_planes(view_proj);

    let mut instances = Vec::new();
    let mut shadow_instances = Vec::new();

    for (mesh, transform, mat) in world.inner.query::<(&MeshHandle, &Transform, Option<&MaterialHandle>)>().iter() {
        let Some(gpu) = gpu_assets.get_gpu_mesh(*mesh) else {
            continue;
        };
        let model = transform.matrix();

        // shadow caster — без culling по камере, попадает всегда (теневой проход видит сцену иначе)
        shadow_instances.push(RenderInstance { mesh: *mesh, material: mat.copied(), model });

        if !transform_aabb(&gpu.aabb, model).intersects_frustum(&frustum) {
            continue;
        }
        instances.push(RenderInstance { mesh: *mesh, material: mat.copied(), model });
    }

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

    let (screen_w, screen_h) = output_size;
    let mut ui_rects = Vec::new();
    let mut ui_texts = Vec::new();

    for (layout, rect) in world.inner.query::<(&UiLayout, &UiRect)>().iter() {
        let pos = Vec2::new(
            layout.anchor.x * screen_w + layout.offset.x - layout.pivot.x * rect.size.x,
            layout.anchor.y * screen_h + layout.offset.y - layout.pivot.y * rect.size.y,
        );
        ui_rects.push(RenderUiRect { pos, size: rect.size, color: rect.color });
    }

    let font_atlas = gpu_assets.font_atlas.as_ref();
    for (layout, text) in world.inner.query::<(&UiLayout, &UiText)>().iter() {
        let (text_width, line_height) = font_atlas
            .map(|a| (a.measure_text(&text.text, text.font_size as u32), a.line_height(text.font_size as u32)))
            .unwrap_or((0.0, text.font_size));

        let pos = Vec2::new(
            layout.anchor.x * screen_w + layout.offset.x - layout.pivot.x * text_width,
            layout.anchor.y * screen_h + layout.offset.y - layout.pivot.y * line_height,
        );
        ui_texts.push(RenderUiText { pos, text: text.text.clone(), font_size: text.font_size, color: text.color });
    }

    RenderWorld {
        instances,
        shadow_instances,
        camera: RenderCamera { eye: camera_comp.eye, view, proj, view_proj },
        light_view_proj,
        lighting: RenderLighting { directional, point_lights, point_light_count },
        ui_rects,
        ui_texts,
    }
}
