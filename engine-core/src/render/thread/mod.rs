pub mod command;

use std::sync::mpsc::Receiver;
use std::sync::Arc;

use raw_window_handle::{RawDisplayHandle, RawWindowHandle};

use crate::assets::gpu_server::GpuAssetServer;
use crate::assets::upload::GpuUploadRequest;
use crate::render::triple_buffer::TripleBuffer;
use crate::render::world::RenderWorld;
use crate::vulkan::VulkanContext;

use self::command::{PipelineFactory, RenderCommand};

pub struct WindowHandles {
    pub display: RawDisplayHandle,
    pub window: RawWindowHandle,
}

unsafe impl Send for WindowHandles {}

pub fn render_thread_main(
    handles: WindowHandles,
    initial_pipeline: PipelineFactory,
    triple_buf: Arc<TripleBuffer<RenderWorld>>,
    cmd_rx: Receiver<RenderCommand>,
    upload_rx: Receiver<GpuUploadRequest>,
    ready_tx: std::sync::mpsc::SyncSender<()>,
) {
    if let Err(e) = render_loop(handles, initial_pipeline, triple_buf, cmd_rx, upload_rx, ready_tx) {
        log::error!("Render thread завершился с ошибкой: {e}");
    }
}

fn render_loop(
    handles: WindowHandles,
    initial_pipeline: PipelineFactory,
    triple_buf: Arc<TripleBuffer<RenderWorld>>,
    cmd_rx: Receiver<RenderCommand>,
    upload_rx: Receiver<GpuUploadRequest>,
    ready_tx: std::sync::mpsc::SyncSender<()>,
) -> anyhow::Result<()> {
    let mut vk = VulkanContext::from_handles(handles.display, handles.window, cfg!(debug_assertions))?;

    let temp_pool = crate::app::create_temp_pool(&vk)?;

    let mut gpu_assets = GpuAssetServer::new(
        vk.device.handle.clone(),
        vk.device.physical,
        vk.instance.handle.clone(),
        temp_pool,
        vk.device.graphics_queue,
    )?;

    let mut renderer = initial_pipeline.build(&vk, &mut gpu_assets, 0.5, 0.2)?;

    let mut render_idx: usize = 2;
    let mut initialized = false;

    let _ = ready_tx.send(());

    loop {
        puffin::GlobalProfiler::lock().new_frame();
        loop {
            match cmd_rx.try_recv() {
                Ok(cmd) => match cmd {
                    RenderCommand::Shutdown => {
                        log::info!("Render thread: получен Shutdown");
                        unsafe { vk.device.handle.device_wait_idle().ok() };
                        unsafe { vk.device.handle.destroy_command_pool(temp_pool, None) };
                        return Ok(());
                    }
                    RenderCommand::Resize { width, height } => {
                        unsafe { vk.device.handle.device_wait_idle().ok() };
                        vk.recreate_swapchain(width, height, false)?;
                        renderer.resize_output(width, height, &gpu_assets)?;
                        log::debug!("Render thread: resize {width}x{height}");
                    }
                    RenderCommand::SetInternalScale(scale) => {
                        let sw = vk.swapchain.as_ref().unwrap();
                        let w = (sw.extent.width as f32 * scale) as u32;
                        let h = (sw.extent.height as f32 * scale) as u32;
                        renderer.resize_internal(w.max(1), h.max(1), &gpu_assets)?;
                    }
                    RenderCommand::SetExposure(v) => renderer.set_exposure(v),
                    RenderCommand::SetFsrSharpness(v) => renderer.set_fsr_sharpness(v),
                    RenderCommand::SetPipeline(factory) => {
                        unsafe { vk.device.handle.device_wait_idle().ok() };
                        let prev_exp = renderer.exposure();
                        let prev_fsr = renderer.fsr_sharpness();
                        renderer = factory.build(&vk, &mut gpu_assets, prev_exp, prev_fsr)?;
                        log::info!("Render thread: frame_pipeline switched");
                    }
                },
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    log::warn!("Render thread: cmd канал закрыт, завершаемся");
                    return Ok(());
                }
            }
        }

        flush_uploads_gpu(&upload_rx, &mut gpu_assets)?;

        let got_new = triple_buf.consume(&mut render_idx);
        if got_new {
            initialized = true;
        }

        if !initialized {
            std::thread::yield_now();
            continue;
        }

        let render_world = triple_buf.render_slot(render_idx);

        let needs_recreate = renderer.draw_frame(&vk, render_world, &mut gpu_assets)?;

        if needs_recreate {
            let sw = vk.swapchain.as_ref().unwrap();
            let (w, h) = (sw.extent.width, sw.extent.height);
            unsafe { vk.device.handle.device_wait_idle().ok() };
            vk.recreate_swapchain(w, h, false)?;
            renderer.resize_output(w, h, &gpu_assets)?;
        }
    }
}

fn flush_uploads_gpu(rx: &Receiver<GpuUploadRequest>, gpu: &mut GpuAssetServer) -> anyhow::Result<()> {
    loop {
        match rx.try_recv() {
            Ok(req) => match req {
                GpuUploadRequest::Mesh { handle, vertices, indices, name } => {
                    use crate::assets::mesh::CpuMesh;
                    let cpu = CpuMesh::new(name, vertices, indices);
                    if let Err(e) = gpu.upload_mesh(handle, &cpu) {
                        log::error!("GPU upload mesh failed: {e}");
                    }
                }
                GpuUploadRequest::Texture { handle, pixels, width, height, format, name } => {
                    if let Err(e) = gpu.upload_texture(handle, &pixels, width, height, format, &name) {
                        log::error!("GPU upload texture failed: {e}");
                    }
                }
                GpuUploadRequest::Material { handle, payload, texture_slots } => {
                    gpu.register_material_payload(handle, payload, texture_slots);
                }
            },
            Err(std::sync::mpsc::TryRecvError::Empty) => break,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
        }
    }
    Ok(())
}
