use engine::ecs::components::Transform;
use engine::{App, Engine, EngineContext};
use glam::{Quat, Vec3};

struct MyApp {
    frame: u64,
    rotation: f32,
}

impl MyApp {
    fn new() -> Self {
        Self {
            frame: 0,
            rotation: 0.0,
        }
    }
}

impl App for MyApp {
    fn on_start(&mut self, ctx: &mut EngineContext) {
        let center = Vec3::new(13.44, 86.95, -3.70);
        ctx.camera.target = center;
        ctx.camera.eye = center + Vec3::new(0.0, 0.0, 280.0);
        ctx.camera.z_far = 1000.0;

        match ctx.assets.load_mesh("assets/duck.glb") {
            Ok(primitives) => {
                for (mesh, material) in primitives {
                    let mut builder = ctx.world.spawn();
                    builder = builder.insert(mesh);
                    builder = builder.insert(Transform::at(0.0, 0.0, 0.0));
                    if let Some(mat) = material {
                        builder = builder.insert(mat);
                    }
                    builder.build();
                }
                log::info!("Модель загружена");
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

    fn on_update(&mut self, ctx: &mut EngineContext, dt: f32) {
        self.frame += 1;
        self.rotation += dt * 30.0_f32.to_radians();

        for transform in ctx.world.inner.query_mut::<&mut Transform>() {
            transform.rotation = Quat::from_rotation_y(self.rotation);
        }

        let center = Vec3::new(13.44, 86.95, -3.70);
        let t = self.frame as f32 * 0.003;
        ctx.camera.eye = center + Vec3::new(t.sin() * 280.0, 50.0, t.cos() * 280.0);
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
