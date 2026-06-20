pub mod command;

use std::sync::mpsc::Receiver;
use std::sync::Arc;

use raw_window_handle::{RawDisplayHandle, RawWindowHandle};

use crate::assets::gpu_server::GpuAssetServer;
use crate::assets::upload::GpuUploadRequest;
use crate::pipeline::{DefaultPipeline, LoadingPipeline};
use crate::render_world::{ExtractedRenderSettings, RenderWorld};
use crate::triple_buffer::TripleBuffer;
use crate::vulkan::renderer::{build_dyn_renderer, DynRenderer};
use crate::vulkan::VulkanContext;

use self::command::RenderCommand;

/// Хэндлы окна для передачи в рендер-поток.
///
/// # Safety
/// `Window` в главном потоке живёт дольше рендер-потока —
/// гарантировано порядком drop в `Engine::run` (join перед drop window).
pub struct WindowHandles {
    pub display: RawDisplayHandle,
    pub window: RawWindowHandle,
}

unsafe impl Send for WindowHandles {}

/// Какой пайплайн сейчас активен.
enum ActivePipeline {
    Loading,
    Default,
}

/// Точка входа рендер-потока.
///
/// Создаёт `VulkanContext`, `GpuAssetServer` и `Renderer` внутри потока.
/// После инициализации отправляет сигнал в главный поток через `ready_tx`.
pub fn render_thread_main(
    handles: WindowHandles,
    triple_buf: Arc<TripleBuffer<RenderWorld>>,
    cmd_rx: Receiver<RenderCommand>,
    upload_rx: Receiver<GpuUploadRequest>,
    ready_tx: std::sync::mpsc::SyncSender<()>,
) {
    if let Err(e) = render_loop(handles, triple_buf, cmd_rx, upload_rx, ready_tx) {
        log::error!("Render thread завершился с ошибкой: {e}");
    }
}

fn render_loop(
    handles: WindowHandles,
    triple_buf: Arc<TripleBuffer<RenderWorld>>,
    cmd_rx: Receiver<RenderCommand>,
    upload_rx: Receiver<GpuUploadRequest>,
    ready_tx: std::sync::mpsc::SyncSender<()>,
) -> anyhow::Result<()> {
    // ── Инициализация ─────────────────────────────────────────────────────────
    let mut vk = VulkanContext::from_handles(handles.display, handles.window, cfg!(debug_assertions))?;

    let temp_pool = crate::app::create_temp_pool(&vk)?;

    let mut gpu_assets = GpuAssetServer::new(
        vk.device.handle.clone(),
        vk.device.physical,
        vk.instance.handle.clone(),
        temp_pool,
        vk.device.graphics_queue,
    )?;

    // Начинаем с LoadingPipeline — главный поток ещё загружает ассеты.
    let mut renderer: Box<dyn DynRenderer> = build_dyn_renderer::<LoadingPipeline>(&vk, &mut gpu_assets, 0.5, 0.2)?;
    let mut active_pipeline = ActivePipeline::Loading;

    let mut render_idx: usize = 2; // начальный render-слот тройного буфера
    let mut initialized = false; // получили ли хоть один кадр

    // Сигнализируем главному потоку что Vulkan готов — окно можно показывать.
    let _ = ready_tx.send(());

    // ── Основной цикл ─────────────────────────────────────────────────────────
    loop {
        // 1. Обработать команды управления
        loop {
            match cmd_rx.try_recv() {
                Ok(cmd) => match cmd {
                    RenderCommand::Shutdown => {
                        log::info!("Render thread: получен Shutdown");
                        unsafe { vk.device.handle.device_wait_idle().ok() };
                        return Ok(());
                    }
                    RenderCommand::Resize { width, height } => {
                        unsafe { vk.device.handle.device_wait_idle().ok() };
                        vk.recreate_swapchain(width, height, false)?;
                        renderer.resize_output(width, height)?;
                        log::debug!("Render thread: resize {width}x{height}");
                    }
                    RenderCommand::SetInternalScale(scale) => {
                        let sw = vk.swapchain.as_ref().unwrap();
                        let w = (sw.extent.width as f32 * scale) as u32;
                        let h = (sw.extent.height as f32 * scale) as u32;
                        renderer.resize_internal(w.max(1), h.max(1))?;
                    }
                    RenderCommand::SetExposure(v) => renderer.set_exposure(v),
                    RenderCommand::SetFsrSharpness(v) => renderer.set_fsr_sharpness(v),
                    RenderCommand::SetPipeline(kind) => match kind {
                        command::PipelineKind::Loading => {
                            if !matches!(active_pipeline, ActivePipeline::Loading) {
                                unsafe { vk.device.handle.device_wait_idle().ok() };
                                let prev_exp = renderer.exposure();
                                let prev_fsr = renderer.fsr_sharpness();
                                renderer =
                                    build_dyn_renderer::<LoadingPipeline>(&vk, &mut gpu_assets, prev_exp, prev_fsr)?;
                                active_pipeline = ActivePipeline::Loading;
                                log::info!("Render thread: switched to LoadingPipeline");
                            }
                        }
                        command::PipelineKind::Default => {
                            if !matches!(active_pipeline, ActivePipeline::Default) {
                                unsafe { vk.device.handle.device_wait_idle().ok() };
                                let prev_exp = renderer.exposure();
                                let prev_fsr = renderer.fsr_sharpness();
                                renderer =
                                    build_dyn_renderer::<DefaultPipeline>(&vk, &mut gpu_assets, prev_exp, prev_fsr)?;
                                active_pipeline = ActivePipeline::Default;
                                log::info!("Render thread: switched to DefaultPipeline");
                            }
                        }
                    },
                },
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    log::warn!("Render thread: cmd канал закрыт, завершаемся");
                    return Ok(());
                }
            }
        }

        // 2. GPU upload новых ассетов
        flush_uploads_gpu(&upload_rx, &mut gpu_assets, &vk)?;

        // 3. Попытаться забрать новый кадр
        let got_new = triple_buf.consume(&mut render_idx);
        if got_new {
            initialized = true;
        }

        // 4. Пропустить кадр если ещё ни одного не было
        if !initialized {
            std::thread::yield_now();
            continue;
        }

        // 5. Рендер
        let render_world = triple_buf.render_slot(render_idx);

        let settings = render_world.get::<ExtractedRenderSettings>().cloned().unwrap_or_default();

        let clear_color = settings.clear_color;

        // upload_materials каждый кадр — материалы могут меняться
        gpu_assets.upload_materials_from_render_world(render_world);

        let needs_recreate = renderer.draw_frame(&vk, render_world, &mut gpu_assets, clear_color)?;

        if needs_recreate {
            let sw = vk.swapchain.as_ref().unwrap();
            let (w, h) = (sw.extent.width, sw.extent.height);
            unsafe { vk.device.handle.device_wait_idle().ok() };
            vk.recreate_swapchain(w, h, false)?;
            renderer.resize_output(w, h)?;
        }
    }
}

/// GPU upload: читаем из канала и создаём GPU ресурсы.
fn flush_uploads_gpu(
    rx: &Receiver<GpuUploadRequest>,
    gpu: &mut GpuAssetServer,
    vk: &VulkanContext,
) -> anyhow::Result<()> {
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
                    match gpu.upload_texture_raw(&pixels, width, height, format, &name) {
                        Ok(tex_handle) => {
                            // handle уже зарезервирован в CpuAssetServer —
                            // проверяем что они совпадают
                            debug_assert_eq!(tex_handle, handle);
                        }
                        Err(e) => log::error!("GPU upload texture failed: {e}"),
                    }
                }
                GpuUploadRequest::Material {
                    handle,
                    base_color,
                    metallic,
                    roughness,
                    emissive,
                    texture_slots,
                    name,
                } => {
                    // Материал регистрируется локально в gpu_assets
                    gpu.register_material_gpu(handle, base_color, metallic, roughness, emissive, texture_slots, name);
                }
                GpuUploadRequest::FontAtlas { pixels, width, height } => {
                    if let Err(e) = gpu.upload_font_atlas_raw(pixels, width, height) {
                        log::error!("GPU upload font atlas failed: {e}");
                    }
                }
            },
            Err(std::sync::mpsc::TryRecvError::Empty) => break,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
        }
    }
    Ok(())
}
