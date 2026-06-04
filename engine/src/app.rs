use crate::assets::AssetServer;
use crate::debug_ui::{self, DebugUiState};
use crate::ecs::GameWorld;
use crate::egui_layer::EguiLayer;
use crate::lighting::LightingUbo;
use crate::vulkan::{Camera, Renderer, VulkanContext};

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
    pub lighting: LightingUbo,
    temp_pool: ash::vk::CommandPool,
}

impl EngineContext {
    fn new(vk: VulkanContext) -> anyhow::Result<Self> {
        let temp_pool = create_temp_pool(&vk)?;

        let mut assets = AssetServer::new(
            vk.device.handle.clone(),
            vk.device.physical,
            vk.instance.handle.clone(),
            temp_pool,
            vk.device.graphics_queue,
        )?;

        let renderer = Renderer::new(&vk, &mut assets)?;

        Ok(Self {
            world: GameWorld::new(),
            assets,
            renderer,
            vk,
            camera: Camera::default(),
            lighting: LightingUbo::default(),
            temp_pool,
        })
    }

    pub fn render_frame(
        &mut self,
        window: &Window,
        egui: &mut EguiLayer,
        egui_output: egui::FullOutput,
        clear_color: [f32; 4],
    ) -> anyhow::Result<bool> {
        self.renderer.draw_frame(
            &self.vk,
            &mut self.world,
            &self.assets,
            &self.camera,
            &self.lighting,
            egui,
            egui_output,
            window,
            clear_color,
        )
    }
}

impl Drop for EngineContext {
    fn drop(&mut self) {
        unsafe {
            self.vk.device.handle.device_wait_idle().ok();
            self.vk
                .device
                .handle
                .destroy_command_pool(self.temp_pool, None);
        }
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
    egui: EguiLayer,
    ctx: EngineContext,
    debug: DebugUiState,
    last: std::time::Instant,
    fps_timer: std::time::Instant,
    fps_frames: u32,
    fps_current: f32,
    tick_accumulator: f32,
    paced_frame_time: f64,
    cpu_history: debug_ui::CpuFrameHistory,
}

const TICK_RATE: f32 = 1.0 / 60.0;

struct EngineHandler {
    app: Box<dyn App>,
    state: Option<RunningState>,
}

impl ApplicationHandler for EngineHandler {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        let window = event_loop
            .create_window(
                WindowAttributes::default()
                    .with_title("engine")
                    .with_inner_size(winit::dpi::LogicalSize::new(1280u32, 720u32)),
            )
            .expect("Failed to create window");

        let vk =
            VulkanContext::new(&window, cfg!(debug_assertions)).expect("Failed to init Vulkan");
        let mut ctx = EngineContext::new(vk).expect("Failed to create EngineContext");

        self.app.on_start(&mut ctx);

        ctx.assets
            .upload_all_meshes()
            .expect("Failed to upload meshes");

        log::info!(
            "AssetServer: {} мешей, {} материалов, {} текстур",
            ctx.assets.mesh_count(),
            ctx.assets.material_count(),
            ctx.assets.texture_count(),
        );

        let swapchain = ctx.vk.swapchain.as_ref().unwrap();
        let egui = EguiLayer::new(
            &window,
            &ctx.vk.instance.handle,
            ctx.vk.device.physical,
            ctx.vk.device.handle.clone(),
            swapchain.format,
        )
        .expect("Failed to create EguiLayer");

        self.state = Some(RunningState {
            window,
            egui,
            ctx,
            debug: DebugUiState::default(),
            last: std::time::Instant::now(),
            fps_timer: std::time::Instant::now(),
            fps_frames: 0,
            fps_current: 0.0,
            tick_accumulator: 0.0,
            paced_frame_time: 1.0 / 120.0,
            cpu_history: debug_ui::CpuFrameHistory::new(120),
        });
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = &mut self.state else { return };

        let consumed = state.egui.handle_window_event(&state.window, &event);
        if consumed {
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                if let Some(state) = &mut self.state {
                    self.app.on_stop(&mut state.ctx);
                    unsafe {
                        state.ctx.vk.device.handle.device_wait_idle().ok();
                    }
                }
                self.state = None;
                event_loop.exit();
            }

            WindowEvent::Resized(size) => {
                if size.width == 0 || size.height == 0 {
                    return;
                }
                if let Err(e) = handle_resize(state, size.width, size.height) {
                    log::error!("Resize failed: {e}");
                }
            }

            WindowEvent::RedrawRequested => {
                let frame_start = std::time::Instant::now();
                puffin::GlobalProfiler::lock().new_frame();

                let now = std::time::Instant::now();
                let dt = now.duration_since(state.last).as_secs_f32().min(0.1);
                state.last = now;
                state.fps_frames += 1;

                if now.duration_since(state.fps_timer).as_secs_f32() >= 1.0 {
                    state.fps_current =
                        state.fps_frames as f32 / now.duration_since(state.fps_timer).as_secs_f32();
                    state.fps_frames = 0;
                    state.fps_timer = now;
                }

                {
                    puffin::profile_scope!("tick_accumulator");
                    state.tick_accumulator += dt;
                    while state.tick_accumulator >= TICK_RATE {
                        puffin::profile_scope!("on_update");
                        self.app.on_update(&mut state.ctx, TICK_RATE);
                        state.tick_accumulator -= TICK_RATE;
                    }
                }

                let egui_output = {
                    puffin::profile_scope!("egui_build");
                    let raw = state.egui.begin_frame(&state.window);
                    state.egui.ctx.run(raw, |ctx| {
                        debug_ui::draw(
                            ctx,
                            &mut state.debug,
                            state.fps_current,
                            state.ctx.world.entity_count(),
                            &state.cpu_history,
                            &state.ctx.renderer.timestamps.last_frame,
                        );
                    })
                };
                puffin::set_scopes_on(state.debug.show_profiler);

                state.ctx.renderer.exposure = state.debug.exposure;

                self.app.on_render(&mut state.ctx);

                let needs_recreate = {
                    puffin::profile_scope!("render_frame");
                    state
                        .ctx
                        .render_frame(
                            &state.window,
                            &mut state.egui,
                            egui_output,
                            [0.0, 0.0, 0.0, 1.0],
                        )
                        .expect("render failed")
                };

                let frame_ms = frame_start.elapsed().as_secs_f32() * 1000.0;
                state.cpu_history.push(frame_ms);

                if needs_recreate || state.debug.swapchain_dirty {
                    state.debug.swapchain_dirty = false;
                    let size = state.window.inner_size();
                    if let Err(e) = handle_resize(state, size.width, size.height) {
                        log::error!("Swapchain recreate failed: {e}");
                    }
                }

                state.window.request_redraw();

                let render_time = frame_start.elapsed();
                state.paced_frame_time =
                    state.paced_frame_time * 0.9 + render_time.as_secs_f64() * 0.1;

                let pace = std::time::Duration::from_secs_f64(state.paced_frame_time * 0.5);
                let elapsed = frame_start.elapsed();
                if pace > elapsed + std::time::Duration::from_micros(500) {
                    std::thread::sleep(pace - elapsed - std::time::Duration::from_micros(500));
                }
            }

            _ => {}
        }
    }
}

fn create_temp_pool(vk: &VulkanContext) -> anyhow::Result<ash::vk::CommandPool> {
    use ash::vk;
    let pool = unsafe {
        vk.device.handle.create_command_pool(
            &vk::CommandPoolCreateInfo::default()
                .queue_family_index(vk.device.graphics_family)
                .flags(vk::CommandPoolCreateFlags::TRANSIENT),
            None,
        )?
    };
    Ok(pool)
}

fn handle_resize(state: &mut RunningState, width: u32, height: u32) -> anyhow::Result<()> {
    unsafe { state.ctx.vk.device.handle.device_wait_idle()? };

    state
        .ctx
        .vk
        .recreate_swapchain(width, height, state.debug.vsync)?;

    state.ctx.renderer.resize_output(width, height)?;

    Ok(())
}
