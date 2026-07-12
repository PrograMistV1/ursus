use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Instant;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowAttributes},
};

use crate::assets::cpu_server::CpuAssetServer;
use crate::assets::loader_registry::LoaderRegistry;
use crate::assets::upload::GpuUploadRequest;
use crate::ecs::GameWorld;
use crate::render::extract::{default_extract_schedule, ExtractSchedule};
use crate::render::frame_pipeline::render_pipeline::RenderPipeline;
use crate::render::frame_stats::FrameStats;
use crate::render::thread::command::{PipelineFactory, RenderCommand};
use crate::render::thread::{render_thread_main, WindowHandles};
use crate::render::triple_buffer::TripleBuffer;
use crate::render::world::{ExtractedRenderSettings, RenderWorld};
use crate::vulkan::VulkanContext;
use crate::EngineFlags;

pub trait App {
    fn initial_pipeline() -> PipelineFactory
    where
        Self: Sized,
    {
        PipelineFactory::empty()
    }
    fn register_loaders(_registry: &mut LoaderRegistry)
    where
        Self: Sized,
    {
    }
    fn on_start(&mut self, ctx: &mut EngineContext);
    fn on_update(&mut self, ctx: &mut EngineContext, dt: f32);
    fn on_render(&mut self, ctx: &mut EngineContext);
    fn on_stop(&mut self, ctx: &mut EngineContext);
}

pub struct EngineContext {
    pub world: GameWorld,
    pub cpu_assets: CpuAssetServer,
    pub extract_schedule: ExtractSchedule,

    pub(crate) cmd_tx: Sender<RenderCommand>,
    upload_tx: Sender<GpuUploadRequest>,
    triple_buf: Arc<TripleBuffer<RenderWorld>>,
    pub(crate) output_size: (f32, f32),
    frame_stats: FrameStats,
}

impl EngineContext {
    fn new(
        cmd_tx: Sender<RenderCommand>,
        upload_tx: Sender<GpuUploadRequest>,
        triple_buf: Arc<TripleBuffer<RenderWorld>>,
        output_size: (f32, f32),
        loader_registry: LoaderRegistry,
        frame_stats: FrameStats,
    ) -> anyhow::Result<Self> {
        let cpu_assets = CpuAssetServer::new(loader_registry);

        Ok(Self {
            world: GameWorld::new(),
            cpu_assets,
            extract_schedule: default_extract_schedule(),
            cmd_tx,
            upload_tx,
            triple_buf,
            output_size,
            frame_stats,
        })
    }

    pub fn frame_stats(&self) -> &FrameStats {
        &self.frame_stats
    }

    pub fn send_render_cmd(&self, cmd: RenderCommand) {
        let _ = self.cmd_tx.send(cmd);
    }

    pub fn set_pipeline<P>(&self)
    where
        P: RenderPipeline + Default + 'static,
    {
        self.send_render_cmd(RenderCommand::SetPipeline(PipelineFactory::of::<P>()));
    }

    pub fn poll_assets(&mut self) {
        self.cpu_assets.poll_loader();
        self.cpu_assets.flush_uploads_cpu(&self.upload_tx);
    }

    pub(crate) fn publish_frame(&mut self, clear_color: [f32; 4]) {
        let write = self.triple_buf.write_slot();
        write.clear();
        write.insert(ExtractedRenderSettings {
            clear_color,
            output_size: self.output_size,
            fsr_sharpness: 0.2,
            exposure: 0.5,
        });
        self.extract_schedule.run(&self.world, write, &mut self.cpu_assets, &self.upload_tx);
        self.triple_buf.publish();
    }
}

pub struct Engine;

impl Engine {
    pub fn run<A: App + 'static>(app: A) -> anyhow::Result<()> {
        env_logger::builder()
            .filter_level(log::LevelFilter::Info)
            .filter_module("cosmic_text::font::fallback", log::LevelFilter::Off)
            .format(|buf, record| {
                use std::io::Write;

                let parts: Vec<&str> = record.target().split("::").collect();
                let start = parts.len().saturating_sub(2);
                let short_target = parts[start..].join("::");

                let ts = buf.timestamp().to_string();
                let ts = ts.split('T').nth(1).unwrap_or(&ts).trim_end_matches('Z');

                writeln!(buf, "[{} {:<5} {}] {}", ts, record.level(), short_target, record.args())
            })
            .parse_default_env()
            .init();

        let flags = EngineFlags::from_args();

        let _puffin_server = if flags.profile {
            let server_addr = format!("127.0.0.1:{}", puffin_http::DEFAULT_PORT);
            let server = puffin_http::Server::new(&server_addr)?;
            log::info!("Run this to view profiling data: puffin_viewer --url {server_addr}");
            puffin::set_scopes_on(true);
            Some(server)
        } else {
            puffin::set_scopes_on(false);
            None
        };

        let initial_pipeline = A::initial_pipeline();

        let mut loader_registry = LoaderRegistry::new();
        A::register_loaders(&mut loader_registry);

        let event_loop = EventLoop::new()?;
        event_loop.set_control_flow(ControlFlow::Poll);

        let mut handler = EngineHandler {
            app: Box::new(app),
            initial_pipeline: Some(initial_pipeline),
            loader_registry: Some(loader_registry),
            flags,
            state: None,
        };
        event_loop.run_app(&mut handler)?;
        Ok(())
    }
}

struct WaitingState {
    window: Window,
    ctx: EngineContext,
    ready_rx: Receiver<()>,
    render_thread: JoinHandle<()>,
}

struct RunningState {
    window: Window,
    ctx: EngineContext,
    render_thread: JoinHandle<()>,
    last: Instant,
    tick_accumulator: f32,
}

enum EngineState {
    Waiting(WaitingState),
    Running(RunningState),
}

const TICK_RATE: f32 = 1.0 / 60.0;

struct EngineHandler {
    app: Box<dyn App>,
    initial_pipeline: Option<PipelineFactory>,
    loader_registry: Option<LoaderRegistry>,
    flags: EngineFlags,
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
                    .with_title("engine-core")
                    .with_inner_size(winit::dpi::LogicalSize::new(1280u32, 720u32))
                    .with_visible(false),
            )
            .expect("Failed to create window");

        let size = window.inner_size();
        let output_size = (size.width as f32, size.height as f32);

        use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};
        let display = window.display_handle().unwrap().as_raw();
        let whandle = window.window_handle().unwrap().as_raw();

        let (cmd_tx, cmd_rx) = mpsc::channel::<RenderCommand>();
        let (upload_tx, upload_rx) = mpsc::channel::<GpuUploadRequest>();
        let (ready_tx, ready_rx) = mpsc::sync_channel::<()>(1);

        let triple_buf = Arc::new(TripleBuffer::<RenderWorld>::new());
        let triple_buf_render = Arc::clone(&triple_buf);

        let loader_registry = self.loader_registry.take().expect("loader_registry already used");

        let frame_stats = FrameStats::new();

        let mut ctx =
            EngineContext::new(cmd_tx, upload_tx, triple_buf, output_size, loader_registry, frame_stats.clone())
                .expect("Failed to create EngineContext");

        self.app.on_start(&mut ctx);
        ctx.publish_frame([0.0, 0.0, 0.0, 1.0]);

        let initial_pipeline = self.initial_pipeline.take().expect("initial_pipeline already used");

        let handles = WindowHandles { display, window: whandle };
        let flags = self.flags;
        let render_thread = std::thread::Builder::new()
            .name("render".into())
            .spawn(move || {
                render_thread_main(
                    handles,
                    flags,
                    initial_pipeline,
                    triple_buf_render,
                    frame_stats,
                    cmd_rx,
                    upload_rx,
                    ready_tx,
                );
            })
            .expect("Failed to spawn render thread");

        self.state = Some(EngineState::Waiting(WaitingState { window, ctx, ready_rx, render_thread }));
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: winit::window::WindowId, event: WindowEvent) {
        if matches!(self.state, Some(EngineState::Waiting(_))) {
            let mut waiting = match self.state.take().unwrap() {
                EngineState::Waiting(w) => w,
                _ => unreachable!(),
            };

            match waiting.ready_rx.try_recv() {
                Ok(()) => {
                    waiting.window.set_visible(true);
                    let WaitingState { window, ctx, render_thread, .. } = waiting;

                    self.state = Some(EngineState::Running(RunningState {
                        window,
                        ctx,
                        render_thread,
                        last: Instant::now(),
                        tick_accumulator: 0.0,
                    }));
                }
                Err(_) => {
                    waiting.ctx.publish_frame([0.0, 0.0, 0.0, 1.0]);
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
                state.ctx.poll_assets();

                let now = Instant::now();
                let dt = now.duration_since(state.last).as_secs_f32().min(0.1);
                state.last = now;

                state.tick_accumulator += dt;
                while state.tick_accumulator >= TICK_RATE {
                    self.app.on_update(&mut state.ctx, TICK_RATE);
                    state.ctx.publish_frame([0.0, 0.0, 0.0, 1.0]);
                    state.tick_accumulator -= TICK_RATE;
                }

                self.app.on_render(&mut state.ctx);

                state.window.request_redraw();
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        match &self.state {
            Some(EngineState::Waiting(s)) => s.window.request_redraw(),
            Some(EngineState::Running(s)) => s.window.request_redraw(),
            None => {}
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
