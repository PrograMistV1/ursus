use std::sync::mpsc::Sender;
use std::sync::Arc;

use crate::assets::asset_registry::AssetRegistry;
use crate::assets::loader_registry::LoaderRegistry;
use crate::assets::upload::GpuUploadRequest;
use crate::ecs::tick::default_tick_schedule;
use crate::ecs::{GameWorld, TickSchedule};
use crate::render::extract::ExtractSchedule;
use crate::render::frame_pipeline::render_pipeline::RenderPipeline;
use crate::render::frame_stats::FrameStats;
use crate::render::thread::command::{PipelineFactory, RenderCommand};
use crate::render::triple_buffer::TripleBuffer;
use crate::render::world::{ExtractedRenderSettings, RenderWorld};

#[allow(clippy::enum_variant_names)]
pub enum WindowCommand {
    SetTitle(String),
    SetSize(u32, u32),
    SetFullscreen(bool), // true = borderless fullscreen
}

pub struct EngineContext {
    pub world: GameWorld,
    pub cpu_assets: AssetRegistry,
    pub extract_schedule: ExtractSchedule,
    pub tick_schedule: TickSchedule,

    pub(crate) cmd_tx: Sender<RenderCommand>,
    upload_tx: Sender<GpuUploadRequest>,
    triple_buf: Arc<TripleBuffer<RenderWorld>>,
    pub(crate) output_size: (f32, f32),
    frame_stats: FrameStats,
    pub(crate) window_cmd_tx: Sender<WindowCommand>,
}

impl EngineContext {
    pub(crate) fn new(
        cmd_tx: Sender<RenderCommand>,
        upload_tx: Sender<GpuUploadRequest>,
        triple_buf: Arc<TripleBuffer<RenderWorld>>,
        output_size: (f32, f32),
        loader_registry: LoaderRegistry,
        frame_stats: FrameStats,
        window_cmd_tx: Sender<WindowCommand>,
    ) -> anyhow::Result<Self> {
        let cpu_assets = AssetRegistry::new(loader_registry);

        Ok(Self {
            world: GameWorld::new(),
            cpu_assets,
            extract_schedule: ExtractSchedule::default(),
            tick_schedule: default_tick_schedule(),
            cmd_tx,
            upload_tx,
            triple_buf,
            output_size,
            frame_stats,
            window_cmd_tx,
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

    pub(crate) fn publish_frame(&mut self, clear_color: [f32; 4], interpolation_alpha: f32) {
        let write = self.triple_buf.write_slot();
        write.clear();
        write.insert(ExtractedRenderSettings {
            clear_color,
            output_size: self.output_size,
            fsr_sharpness: 0.2,
            exposure: 0.5,
            interpolation_alpha,
        });
        self.extract_schedule.run(&self.world, write, &mut self.cpu_assets, &self.upload_tx);
        self.triple_buf.publish();
    }

    pub fn set_window_title(&self, title: impl Into<String>) {
        let _ = self.window_cmd_tx.send(WindowCommand::SetTitle(title.into()));
    }
    pub fn set_window_size(&self, width: u32, height: u32) {
        let _ = self.window_cmd_tx.send(WindowCommand::SetSize(width, height));
    }
    pub fn set_fullscreen(&self, enabled: bool) {
        let _ = self.window_cmd_tx.send(WindowCommand::SetFullscreen(enabled));
    }
}
