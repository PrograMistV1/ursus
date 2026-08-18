pub mod builtin_shaders;
pub mod deferred;
pub mod loading;
pub mod passes;
pub mod plugins;
mod systems;

pub use deferred::DefaultPipeline;
pub use loading::LoadingPipeline;
