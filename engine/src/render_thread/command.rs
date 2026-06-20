#[derive(Debug)]
pub enum RenderCommand {
    /// Окно изменило размер — пересоздать swapchain и внутренние буферы.
    Resize { width: u32, height: u32 },

    /// Изменить внутреннее разрешение рендера (до апскейла FSR).
    /// Значение в диапазоне 0.0..=1.0 — масштаб от выходного разрешения.
    SetInternalScale(f32),

    /// Изменить экспозицию (для HDR tonemapping).
    SetExposure(f32),

    /// Изменить резкость FSR RCAS.
    SetFsrSharpness(f32),

    /// Завершить работу рендер-потока.
    Shutdown,
}
