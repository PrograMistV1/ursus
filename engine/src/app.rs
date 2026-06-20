use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;

use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowAttributes},
};

use crate::assets::cpu_server::CpuAssetServer;
use crate::assets::upload::GpuUploadRequest;
use crate::ecs::GameWorld;
use crate::extract::{default_extract_schedule, ExtractSchedule};
use crate::render_thread::command::RenderCommand;
use crate::render_thread::{render_thread_main, WindowHandles};
use crate::render_world::{ExtractedRenderSettings, RenderWorld};
use crate::triple_buffer::TripleBuffer;
use crate::vulkan::VulkanContext;

pub trait App {
    fn on_load(&mut self, _ctx: &mut EngineContext) {}
    fn on_start(&mut self, ctx: &mut EngineContext);
    fn on_update(&mut self, ctx: &mut EngineContext, dt: f32);
    fn on_render(&mut self, ctx: &mut EngineContext);
    fn on_stop(&mut self, ctx: &mut EngineContext);
}

/// Контекст главного потока. Не содержит GPU ресурсов.
pub struct EngineContext {
    pub world: GameWorld,
    pub cpu_assets: CpuAssetServer,
    pub extract_schedule: ExtractSchedule,

    pub(crate) cmd_tx: Sender<RenderCommand>,
    upload_tx: Sender<GpuUploadRequest>,
    triple_buf: Arc<TripleBuffer<RenderWorld>>,
    pub(crate) output_size: (f32, f32),
    was_loading: bool,
}

impl EngineContext {
    fn new(
        cmd_tx: Sender<RenderCommand>,
        upload_tx: Sender<GpuUploadRequest>,
        triple_buf: Arc<TripleBuffer<RenderWorld>>,
        output_size: (f32, f32),
    ) -> anyhow::Result<Self> {
        let cpu_assets = CpuAssetServer::new();

        Ok(Self {
            world: GameWorld::new(),
            cpu_assets,
            extract_schedule: default_extract_schedule(),
            cmd_tx,
            upload_tx,
            triple_buf,
            output_size,
            was_loading: false,
        })
    }

    /// Отправить команду рендер-потоку.
    pub fn send_render_cmd(&self, cmd: RenderCommand) {
        let _ = self.cmd_tx.send(cmd);
    }

    /// Опросить загрузчик ассетов, отправить CPU данные в рендер-поток,
    /// переключить пайплайн если загрузка завершилась.
    pub fn poll_assets(&mut self) {
        self.cpu_assets.poll_loader();
        self.cpu_assets.flush_uploads_cpu(&self.upload_tx);

        let is_loading = self.cpu_assets.is_loading();
        if self.was_loading && !is_loading {
            let _ = self.cmd_tx.send(RenderCommand::SetPipeline(PipelineKind::Default));
            log::info!("Загрузка завершена — переключаем на DefaultPipeline");
        } else if !self.was_loading && is_loading {
            let _ = self.cmd_tx.send(RenderCommand::SetPipeline(PipelineKind::Loading));
        }
        self.was_loading = is_loading;
    }

    pub fn is_loading(&self) -> bool {
        self.cpu_assets.is_loading()
    }

    pub(crate) fn publish_frame(&self, clear_color: [f32; 4]) {
        let write = self.triple_buf.write_slot();
        write.clear();
        write.insert(ExtractedRenderSettings { clear_color, output_size: self.output_size });
        self.extract_schedule.run(&self.world, write); // убрали gpu_assets-аргумент
        self.triple_buf.publish();
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

// ── Состояния ─────────────────────────────────────────────────────────────────

/// Vulkan инициализируется в рендер-потоке, окно скрыто.
struct WaitingState {
    window: Window,
    ctx: EngineContext,
    ready_rx: Receiver<()>,
    render_thread: JoinHandle<()>,
}

/// Рендер готов, основной игровой цикл.
struct RunningState {
    window: Window,
    ctx: EngineContext,
    render_thread: JoinHandle<()>,
    last: std::time::Instant,
    fps_timer: std::time::Instant,
    fps_frames: u32,
    tick_accumulator: f32,
    paced_frame_time: f64,
}

enum EngineState {
    Waiting(WaitingState),
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
                    .with_inner_size(winit::dpi::LogicalSize::new(1280u32, 720u32))
                    .with_visible(false), // скрыто до готовности рендера
            )
            .expect("Failed to create window");

        let size = window.inner_size();
        let output_size = (size.width as f32, size.height as f32);

        use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};
        let display = window.display_handle().unwrap().as_raw();
        let whandle = window.window_handle().unwrap().as_raw();

        let (cmd_tx, cmd_rx) = mpsc::channel::<RenderCommand>();
        let (upload_tx, upload_rx) = mpsc::channel::<GpuUploadRequest>();
        // SyncSender с буфером 1 — рендер-поток отправляет один сигнал и не блокируется
        let (ready_tx, ready_rx) = mpsc::sync_channel::<()>(1);

        let triple_buf = Arc::new(TripleBuffer::<RenderWorld>::new());
        let triple_buf_render = Arc::clone(&triple_buf);

        let handles = WindowHandles { display, window: whandle };
        let render_thread = std::thread::Builder::new()
            .name("render".into())
            .spawn(move || {
                render_thread_main(handles, triple_buf_render, cmd_rx, upload_rx, ready_tx);
            })
            .expect("Failed to spawn render thread");

        let mut ctx =
            EngineContext::new(cmd_tx, upload_tx, triple_buf, output_size).expect("Failed to create EngineContext");

        self.app.on_load(&mut ctx);

        self.state = Some(EngineState::Waiting(WaitingState { window, ctx, ready_rx, render_thread }));
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: winit::window::WindowId, event: WindowEvent) {
        // Попытаться перейти из Waiting → Running
        if matches!(self.state, Some(EngineState::Waiting(_))) {
            let waiting = match self.state.take().unwrap() {
                EngineState::Waiting(w) => w,
                _ => unreachable!(),
            };

            match waiting.ready_rx.try_recv() {
                Ok(()) => {
                    // Рендер-поток готов
                    waiting.window.set_visible(true);
                    let WaitingState { window, mut ctx, render_thread, .. } = waiting;
                    self.app.on_start(&mut ctx);
                    self.state = Some(EngineState::Running(RunningState {
                        window,
                        ctx,
                        render_thread,
                        last: std::time::Instant::now(),
                        fps_timer: std::time::Instant::now(),
                        fps_frames: 0,
                        tick_accumulator: 0.0,
                        paced_frame_time: 1.0 / 120.0,
                    }));
                }
                Err(_) => {
                    // Ещё не готов — возвращаем состояние
                    self.state = Some(EngineState::Waiting(waiting));
                    if let WindowEvent::CloseRequested = event {
                        event_loop.exit();
                    }
                    return;
                }
            }
        }

        let Some(EngineState::Running(state)) = self.state.as_mut() else {
            return;
        };

        match event {
            WindowEvent::CloseRequested => {
                self.app.on_stop(&mut state.ctx);
                let _ = state.ctx.cmd_tx.send(RenderCommand::Shutdown);
                // join через take чтобы не бороться с borrow checker
                if let Some(EngineState::Running(s)) = self.state.take() {
                    s.render_thread.join().ok();
                }
                event_loop.exit();
            }

            WindowEvent::Resized(size) => {
                if size.width == 0 || size.height == 0 {
                    return;
                }
                state.ctx.output_size = (size.width as f32, size.height as f32);
                let _ = state.ctx.cmd_tx.send(RenderCommand::Resize { width: size.width, height: size.height });
            }

            WindowEvent::RedrawRequested => {
                puffin::GlobalProfiler::lock().new_frame();

                let frame_start = std::time::Instant::now();

                state.ctx.poll_assets();

                let now = std::time::Instant::now();
                let dt = now.duration_since(state.last).as_secs_f32().min(0.1);
                state.last = now;
                state.fps_frames += 1;
                if now.duration_since(state.fps_timer).as_secs_f32() >= 1.0 {
                    state.fps_frames = 0;
                    state.fps_timer = now;
                }

                state.tick_accumulator += dt;
                while state.tick_accumulator >= TICK_RATE {
                    self.app.on_update(&mut state.ctx, TICK_RATE);
                    state.tick_accumulator -= TICK_RATE;
                }

                self.app.on_render(&mut state.ctx);

                state.ctx.publish_frame([0.0, 0.0, 0.0, 1.0]);

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
