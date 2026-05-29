use engine::ecs::components::{MeshHandle, Transform};
use engine::{App, Engine, EngineContext};
use glam::{Quat, Vec3};

struct MyApp {
    frame: u64,
    cube: Option<MeshHandle>,
    rotation: f32,
}

impl MyApp {
    fn new() -> Self {
        Self {
            frame: 0,
            cube: None,
            rotation: 0.0,
        }
    }
}

impl App for MyApp {
    fn on_start(&mut self, ctx: &mut EngineContext) {
        log::info!("App started");

        let cube_handle = ctx.assets.mesh_cube();
        self.cube = Some(cube_handle);

        ctx.world
            .spawn()
            .insert(cube_handle)
            .insert(Transform::at(0.0, 0.0, 0.0))
            .build();

        ctx.world
            .spawn()
            .insert(cube_handle)
            .insert(Transform::at(2.5, 0.0, 0.0).with_scale(0.5))
            .build();

        ctx.world
            .spawn()
            .insert(cube_handle)
            .insert(Transform::at(-2.5, 0.0, 0.0).with_scale(0.5))
            .build();

        log::info!("Spawned {} entities", ctx.world.entity_count());
    }

    fn on_update(&mut self, ctx: &mut EngineContext, dt: f32) {
        self.frame += 1;
        self.rotation += dt * 45.0_f32.to_radians();

        for transform in ctx.world.inner.query_mut::<&mut Transform>() {
            transform.rotation = Quat::from_rotation_y(self.rotation);
        }

        let t = self.frame as f32 * 0.005;
        ctx.camera.eye = Vec3::new(t.sin() * 5.0, 3.0, t.cos() * 5.0);
    }

    fn on_render(&mut self, ctx: &mut EngineContext) {
        ctx.render_world([0.05, 0.05, 0.1, 1.0])
            .expect("render_world failed");
    }

    fn on_stop(&mut self, ctx: &mut EngineContext) {
        log::info!(
            "App stopped after {} frames ({} entities)",
            self.frame,
            ctx.world.entity_count()
        );
    }
}

fn main() -> anyhow::Result<()> {
    Engine::run(MyApp::new())
}
