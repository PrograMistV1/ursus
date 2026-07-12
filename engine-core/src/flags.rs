use clap::Parser;

#[derive(Parser, Debug, Clone, Copy)]
#[command(name = "engine", about = "Ursus engine runtime flags")]
pub struct EngineFlags {
    /// Enable puffin profiler (starts puffin_http server)
    #[arg(long, default_value_t = cfg!(debug_assertions))]
    pub profile: bool,

    /// Enable Vulkan validation layers
    #[arg(long, default_value_t = cfg!(debug_assertions))]
    pub validation: bool,

    /// Enable Vulkan debug_utils labels (RenderDoc/Nsight object names)
    #[arg(long, default_value_t = cfg!(debug_assertions))]
    pub debug_labels: bool,
}

impl EngineFlags {
    pub fn from_args() -> Self {
        Self::parse()
    }
}
