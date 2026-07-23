extern crate self as engine_core;
pub mod app;
pub mod assets;
pub mod ecs;
pub mod ffi;
pub mod flags;
pub mod math;
pub mod render;
pub mod vulkan;

pub use app::{App, Engine, EngineContext};
pub use assets::AsyncMeshHandle;
pub use ecs::{components, GameWorld};
pub use flags::EngineFlags;
pub use vulkan::VulkanContext;
