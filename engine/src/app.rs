use crate::assets::AssetServer;
use crate::ecs::systems::collect_draw_calls;
use crate::ecs::GameWorld;
use crate::vulkan::frustum::transform_aabb;
use crate::vulkan::{Camera, DrawCall, Renderer, VulkanContext};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowAttributes},
};

pub trait App {
    fn on_start(&mut self, ctx: &mut EngineContext);
    fn on_update(&mut self, ctx: &mut EngineContext, dt: f32);
    fn on_render(&mut self, ctx: &mut EngineContext);
    fn on_stop(&mut self, ctx: &mut EngineContext);
}

pub struct EngineContext {
    pub world: GameWorld,
    pub assets: AssetServer,
    pub renderer: Renderer,
    pub vk: VulkanContext,
    pub camera: Camera,
    temp_pool: ash::vk::CommandPool,
}

impl EngineContext {
    fn new(vk: VulkanContext) -> anyhow::Result<Self> {
        let temp_pool = create_temp_pool(&vk)?;

        let mut assets = AssetServer::new(
            vk.device.handle.clone(),
            vk.device.physical,
            vk.instance.handle.clone(),
            temp_pool,
            vk.device.graphics_queue,
        )?;

        let renderer = Renderer::new(&vk, &mut assets)?;

        Ok(Self {
            world: GameWorld::new(),
            assets,
            renderer,
            vk,
            camera: Camera::default(),
            temp_pool,
        })
    }

    pub fn render_world(&mut self, clear_color: [f32; 4]) -> anyhow::Result<()> {
        let ecs_calls = collect_draw_calls(&mut self.world, &self.assets);

        let swapchain = self.vk.swapchain.as_ref().unwrap();
        let aspect = swapchain.extent.width as f32 / swapchain.extent.height as f32;
        let view_proj = self.camera.view_proj(aspect);
        let frustum_planes = crate::vulkan::frustum::extract_planes(view_proj);

        let device = self.vk.device.handle.clone();
        for dc in &ecs_calls {
            self.renderer.geometry.get_or_create_pipeline(&device, dc.shader, &mut self.assets.shaders)?;
        }

        let gpu_calls: Vec<DrawCall<'_>> = ecs_calls
            .iter()
            .filter_map(|dc| {
                let gpu = self.assets.get_gpu_mesh(dc.mesh)?;

                let model = dc.transform.matrix();
                let world_aabb = transform_aabb(&gpu.aabb, model);
                if !world_aabb.intersects_frustum(&frustum_planes) {
                    return None;
                }

                Some(DrawCall { gpu_mesh: gpu, transform: &dc.transform, material: dc.material, shader: dc.shader })
            })
            .collect();

        self.renderer.draw_frame(&self.vk, clear_color, &self.camera, &gpu_calls, &self.assets)
    }
}

impl Drop for EngineContext {
    fn drop(&mut self) {
        unsafe {
            self.vk.device.handle.device_wait_idle().ok();
            self.vk
                .device
                .handle
                .destroy_command_pool(self.temp_pool, None);
        }
    }
}

pub struct Engine;

impl Engine {
    pub fn run(app: impl App + 'static) -> anyhow::Result<()> {
        env_logger::builder()
            .filter_level(log::LevelFilter::Info)
            .parse_default_env()
            .init();

        let event_loop = EventLoop::new()?;
        event_loop.set_control_flow(ControlFlow::Poll);

        let mut handler = EngineHandler {
            app: Box::new(app),
            state: None,
        };
        event_loop.run_app(&mut handler)?;
        Ok(())
    }
}

struct RunningState {
    window: Window,
    ctx: EngineContext,
    last: std::time::Instant,
    fps_timer: std::time::Instant,
    fps_frames: u32,
    tick_accumulator: f32,
}

const TICK_RATE: f32 = 1.0 / 60.0;

struct EngineHandler {
    app: Box<dyn App>,
    state: Option<RunningState>,
}

impl ApplicationHandler for EngineHandler {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        let window = event_loop
            .create_window(
                WindowAttributes::default()
                    .with_title("engine")
                    .with_inner_size(winit::dpi::LogicalSize::new(1280u32, 720u32)),
            )
            .expect("Failed to create window");

        let vk =
            VulkanContext::new(&window, cfg!(debug_assertions)).expect("Failed to init Vulkan");
        let mut ctx = EngineContext::new(vk).expect("Failed to create EngineContext");

        self.app.on_start(&mut ctx);

        ctx.assets
            .upload_all_meshes()
            .expect("Failed to upload meshes");

        log::info!(
            "AssetServer: {} мешей, {} материалов, {} текстур",
            ctx.assets.mesh_count(),
            ctx.assets.material_count(),
            ctx.assets.texture_count(),
        );

        self.state = Some(RunningState {
            window,
            ctx,
            last: std::time::Instant::now(),
            fps_timer: std::time::Instant::now(),
            fps_frames: 0,
            tick_accumulator: 0.0,
        });
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = &mut self.state else { return };

        match event {
            WindowEvent::CloseRequested => {
                self.app.on_stop(&mut state.ctx);
                event_loop.exit();
            }
            WindowEvent::Resized(_) => {
                // TODO: пересоздать swapchain
            }
            WindowEvent::RedrawRequested => {
                let now = std::time::Instant::now();
                let dt = now.duration_since(state.last).as_secs_f32().min(0.1);
                state.last = now;
                state.fps_frames += 1;

                if now.duration_since(state.fps_timer).as_secs_f32() >= 1.0 {
                    state.fps_frames = 0;
                    state.fps_timer = now;
                }

                state.tick_accumulator += dt;
                while state.tick_accumulator >= TICK_RATE {
                    self.app.on_update(&mut state.ctx, TICK_RATE);
                    state.tick_accumulator -= TICK_RATE;
                }

                self.app.on_render(&mut state.ctx);
                state.window.request_redraw();
            }
            _ => {}
        }
    }
}

fn create_temp_pool(vk: &VulkanContext) -> anyhow::Result<ash::vk::CommandPool> {
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
