use glam::{Quat, Vec3, Vec4};
use std::f32::consts::PI;
use ursus_core::app::{App, Engine, EngineContext};
use ursus_core::assets::CpuMesh;
use ursus_core::render::thread::command::PipelineFactory;
use ursus_ecs::components::camera::{ActiveCamera, CameraComponent};
use ursus_ecs::components::light::DirectionalLightComponent;
use ursus_ecs::components::transform::Transform;
use ursus_ecs::components::transform_interpolation::TransformInterpolation;
use ursus_ecs::Entity;
use ursus_materials::strategies::properties::BASE_COLOR;
use ursus_materials::{Material, MaterialValue, PropertyId};
use ursus_pipelines::DefaultPipeline;

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
        ctx.world.spawn((CameraComponent::default(), ActiveCamera));
        ctx.world.spawn((DirectionalLightComponent::default(),));

        let cube_mesh = ctx.asset_registry.meshes.upload(CpuMesh::cube());

        let red_material = ctx.asset_registry.materials.insert(
            Material::new("interpolated_cube_material", "pbr")
                .with(BASE_COLOR, MaterialValue::Color(Vec4::new(0.9, 0.2, 0.2, 1.0)))
                .with(PropertyId::new("metallic"), MaterialValue::Float(0.1))
                .with(PropertyId::new("roughness"), MaterialValue::Float(0.6)),
        );

        let blue_material = ctx.asset_registry.materials.insert(
            Material::new("plain_cube_material", "pbr")
                .with(BASE_COLOR, MaterialValue::Color(Vec4::new(0.2, 0.4, 0.9, 1.0)))
                .with(PropertyId::new("metallic"), MaterialValue::Float(0.0))
                .with(PropertyId::new("roughness"), MaterialValue::Float(0.9)),
        );

        let interpolated = ctx.world.spawn((
            cube_mesh,
            Transform::at(-1.5, 2.0, -3.0),
            TransformInterpolation::default(),
            red_material,
        ));
        self.interpolated_cube = Some(interpolated);

        let plain = ctx.world.spawn((cube_mesh, Transform::at(1.5, 2.0, -3.0), blue_material));
        self.plain_cube = Some(plain);
    }

    fn on_update(&mut self, ctx: &mut EngineContext, dt: f32) {
        self.angle += ROT_SPEED * dt;
        let rotation = Quat::from_rotation_y(self.angle);

        if let Some(e) = self.interpolated_cube {
            if let Ok(mut t) = ctx.world.get::<&mut Transform>(e) {
                t.rotation = rotation;
            }
        }
        if let Some(e) = self.plain_cube {
            if let Ok(mut t) = ctx.world.get::<&mut Transform>(e) {
                t.rotation = rotation;
            }
        }

        for (cam, _) in ctx.world.query_mut::<(&mut CameraComponent, &ActiveCamera)>() {
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
