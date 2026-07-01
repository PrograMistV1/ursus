use engine_core::components::camera::{ActiveCamera, CameraComponent};
use engine_core::components::light::DirectionalLightComponent;
use engine_core::components::ui::{UiLayout, UiText};
use engine_core::render::thread::command::PipelineFactory;
use engine_core::{App, AsyncMeshHandle, Engine, EngineContext};
use engine_default::{DefaultPipeline, LoadingPipeline};
use glam::{Vec2, Vec3};

struct MyApp {
    sponza: Option<AsyncMeshHandle>,
    spawned: bool,
    frame: u64,
}

impl MyApp {
    fn new() -> Self {
        Self { sponza: None, spawned: false, frame: 0 }
    }
}

impl App for MyApp {
    fn initial_pipeline() -> PipelineFactory
    where
        Self: Sized,
    {
        PipelineFactory::of::<LoadingPipeline>()
    }

    fn on_start(&mut self, ctx: &mut EngineContext) {
        self.sponza = Some(ctx.cpu_assets.load_mesh_async("assets/sponza/glTF/Sponza.gltf"));

        ctx.world
            .spawn()
            .insert(UiLayout::top_left(Vec2::new(16.0, 16.0)))
            .insert(UiText::new("FPS: 60").with_size(18.0).with_color([1.0; 4]))
            .build();
        ctx.world.spawn().insert(CameraComponent::default()).insert(ActiveCamera).build();
        ctx.world.spawn().insert(DirectionalLightComponent::default()).build();
    }

    fn on_update(&mut self, ctx: &mut EngineContext, _dt: f32) {
        self.frame += 1;

        if !self.spawned && !ctx.cpu_assets.is_loading() {
            if let Some(handle) = &self.sponza {
                for (mesh, mat, transform, _) in ctx.cpu_assets.get_mesh_instances(handle).unwrap() {
                    let mut builder = ctx.world.spawn();
                    builder = builder.insert(mesh);
                    builder = builder.insert(transform.clone());
                    if let Some(m) = mat {
                        builder = builder.insert(m);
                    }
                    builder.build();
                }
                log::info!("Sponza заспавнена");
            }

            for (cam, _) in ctx.world.inner.query_mut::<(&mut CameraComponent, &ActiveCamera)>() {
                cam.eye = Vec3::new(8.0, 4.0, 0.0);
                cam.target = Vec3::new(0.0, 4.0, 0.0);
                cam.z_near = 0.01;
                cam.z_far = 50.0;
            }

            ctx.set_pipeline::<DefaultPipeline>();
            self.spawned = true;
            log::info!("Загрузка завершена - переключились на DefaultPipeline");
        }

        let t = self.frame as f32 * 0.003;
        for (cam, _) in ctx.world.inner.query_mut::<(&mut CameraComponent, &ActiveCamera)>() {
            if self.spawned {
                cam.eye = Vec3::new(t.sin() * 9.0, 2.0, t.cos() * 4.0);
                cam.target = Vec3::new(0.0, 2.0, 0.0);
            }
        }
    }

    fn on_render(&mut self, _ctx: &mut EngineContext) {}

    fn on_stop(&mut self, _ctx: &mut EngineContext) {
        log::info!("Stopped after {} frames", self.frame);
    }
}

fn main() -> anyhow::Result<()> {
    Engine::run(MyApp::new())
}
