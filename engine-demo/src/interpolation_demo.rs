use engine_core::app::{App, Engine, EngineContext};
use engine_core::assets::CpuMesh;
use engine_core::components::camera::{ActiveCamera, CameraComponent};
use engine_core::components::light::DirectionalLightComponent;
use engine_core::components::mesh::TechniqueHandle;
use engine_core::components::transform::Transform;
use engine_core::components::transform_interpolation::TransformInterpolation;
use engine_core::ecs::world::Entity;
use engine_core::render::gfx::types::Format;
use engine_core::render::thread::command::PipelineFactory;
use engine_gltf_loader::PbrMetallicRoughness;
use engine_pipelines::DefaultPipeline;
use glam::{Quat, Vec3};
use std::f32::consts::PI;

mod text_texture;

const ROT_SPEED: f32 = PI * 0.2;

struct InterpolationDemoApp {
    interpolated_cube: Option<Entity>,
    plain_cube: Option<Entity>,
    angle: f32,
}

impl InterpolationDemoApp {
    fn new() -> Self {
        Self { interpolated_cube: None, plain_cube: None, angle: 0.0 }
    }
}

impl App for InterpolationDemoApp {
    fn initial_pipeline() -> PipelineFactory
    where
        Self: Sized,
    {
        PipelineFactory::of::<DefaultPipeline>()
    }

    fn tick_rate(&self) -> f32 {
        5.0
    }

    fn on_start(&mut self, ctx: &mut EngineContext) {
        ctx.world.spawn().insert(CameraComponent::default()).insert(ActiveCamera).build();
        ctx.world.spawn().insert(DirectionalLightComponent::default()).build();

        let cube_mesh = ctx.asset_registry.upload_mesh(CpuMesh::cube());

        let interpolated_material = {
            let (pixels, w, h) = text_texture::render_label_texture(&["Interpolated", "Diffuse"], "#2a4d69", "#ffffff")
                .expect("failed to generate texture Interpolated");
            let tex = ctx.asset_registry.upload_texture_rgba8(pixels, w, h, Format::Rgba8Srgb, "label_interpolated");
            ctx.asset_registry.register_material(
                Box::new(PbrMetallicRoughness {
                    name: "interpolated_label".into(),
                    base_color: [1.0, 1.0, 1.0, 1.0],
                    metallic: 0.0,
                    roughness: 0.8,
                    emissive: [0.0; 3],
                }),
                vec![("base_color".to_string(), tex)],
            )
        };

        let no_interpolated_material = {
            let (pixels, w, h) = text_texture::render_label_texture(&["NoInterpolated", "Unlit"], "#8a3d2a", "#ffffff")
                .expect("failed to generate texture NoInterpolated");
            let tex = ctx.asset_registry.upload_texture_rgba8(pixels, w, h, Format::Rgba8Srgb, "label_no_interpolated");
            ctx.asset_registry.register_material(
                Box::new(PbrMetallicRoughness {
                    name: "no_interpolated_label".into(),
                    base_color: [1.0, 1.0, 1.0, 1.0],
                    metallic: 0.0,
                    roughness: 0.8,
                    emissive: [0.0; 3],
                }),
                vec![("base_color".to_string(), tex)],
            )
        };

        let interpolated = ctx
            .world
            .spawn()
            .insert(cube_mesh)
            .insert(interpolated_material)
            .insert(Transform::at(-1.5, 2.0, -3.0))
            .insert(TransformInterpolation::default())
            .build();
        self.interpolated_cube = Some(interpolated);

        let plain = ctx
            .world
            .spawn()
            .insert(cube_mesh)
            .insert(no_interpolated_material)
            .insert(Transform::at(1.5, 2.0, -3.0))
            .insert(TechniqueHandle("unlit".into()))
            .build();
        self.plain_cube = Some(plain);
    }

    fn on_update(&mut self, ctx: &mut EngineContext, dt: f32) {
        self.angle += ROT_SPEED * dt;
        let rotation = Quat::from_rotation_y(self.angle);

        if let Some(e) = self.interpolated_cube {
            if let Ok(mut t) = ctx.world.inner.get::<&mut Transform>(e) {
                t.rotation = rotation;
            }
        }
        if let Some(e) = self.plain_cube {
            if let Ok(mut t) = ctx.world.inner.get::<&mut Transform>(e) {
                t.rotation = rotation;
            }
        }

        for (cam, _) in ctx.world.inner.query_mut::<(&mut CameraComponent, &ActiveCamera)>() {
            cam.eye = Vec3::new(0.0, 2.5, -6.0);
            cam.target = Vec3::new(0.0, 1.0, 0.0);
        }
    }

    fn on_render(&mut self, _ctx: &mut EngineContext) {}

    fn on_stop(&mut self, _ctx: &mut EngineContext) {}
}

fn main() -> anyhow::Result<()> {
    Engine::run(InterpolationDemoApp::new())
}
