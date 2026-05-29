use crate::vulkan::{Renderer, VulkanContext};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowAttributes},
};

pub trait App {
    fn on_start(&mut self, ctx: &mut EngineContext);
    fn on_update(&mut self, ctx: &mut EngineContext, dt: f32);
    fn on_render(&mut self, ctx: &mut EngineContext);
    fn on_stop(&mut self, ctx: &mut EngineContext);
}

pub struct EngineContext {
    pub renderer: Renderer,
    pub vk: VulkanContext,
}

pub struct Engine;

impl Engine {
    pub fn run(app: impl App + 'static) -> anyhow::Result<()> {
        env_logger::builder()
            .filter_level(log::LevelFilter::Info)
            .init();

        let event_loop = EventLoop::new()?;
        event_loop.set_control_flow(ControlFlow::Poll);

        let mut handler = EngineHandler {
            app: Box::new(app),
            state: None,
        };
        event_loop.run_app(&mut handler)?;
        Ok(())
    }
}

struct RunningState {
    window: Window,
    ctx: EngineContext,
    last: std::time::Instant,
}

struct EngineHandler {
    app: Box<dyn App>,
    state: Option<RunningState>,
}

impl ApplicationHandler for EngineHandler {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() { return; }

        let attrs = WindowAttributes::default()
            .with_title("engine")
            .with_inner_size(winit::dpi::LogicalSize::new(1280u32, 720u32));

        let window = event_loop
            .create_window(attrs)
            .expect("Failed to create window");

        let vk = VulkanContext::new(&window, cfg!(debug_assertions))
            .expect("Failed to init Vulkan");

        let renderer = Renderer::new(&vk)
            .expect("Failed to create renderer");

        let mut ctx = EngineContext { renderer, vk };
        self.app.on_start(&mut ctx);

        self.state = Some(RunningState {
            window,
            ctx,
            last: std::time::Instant::now(),
        });
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = &mut self.state else { return };

        match event {
            WindowEvent::CloseRequested => {
                self.app.on_stop(&mut state.ctx);
                event_loop.exit();
            }
            WindowEvent::Resized(_) => {
                // TODO: пересоздать swapchain
            }
            WindowEvent::RedrawRequested => {
                let now = std::time::Instant::now();
                let dt = now.duration_since(state.last).as_secs_f32();
                state.last = now;

                self.app.on_update(&mut state.ctx, dt);
                self.app.on_render(&mut state.ctx);
                state.window.request_redraw();
            }
            _ => {}
        }
    }
}