use crate::assets::mesh::Vertex;
use crate::assets::shader_registry::TextureSlot;
use crate::assets::TextureHandle;
use crate::ecs::components::{MaterialHandle, MeshHandle};
use ash::vk;

/// CPU-готовые данные для GPU upload.
///
/// Создаётся в главном потоке после декодирования файла (парсинг GLTF,
/// распаковка пикселей). Отправляется в рендер-поток через `mpsc::Sender`.
/// Рендер-поток создаёт GPU ресурсы через `GpuAssetServer::flush_uploads_gpu`.
#[derive(Debug)]
pub enum GpuUploadRequest {
    Mesh {
        handle: MeshHandle,
        vertices: Vec<Vertex>,
        indices: Vec<u32>,
        name: String,
    },
    Texture {
        handle: TextureHandle,
        pixels: Vec<u8>,
        width: u32,
        height: u32,
        format: vk::Format,
        name: String,
    },
    /// Материал не требует GPU upload — хранится в `MaterialBuffer` который
    /// обновляется каждый кадр через `upload_materials`. Тем не менее рендер-поток
    /// должен знать о новом материале чтобы зарезервировать слот.
    Material {
        handle: MaterialHandle,
        base_color: [f32; 4],
        metallic: f32,
        roughness: f32,
        emissive: [f32; 4],
        /// Текстуры уже загружены отдельными `Texture` запросами.
        texture_slots: Vec<(TextureSlot, TextureHandle)>,
        name: String,
    },
    /// Загрузка font atlas завершена.
    FontAtlas { pixels: Vec<u8>, width: u32, height: u32 },
}
