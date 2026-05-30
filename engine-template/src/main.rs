use engine::ecs::components::Transform;
use engine::{App, Engine, EngineContext};
use glam::Vec3;

struct MyApp {
    frame: u64,
}

impl MyApp {
    fn new() -> Self {
        Self {
            frame: 0
        }
    }
}

impl App for MyApp {
    fn on_start(&mut self, ctx: &mut EngineContext) {
        let center = Vec3::new(0.0, 4.0, 0.0);
        ctx.camera.target = center;
        ctx.camera.eye = Vec3::new(8.0, 4.0, 0.0);
        ctx.camera.z_near = 0.01;
        ctx.camera.z_far = 50.0;

        match ctx.assets.load_mesh("assets/sponza/glTF/Sponza.gltf") {
            Ok(primitives) => {
                for (mesh, material, transform) in primitives {
                    let mut builder = ctx.world.spawn();
                    builder = builder.insert(mesh);
                    builder = builder.insert(transform);
                    if let Some(mat) = material {
                        builder = builder.insert(mat);
                    }
                    builder.build();
                }
                log::info!("Sponza загружена");
            }
            Err(e) => {
                log::warn!("Не удалось загрузить модель: {e}, спавним куб");
                ctx.world
                    .spawn()
                    .insert(ctx.assets.mesh_cube())
                    .insert(Transform::at(0.0, 0.0, 0.0))
                    .build();
            }
        }
    }

    fn on_update(&mut self, ctx: &mut EngineContext, _dt: f32) {
        self.frame += 1;
        let center = Vec3::new(0.0, 2.0, 0.0);
        let t = self.frame as f32 * 0.003;
        ctx.camera.eye = Vec3::new(t.sin() * 9.0, 2.0, t.cos() * 4.0);
        ctx.camera.target = center;
    }

    fn on_render(&mut self, ctx: &mut EngineContext) {
        ctx.render_world([0.0, 0.0, 0.0, 1.0])
            .expect("render_world failed");
    }

    fn on_stop(&mut self, _ctx: &mut EngineContext) {
        log::info!("Stopped after {} frames", self.frame);
    }
}

fn main() -> anyhow::Result<()> {
    Engine::run(MyApp::new())
}
