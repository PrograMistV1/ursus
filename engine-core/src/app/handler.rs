use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Instant;

use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::ActiveEventLoop,
    window::{Window, WindowAttributes},
};

use crate::app::context::EngineContext;
use crate::app::traits::App;
use crate::assets::loader_registry::LoaderRegistry;
use crate::assets::upload::GpuUploadRequest;
use crate::render::frame_stats::FrameStats;
use crate::render::thread::command::{PipelineFactory, RenderCommand};
use crate::render::thread::{render_thread_main, WindowHandles};
use crate::render::triple_buffer::TripleBuffer;
use crate::render::world::RenderWorld;
use crate::EngineFlags;

enum Phase {
    WaitingForRender {
        ready_rx: Receiver<()>,
    },
    Running {
        last: Instant,
        tick_accumulator: f32,
        tick_duration: f32,
    },
}

struct EngineState {
    window: Window,
    ctx: EngineContext,
    render_thread: JoinHandle<()>,
    phase: Phase,
}

impl EngineState {
    fn poll_ready(&mut self, tick_rate: f32) -> bool {
        let Phase::WaitingForRender { ready_rx } = &self.phase else {
            return false;
        };

        match ready_rx.try_recv() {
            Ok(()) => {}
            Err(mpsc::TryRecvError::Empty) => return false,
            Err(mpsc::TryRecvError::Disconnected) => return false,
        }

        self.window.set_visible(true);
        self.phase = Phase::Running { last: Instant::now(), tick_accumulator: 0.0, tick_duration: 1.0 / tick_rate };
        true
    }
}

pub(crate) struct EngineHandler {
    pub app: Box<dyn App>,
    pub initial_pipeline: Option<PipelineFactory>,
    pub loader_registry: Option<LoaderRegistry>,
    pub flags: EngineFlags,
    state: Option<EngineState>,
}

impl EngineHandler {
    pub fn new(
        app: Box<dyn App>,
        initial_pipeline: PipelineFactory,
        loader_registry: LoaderRegistry,
        flags: EngineFlags,
    ) -> Self {
        Self {
            app,
            initial_pipeline: Some(initial_pipeline),
            loader_registry: Some(loader_registry),
            flags,
            state: None,
        }
    }
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

        self.state = Some(EngineState { window, ctx, render_thread, phase: Phase::WaitingForRender { ready_rx } });
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: winit::window::WindowId, event: WindowEvent) {
        let Some(state) = self.state.as_mut() else { return };

        if let WindowEvent::CloseRequested = event {
            self.app.on_stop(&mut state.ctx);
            let _ = state.ctx.cmd_tx.send(RenderCommand::Shutdown);
            if let Some(state) = self.state.take() {
                state.render_thread.join().ok();
            }
            event_loop.exit();
            return;
        }

        if let Phase::WaitingForRender { .. } = &state.phase {
            let transitioned = state.poll_ready(self.app.tick_rate());
            if !transitioned {
                state.ctx.publish_frame([0.0, 0.0, 0.0, 1.0], 1.0);
                return;
            }
        }

        let Phase::Running { last, tick_accumulator, tick_duration } = &mut state.phase else {
            return;
        };

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
                let dt = now.duration_since(*last).as_secs_f32().min(0.1);
                *last = now;

                *tick_accumulator += dt;
                while *tick_accumulator >= *tick_duration {
                    state.ctx.tick_schedule.run(&mut state.ctx.world, *tick_duration);
                    self.app.on_update(&mut state.ctx, *tick_duration);
                    *tick_accumulator -= *tick_duration;
                }

                let alpha = (*tick_accumulator / *tick_duration).clamp(0.0, 1.0);
                state.ctx.publish_frame([0.0, 0.0, 0.0, 1.0], alpha);

                self.app.on_render(&mut state.ctx);

                state.window.request_redraw();
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = &self.state {
            state.window.request_redraw();
        }
    }
}
