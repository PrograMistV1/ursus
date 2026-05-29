use crate::assets::AssetServer;
use crate::ecs::systems::collect_draw_calls;
use crate::ecs::GameWorld;
use crate::vulkan::{Camera, DrawCall, Renderer, VulkanContext};
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
    pub world: GameWorld,
    pub assets: AssetServer,
    pub renderer: Renderer,
    pub vk: VulkanContext,
    pub camera: Camera,
}

impl EngineContext {
    fn new(vk: VulkanContext) -> anyhow::Result<Self> {
        let renderer = Renderer::new(&vk)?;

        let assets = AssetServer::new(
            vk.device.handle.clone(),
            vk.device.physical,
            vk.instance.handle.clone(),
            renderer.commands.pool,
            vk.device.graphics_queue,
        );

        Ok(Self {
            world: GameWorld::new(),
            assets,
            renderer,
            vk,
            camera: Camera::default(),
        })
    }
    pub fn render_world(&mut self, clear_color: [f32; 4]) -> anyhow::Result<()> {
        let ecs_calls = collect_draw_calls(&mut self.world, &self.assets);

        let gpu_calls: Vec<DrawCall<'_>> = ecs_calls
            .iter()
            .filter_map(|dc| {
                let gpu = self.assets.get_gpu_mesh(dc.mesh)?;
                Some(DrawCall {
                    gpu_mesh: gpu,
                    transform: &dc.transform,
                })
            })
            .collect();

        self.renderer
            .draw_frame(&self.vk, clear_color, &self.camera, &gpu_calls)
    }
}

pub struct Engine;

impl Engine {
    pub fn run(app: impl App + 'static) -> anyhow::Result<()> {
        env_logger::builder()
            .filter_level(log::LevelFilter::Info)
            .parse_default_env()
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
        if self.state.is_some() {
            return;
        }

        let attrs = WindowAttributes::default()
            .with_title("engine")
            .with_inner_size(winit::dpi::LogicalSize::new(1280u32, 720u32));

        let window = event_loop
            .create_window(attrs)
            .expect("Failed to create window");

        let vk =
            VulkanContext::new(&window, cfg!(debug_assertions)).expect("Failed to init Vulkan");

        let mut ctx = EngineContext::new(vk).expect("Failed to create EngineContext");

        self.app.on_start(&mut ctx);

        ctx.assets
            .upload_all_meshes()
            .expect("Failed to upload meshes to GPU");

        log::info!(
            "AssetServer: {} мешей, {} материалов",
            ctx.assets.mesh_count(),
            ctx.assets.material_count(),
        );

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
