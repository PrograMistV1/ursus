use crate::assets::cpu_server::CpuAssetServer;
use crate::assets::gpu_server::GpuAssetServer;
use crate::assets::CpuMesh;
use crate::debug_ui::{self, DebugUiState};
use crate::ecs::GameWorld;
use crate::egui_layer::EguiLayer;
use crate::lighting::LightingUbo;
use crate::pipeline::{DefaultPipeline, LoadingPipeline, RenderPipeline};
use crate::vulkan::{build_dyn_renderer, DynRenderer};
use crate::vulkan::{Camera, VulkanContext};
use std::sync::{Arc, Mutex};

use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowAttributes},
};

pub trait App {
    fn on_load(&mut self, _ctx: &mut EngineContext) {}
    fn on_start(&mut self, ctx: &mut EngineContext);
    fn on_update(&mut self, ctx: &mut EngineContext, dt: f32);
    fn on_render(&mut self, ctx: &mut EngineContext);
    fn on_stop(&mut self, ctx: &mut EngineContext);
}

pub struct EngineContext {
    pub camera: Camera,
    pub lighting: LightingUbo,
    pub world: GameWorld,
    pub renderer: Box<dyn DynRenderer>,
    pub cpu_assets: CpuAssetServer,
    pub gpu_assets: GpuAssetServer,
    temp_pool: ash::vk::CommandPool,
    pub vk: VulkanContext,
}

impl EngineContext {
    fn new(vk: VulkanContext) -> anyhow::Result<Self> {
        let temp_pool = create_temp_pool(&vk)?;

        let upload_queue = Arc::new(Mutex::new(Vec::new()));
        let mut cpu_assets = CpuAssetServer::new(Arc::clone(&upload_queue));

        let mut gpu_assets = GpuAssetServer::new(
            vk.device.handle.clone(),
            vk.device.physical,
            vk.instance.handle.clone(),
            temp_pool,
            vk.device.graphics_queue,
            upload_queue,
            Arc::clone(&cpu_assets.mesh_path_cache),
        )?;

        let tri = cpu_assets.register_mesh(CpuMesh::triangle());
        let cube = cpu_assets.register_mesh(CpuMesh::cube());
        let plane = cpu_assets.register_mesh(CpuMesh::plane(10.0, 10));
        gpu_assets.upload_mesh(tri, cpu_assets.get_cpu_mesh(tri).unwrap())?;
        gpu_assets.upload_mesh(cube, cpu_assets.get_cpu_mesh(cube).unwrap())?;
        gpu_assets.upload_mesh(plane, cpu_assets.get_cpu_mesh(plane).unwrap())?;

        let renderer = build_dyn_renderer::<DefaultPipeline>(&vk, &mut cpu_assets, &mut gpu_assets, 0.5, 0.2)?;

        Ok(Self {
            camera: Camera::default(),
            lighting: LightingUbo::default(),
            world: GameWorld::new(),
            renderer,
            cpu_assets,
            gpu_assets,
            temp_pool,
            vk,
        })
    }

    pub fn poll_assets(&mut self) {
        self.cpu_assets.poll_loader();
        self.gpu_assets.flush_uploads(&mut self.cpu_assets).ok();
    }

    pub fn is_loading(&self) -> bool {
        self.cpu_assets.is_loading()
    }

    pub fn set_pipeline<P: RenderPipeline + Default + 'static>(&mut self) -> anyhow::Result<()> {
        unsafe { self.vk.device.handle.device_wait_idle()? };

        let prev_exposure = self.renderer.exposure();
        let prev_fsr = self.renderer.fsr_sharpness();

        let new_renderer =
            build_dyn_renderer::<P>(&self.vk, &mut self.cpu_assets, &mut self.gpu_assets, prev_exposure, prev_fsr)?;

        self.renderer = new_renderer;
        log::info!("Pipeline switched to {}", std::any::type_name::<P>());
        Ok(())
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
            &mut self.cpu_assets,
            &mut self.gpu_assets,
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
            if let Err(e) = self.vk.device.handle.device_wait_idle() {
                log::error!("device_wait_idle failed on shutdown: {e}");
            }
            self.vk.device.handle.destroy_command_pool(self.temp_pool, None);
        }
    }
}

pub struct Engine;

impl Engine {
    pub fn run(app: impl App + 'static) -> anyhow::Result<()> {
        env_logger::builder().filter_level(log::LevelFilter::Info).parse_default_env().init();

        let event_loop = EventLoop::new()?;
        event_loop.set_control_flow(ControlFlow::Poll);

        let mut handler = EngineHandler { app: Box::new(app), state: None };
        event_loop.run_app(&mut handler)?;
        Ok(())
    }
}

struct LoadingState {
    window: Window,
    egui: EguiLayer,
    ctx: EngineContext,
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

enum EngineState {
    Loading(LoadingState),
    Running(RunningState),
}

const TICK_RATE: f32 = 1.0 / 60.0;

struct EngineHandler {
    app: Box<dyn App>,
    state: Option<EngineState>,
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

        let vk = VulkanContext::new(&window, cfg!(debug_assertions)).expect("Failed to init Vulkan");

        let mut ctx = EngineContext::new(vk).expect("Failed to create EngineContext");

        self.app.on_load(&mut ctx);

        let swapchain = ctx.vk.swapchain.as_ref().unwrap();
        let egui = EguiLayer::new(
            &window,
            &ctx.vk.instance.handle,
            ctx.vk.device.physical,
            ctx.vk.device.handle.clone(),
            swapchain.format,
        )
        .expect("Failed to create EguiLayer");

        if ctx.is_loading() {
            ctx.set_pipeline::<LoadingPipeline>().expect("Failed to switch to LoadingPipeline");

            log::info!("Входим в Loading state");
            self.state = Some(EngineState::Loading(LoadingState { window, egui, ctx }));
        } else {
            self.app.on_start(&mut ctx);
            self.state = Some(EngineState::Running(make_running_state(window, egui, ctx)));
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: winit::window::WindowId, event: WindowEvent) {
        if self.state.is_none() {
            return;
        }

        match self.state.as_mut() {
            Some(EngineState::Loading(s)) => {
                handle_loading_event(s, &event, event_loop);

                if !s.ctx.is_loading() {
                    log::info!("Загрузка завершена — переходим в Running state");
                    let state = self.state.take().unwrap();
                    if let EngineState::Loading(mut ls) = state {
                        unsafe {
                            ls.ctx.vk.device.handle.device_wait_idle().ok();
                        }
                        ls.ctx.set_pipeline::<DefaultPipeline>().expect("Failed to switch to DefaultPipeline");
                        self.app.on_start(&mut ls.ctx);
                        let running = make_running_state(ls.window, ls.egui, ls.ctx);
                        self.state = Some(EngineState::Running(running));
                    }
                }
            }
            Some(EngineState::Running(s)) => {
                if let WindowEvent::CloseRequested = event {
                    if let Some(EngineState::Running(mut s)) = self.state.take() {
                        self.app.on_stop(&mut s.ctx);
                        unsafe {
                            s.ctx.vk.device.handle.device_wait_idle().ok();
                        }
                        drop(s);
                    }
                    event_loop.exit();
                    return;
                }
                handle_running_event(s, &event, &mut *self.app);
            }
            _ => {}
        }
    }
}

fn handle_loading_event(state: &mut LoadingState, event: &WindowEvent, event_loop: &ActiveEventLoop) {
    match event {
        WindowEvent::CloseRequested => {
            event_loop.exit();
        }

        WindowEvent::Resized(size) => {
            if size.width == 0 || size.height == 0 {
                return;
            }
            if let Err(e) = state.ctx.vk.recreate_swapchain(size.width, size.height, false) {
                log::error!("Resize during loading failed: {e}");
                return;
            }
            if let Err(e) = state.ctx.renderer.resize_output(size.width, size.height) {
                log::error!("Renderer resize during loading failed: {e}");
            }
        }

        WindowEvent::RedrawRequested => {
            state.ctx.poll_assets();

            let progress = state.ctx.cpu_assets.load_progress.clone();

            let raw = state.egui.begin_frame(&state.window);
            let egui_output = state.egui.ctx.run(raw, |ctx| {
                draw_loading_ui(ctx, &progress);
            });

            if let Err(e) = state.ctx.render_frame(&state.window, &mut state.egui, egui_output, [0.0; 4]) {
                log::error!("Loading render error: {e}");
            }

            state.window.request_redraw();
        }

        _ => {}
    }
}

fn draw_loading_ui(ctx: &egui::Context, progress: &crate::assets::LoadProgress) {
    egui::Area::new("loading".into()).anchor(egui::Align2::CENTER_BOTTOM, [0.0, -60.0]).show(ctx, |ui| {
        ui.set_min_width(400.0);
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new("Loading...").size(18.0).color(egui::Color32::WHITE));
            ui.add_space(8.0);
            ui.add(egui::ProgressBar::new(progress.fraction()).desired_width(400.0).show_percentage());
            ui.add_space(4.0);
            ui.label(egui::RichText::new(&progress.current).size(11.0).color(egui::Color32::GRAY));
        });
    });
}

fn handle_running_event(state: &mut RunningState, event: &WindowEvent, app: &mut dyn App) {
    let consumed = state.egui.handle_window_event(&state.window, event);
    if consumed {
        return;
    }

    match event {
        WindowEvent::Resized(size) => {
            if size.width == 0 || size.height == 0 {
                return;
            }
            if let Err(e) = handle_resize(state, size.width, size.height) {
                log::error!("Resize failed: {e}");
            }
        }

        WindowEvent::RedrawRequested => {
            state.ctx.poll_assets();

            let frame_start = std::time::Instant::now();
            puffin::GlobalProfiler::lock().new_frame();

            let now = std::time::Instant::now();
            let dt = now.duration_since(state.last).as_secs_f32().min(0.1);
            state.last = now;
            state.fps_frames += 1;

            if now.duration_since(state.fps_timer).as_secs_f32() >= 1.0 {
                state.fps_current = state.fps_frames as f32 / now.duration_since(state.fps_timer).as_secs_f32();
                state.fps_frames = 0;
                state.fps_timer = now;
            }

            {
                puffin::profile_scope!("tick_accumulator");
                state.tick_accumulator += dt;
                while state.tick_accumulator >= TICK_RATE {
                    puffin::profile_scope!("on_update");
                    app.on_update(&mut state.ctx, TICK_RATE);
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
                        state.ctx.renderer.last_frame_times(),
                    );
                })
            };
            puffin::set_scopes_on(state.debug.show_profiler);
            state.ctx.renderer.set_exposure(state.debug.exposure);

            app.on_render(&mut state.ctx);

            let needs_recreate = {
                puffin::profile_scope!("render_frame");
                state
                    .ctx
                    .render_frame(&state.window, &mut state.egui, egui_output, [0.0, 0.0, 0.0, 1.0])
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
            state.paced_frame_time = state.paced_frame_time * 0.9 + render_time.as_secs_f64() * 0.1;

            let pace = std::time::Duration::from_secs_f64(state.paced_frame_time * 0.5);
            let elapsed = frame_start.elapsed();
            if pace > elapsed + std::time::Duration::from_micros(500) {
                std::thread::sleep(pace - elapsed - std::time::Duration::from_micros(500));
            }
        }

        _ => {}
    }
}

fn make_running_state(window: Window, egui: EguiLayer, ctx: EngineContext) -> RunningState {
    RunningState {
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
    }
}

pub fn create_temp_pool(vk: &VulkanContext) -> anyhow::Result<ash::vk::CommandPool> {
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
    state.ctx.vk.recreate_swapchain(width, height, state.debug.vsync)?;
    state.ctx.renderer.resize_output(width, height)?;
    Ok(())
}
