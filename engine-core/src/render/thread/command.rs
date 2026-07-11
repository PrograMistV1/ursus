use crate::assets::gpu_server::GpuAssetServer;
use crate::assets::loader_registry::LoaderRegistry;
use crate::render::frame_pipeline::render_pipeline::{NoopPipeline, RenderPipeline};
use crate::vulkan::renderer::{build_dyn_renderer, DynRenderer};
use crate::vulkan::VulkanContext;

pub struct PipelineFactory {
    build:
        Box<dyn FnOnce(&VulkanContext, &mut GpuAssetServer, f32, f32) -> anyhow::Result<Box<dyn DynRenderer>> + Send>,
    register_loaders_fn: fn(&mut LoaderRegistry),
}

impl PipelineFactory {
    pub fn of<P>() -> Self
    where
        P: RenderPipeline + Default + 'static,
    {
        Self {
            build: Box::new(|ctx, gpu_assets, exposure, fsr_sharpness| {
                build_dyn_renderer::<P>(ctx, gpu_assets, exposure, fsr_sharpness)
            }),
            register_loaders_fn: P::register_loaders,
        }
    }

    pub fn register_loaders(&self, registry: &mut LoaderRegistry) {
        (self.register_loaders_fn)(registry);
    }

    pub fn build(
        self,
        ctx: &VulkanContext,
        gpu_assets: &mut GpuAssetServer,
        prev_exposure: f32,
        prev_fsr_sharpness: f32,
    ) -> anyhow::Result<Box<dyn DynRenderer>> {
        (self.build)(ctx, gpu_assets, prev_exposure, prev_fsr_sharpness)
    }

    pub fn empty() -> Self {
        Self::of::<NoopPipeline>()
    }
}

impl std::fmt::Debug for PipelineFactory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PipelineFactory(..)")
    }
}

#[derive(Debug)]
pub enum RenderCommand {
    Resize { width: u32, height: u32 },
    SetInternalScale(f32),
    SetExposure(f32),
    SetFsrSharpness(f32),
    SetPipeline(PipelineFactory),
    Shutdown,
}
