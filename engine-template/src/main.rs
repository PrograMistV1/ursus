use engine::components::{UiLayout, UiText};
use engine::{App, AsyncMeshHandle, Engine, EngineContext};
use glam::{Vec2, Vec3};

struct MyApp {
    sponza: Option<AsyncMeshHandle>,
    frame: u64,
}

impl MyApp {
    fn new() -> Self {
        Self { sponza: None, frame: 0 }
    }
}

impl App for MyApp {
    fn on_load(&mut self, ctx: &mut EngineContext) {
        self.sponza = Some(ctx.cpu_assets.load_mesh_async("assets/sponza/glTF/Sponza.gltf"));
        let entity = ctx
            .world
            .spawn()
            .insert(UiLayout::top_left(Vec2::new(16.0, 16.0)))
            .insert(UiText::new("FPS: 60").with_size(18.0).with_color([1.0; 4]))
            .build();
    }

    fn on_start(&mut self, ctx: &mut EngineContext) {
        if let Some(handle) = &self.sponza {
            for (mesh, mat, transform) in ctx.cpu_assets.get_mesh_instances(handle).unwrap() {
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

        ctx.camera.target = Vec3::new(0.0, 4.0, 0.0);
        ctx.camera.eye = Vec3::new(8.0, 4.0, 0.0);
        ctx.camera.z_near = 0.01;
        ctx.camera.z_far = 50.0;
    }

    fn on_update(&mut self, ctx: &mut EngineContext, _dt: f32) {
        self.frame += 1;
        let t = self.frame as f32 * 0.003;
        ctx.camera.eye = Vec3::new(t.sin() * 9.0, 2.0, t.cos() * 4.0);
        ctx.camera.target = Vec3::new(0.0, 2.0, 0.0);
    }

    fn on_render(&mut self, _ctx: &mut EngineContext) {}

    fn on_stop(&mut self, _ctx: &mut EngineContext) {
        log::info!("Stopped after {} frames", self.frame);
    }
}

fn main() -> anyhow::Result<()> {
    Engine::run(MyApp::new())
}
