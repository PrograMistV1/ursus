use crate::assets::cpu_server::CpuAssetServer;
use crate::assets::gpu_server::GpuAssetServer;
use crate::ecs::GameWorld;
use crate::egui_layer::EguiLayer;
use crate::lighting::LightingUbo;
use crate::render_graph::{RenderGraph, ResourceHandle};
use crate::vulkan::{Camera, VulkanContext};
use ash::vk;
use glam::Mat4;

pub struct FrameInput<'a> {
    pub device: &'a ash::Device,
    pub world: &'a mut GameWorld,
    pub cpu_assets: &'a mut CpuAssetServer,
    pub gpu_assets: &'a mut GpuAssetServer,
    pub camera: &'a Camera,
    pub lighting: &'a LightingUbo,
    pub view_proj: Mat4,
    pub light_view_proj: Mat4,
    pub egui: &'a mut EguiLayer,
    pub egui_output: egui::FullOutput,
    pub window: &'a winit::window::Window,
    pub graphics_queue: vk::Queue,
    pub command_pool: vk::CommandPool,
    pub exposure: f32,
    pub clear_color: [f32; 4],
    pub internal_resolution: (u32, u32),
    pub output_resolution: (u32, u32),
    pub fsr_sharpness: f32,
}

pub trait RenderPipeline: Send + 'static {
    fn build(
        ctx: &VulkanContext,
        cpu_assets: &mut CpuAssetServer,
        gpu_assets: &mut GpuAssetServer,
        graph: &mut RenderGraph,
    ) -> anyhow::Result<PipelineHandles>
    where
        Self: Sized;

    fn prepare_frame(&mut self, graph: &mut RenderGraph, input: FrameInput<'_>) -> anyhow::Result<()>;
    fn on_resize(&mut self, _graph: &mut RenderGraph, _width: u32, _height: u32) -> anyhow::Result<()> {
        Ok(())
    }
    fn on_resize_internal(
        &mut self,
        _graph: &mut RenderGraph,
        _new_width: u32,
        _new_height: u32,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}

pub struct PipelineHandles {
    pub swapchain: ResourceHandle,
}
