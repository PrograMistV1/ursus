use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Instant;

use winit::{application::ApplicationHandler, event::WindowEvent, event_loop::ActiveEventLoop, window::Window};

use crate::app::context::{EngineContext, WindowCommand};
use crate::app::traits::App;
use crate::app::window_config::WindowConfig;
use crate::assets::loader_registry::LoaderRegistry;
use crate::assets::upload::GpuUploadRequest;
use crate::render::frame_stats::FrameStats;
use crate::render::thread::command::{PipelineFactory, RenderCommand};
use crate::render::thread::{render_thread_main, WindowHandles};
use crate::render::triple_buffer::TripleBuffer;
use crate::render::world::RenderWorld;
use crate::EngineFlags;

struct RenderLoopState {
    last: Instant,
    tick_accumulator: f32,
    tick_duration: f32,
}

struct EngineState {
    window: Window,
    ctx: EngineContext,
    render_thread: JoinHandle<()>,
    ready_rx: Receiver<()>,
    render_loop: Option<RenderLoopState>,
    window_cmd_rx: Receiver<WindowCommand>,
    rendering_paused: bool,
}

impl EngineState {
    fn try_start_render_loop(&mut self, tick_rate: f32) -> bool {
        if self.render_loop.is_some() {
            return true;
        }
        match self.ready_rx.try_recv() {
            Ok(()) => {
                self.window.set_visible(true);
                self.render_loop = Some(RenderLoopState {
                    last: Instant::now(),
                    tick_accumulator: 0.0,
                    tick_duration: 1.0 / tick_rate,
                });
                true
            }
            Err(_) => false,
        }
    }

    fn set_rendering_paused(&mut self, paused: bool) {
        if self.rendering_paused == paused {
            return;
        }
        self.rendering_paused = paused;
        self.ctx.send_render_cmd(RenderCommand::SetPaused(paused));
        log::debug!("Rendering paused={paused}");
    }

    fn drain_window_commands(&mut self) {
        while let Ok(cmd) = self.window_cmd_rx.try_recv() {
            match cmd {
                WindowCommand::SetTitle(t) => self.window.set_title(&t),
                WindowCommand::SetSize(w, h) => {
                    let _ = self.window.request_inner_size(winit::dpi::LogicalSize::new(w, h));
                }
                WindowCommand::SetFullscreen(true) => {
                    self.window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
                }
                WindowCommand::SetFullscreen(false) => self.window.set_fullscreen(None),
            }
        }
    }
}

pub(crate) struct EngineHandler {
    pub app: Box<dyn App>,
    pub initial_pipeline: Option<PipelineFactory>,
    pub loader_registry: Option<LoaderRegistry>,
    pub flags: EngineFlags,
    window_config: WindowConfig,
    state: Option<EngineState>,
}

impl EngineHandler {
    pub fn new(
        app: Box<dyn App>,
        initial_pipeline: PipelineFactory,
        loader_registry: LoaderRegistry,
        flags: EngineFlags,
        window_config: WindowConfig,
    ) -> Self {
        Self {
            app,
            initial_pipeline: Some(initial_pipeline),
            loader_registry: Some(loader_registry),
            flags,
            state: None,
            window_config,
        }
    }
}

impl ApplicationHandler for EngineHandler {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        let window =
            event_loop.create_window(self.window_config.to_winit_attributes()).expect("Failed to create window");

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

        let (window_cmd_tx, window_cmd_rx) = mpsc::channel::<WindowCommand>();

        let mut ctx = EngineContext::new(
            cmd_tx,
            upload_tx,
            triple_buf,
            output_size,
            loader_registry,
            frame_stats.clone(),
            window_cmd_tx,
        )
        .expect("Failed to create EngineContext");

        self.app.on_start(&mut ctx);
        ctx.publish_frame([0.0, 0.0, 0.0, 1.0], 1.0);

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

        self.state = Some(EngineState {
            window,
            ctx,
            render_thread,
            ready_rx,
            render_loop: None,
            rendering_paused: false,
            window_cmd_rx,
        });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: winit::window::WindowId, event: WindowEvent) {
        let Some(state) = self.state.as_mut() else { return };
        state.drain_window_commands();

        if let WindowEvent::CloseRequested = event {
            self.app.on_stop(&mut state.ctx);
            let _ = state.ctx.cmd_tx.send(RenderCommand::Shutdown);
            if let Some(state) = self.state.take() {
                state.render_thread.join().ok();
            }
            event_loop.exit();
            return;
        }

        if let WindowEvent::Focused(focused) = event {
            state.set_rendering_paused(!focused);
        }

        if !state.try_start_render_loop(self.app.tick_rate()) {
            state.ctx.publish_frame([0.0, 0.0, 0.0, 1.0], 1.0);
            return;
        }

        let Some(rl) = state.render_loop.as_mut() else { return };

        match event {
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
                let dt = now.duration_since(rl.last).as_secs_f32().min(0.1);
                rl.last = now;

                rl.tick_accumulator += dt;
                while rl.tick_accumulator >= rl.tick_duration {
                    state.ctx.tick_schedule.run(&mut state.ctx.world, rl.tick_duration);
                    self.app.on_update(&mut state.ctx, rl.tick_duration);
                    rl.tick_accumulator -= rl.tick_duration;
                }

                let alpha = (rl.tick_accumulator / rl.tick_duration).clamp(0.0, 1.0);

                // Skip publishing/redraw-request while rendering is paused -
                // logic still ticked above, only presentation is suppressed.
                if !state.rendering_paused {
                    state.ctx.publish_frame([0.0, 0.0, 0.0, 1.0], alpha);
                    self.app.on_render(&mut state.ctx);
                }

                state.window.request_redraw();
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = &mut self.state {
            state.window.request_redraw();
            state.drain_window_commands()
        }
    }
}
