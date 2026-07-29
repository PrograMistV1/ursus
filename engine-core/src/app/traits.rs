use crate::app::context::EngineContext;
use crate::app::window_config::WindowConfig;
use crate::assets::loader_registry::LoaderRegistry;
use crate::render::thread::command::PipelineFactory;

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

    fn window_config() -> WindowConfig
    where
        Self: Sized,
    {
        WindowConfig::default()
    }

    fn tick_rate(&self) -> f32 {
        60.0
    }

    fn on_start(&mut self, ctx: &mut EngineContext);
    fn on_update(&mut self, ctx: &mut EngineContext, dt: f32);
    fn on_render(&mut self, ctx: &mut EngineContext);
    fn on_stop(&mut self, ctx: &mut EngineContext);
}
