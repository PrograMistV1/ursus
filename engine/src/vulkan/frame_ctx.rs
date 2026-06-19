use crate::assets::gpu_server::GpuAssetServer;
use crate::assets::CpuAssetServer;
use crate::lighting::LightingUbo;
use crate::vulkan::passes::geometry::DrawCall;
pub use crate::vulkan::passes::geometry::DrawCall as FrameDrawCall;
use crate::vulkan::timestamps::GpuTimestampPool;
use crate::vulkan::Camera;
use ash::vk;
use glam::Mat4;

pub struct FrameCtx<'a> {
    pub device: &'a ash::Device,

    pub draw_calls: Vec<DrawCall<'a>>,
    pub shadow_calls: Vec<DrawCall<'a>>,

    pub camera: &'a Camera,
    pub view_proj: Mat4,
    pub light_view_proj: Mat4,

    pub lighting: &'a LightingUbo,

    pub swapchain_image: vk::Image,
    pub swapchain_view: vk::ImageView,
    pub swapchain_extent: vk::Extent2D,

    pub internal_resolution: (u32, u32),
    pub output_resolution: (u32, u32),
    pub fsr_sharpness: f32,

    pub exposure: f32,
    pub clear_color: [f32; 4],

    pub cpu_assets: &'a CpuAssetServer,
    pub gpu_assets: &'a GpuAssetServer,

    pub graphics_queue: vk::Queue,
    pub command_pool: vk::CommandPool,

    pub timestamps: *const GpuTimestampPool,
    pub frame_index: usize,
}

impl<'a> FrameCtx<'a> {
    #[inline]
    pub unsafe fn from_ptr<'b>(ptr: *mut ()) -> &'b mut FrameCtx<'b> {
        &mut *(ptr as *mut FrameCtx<'b>)
    }

    pub fn as_ptr(&mut self) -> *mut () {
        self as *mut FrameCtx as *mut ()
    }
}
