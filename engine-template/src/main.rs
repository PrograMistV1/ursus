use engine::{App, Engine, EngineContext};

struct MyApp {
    frame: u64,
}

impl MyApp {
    fn new() -> Self {
        Self { frame: 0 }
    }
}

impl App for MyApp {
    fn on_start(&mut self, _ctx: &mut EngineContext) {
        log::info!("App started");
    }
    fn on_update(&mut self, _ctx: &mut EngineContext, _dt: f32) {
        self.frame += 1;
    }
    fn on_render(&mut self, ctx: &mut EngineContext) {
        ctx.renderer
            .draw_frame(&ctx.vk, [0.0, 0.0, 0.0, 1.0])
            .expect("draw_frame failed");
    }
    fn on_stop(&mut self, _ctx: &mut EngineContext) {
        log::info!("App stopped after {} frames", self.frame);
    }
}

fn main() -> anyhow::Result<()> {
    Engine::run(MyApp::new())
}