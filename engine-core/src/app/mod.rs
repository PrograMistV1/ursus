mod context;
mod handler;
mod traits;
pub mod window_config;

pub use context::EngineContext;
pub use traits::App;

use handler::EngineHandler;

use crate::assets::loader_registry::LoaderRegistry;
use crate::EngineFlags;
use winit::event_loop::{ControlFlow, EventLoop};

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

        let window_config = A::window_config();
        let initial_pipeline = A::initial_pipeline();

        let mut loader_registry = LoaderRegistry::new();
        A::register_loaders(&mut loader_registry);

        let event_loop = EventLoop::new()?;
        event_loop.set_control_flow(ControlFlow::Poll);

        let mut handler = EngineHandler::new(Box::new(app), initial_pipeline, loader_registry, flags, window_config);
        event_loop.run_app(&mut handler)?;
        Ok(())
    }
}

pub fn create_temp_pool(vk: &crate::VulkanContext) -> anyhow::Result<ash::vk::CommandPool> {
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
