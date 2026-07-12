use engine_core::assets::gpu_server::GpuAssetServer;
use engine_core::components::mesh::MaterialHandle;
use engine_core::render::gfx::{BufferUsage, DescriptorSetDesc, DescriptorSetId, ShaderStage};
use engine_core::vulkan::MappedGpuBuffer;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MaterialData {
    pub base_color: [f32; 4],
    pub emissive: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub _pad: [f32; 2],
    pub tex_indices0: [u32; 4],
    pub tex_indices1: [u32; 4],
}

unsafe impl bytemuck::Pod for MaterialData {}
unsafe impl bytemuck::Zeroable for MaterialData {}

impl MaterialData {
    pub fn default_white() -> Self {
        Self {
            base_color: [1.0, 1.0, 1.0, 1.0],
            emissive: [0.0; 4],
            metallic: 0.0,
            roughness: 0.5,
            _pad: [0.0; 2],
            tex_indices0: [0; 4],
            tex_indices1: [0; 4],
        }
    }
}

pub const MAX_MATERIALS: usize = 4096;

pub struct MaterialBuffer {
    inner: MappedGpuBuffer<MaterialData>,
    pub descriptor_set: DescriptorSetId,
}

unsafe impl Send for MaterialBuffer {}
unsafe impl Sync for MaterialBuffer {}

impl MaterialBuffer {
    pub fn new(gpu: &mut GpuAssetServer) -> anyhow::Result<Self> {
        let inner = gpu.create_mapped_buffer::<MaterialData>(BufferUsage::Storage, MAX_MATERIALS)?;

        let descriptor_set = gpu.create_descriptor_set(
            DescriptorSetDesc::new().with_storage_buffer::<MaterialData>(0, ShaderStage::Fragment),
        )?;
        gpu.bind_mapped_storage_buffer(descriptor_set, 0, &inner);

        Ok(Self { inner, descriptor_set })
    }

    pub fn upload(&self, materials: &[MaterialData]) {
        self.inner.upload_slice(materials);
    }
}

pub fn resolve_material(gpu: &GpuAssetServer, handle: MaterialHandle) -> MaterialData {
    #[cfg(feature = "gltf-loader")]
    {
        use engine_gltf_loader::{PbrMetallicRoughness, UnlitMaterial};
        if let Some(pbr) = gpu.get_material::<PbrMetallicRoughness>(handle) {
            return pack_pbr(gpu, handle, pbr);
        }
        if let Some(unlit) = gpu.get_material::<UnlitMaterial>(handle) {
            return pack_unlit(gpu, handle, unlit);
        }
    }
    #[cfg(not(feature = "gltf-loader"))]
    let _ = handle;

    MaterialData::default_white()
}

#[cfg(feature = "gltf-loader")]
fn pack_pbr(
    gpu: &GpuAssetServer,
    handle: MaterialHandle,
    m: &engine_gltf_loader::PbrMetallicRoughness,
) -> MaterialData {
    let slots = gpu.material_textures(handle);
    let find = |role: &str| slots.iter().find(|(r, _)| r == role).map(|(_, h)| gpu.texture_slot(*h)).unwrap_or(0);
    MaterialData {
        base_color: m.base_color,
        emissive: [m.emissive[0], m.emissive[1], m.emissive[2], 0.0],
        metallic: m.metallic,
        roughness: m.roughness,
        _pad: [0.0; 2],
        tex_indices0: [
            find("base_color"),
            find("normal"),
            find("metallic_roughness"),
            find("emissive"),
        ],
        tex_indices1: [find("occlusion"), 0, 0, 0],
    }
}

#[cfg(feature = "gltf-loader")]
fn pack_unlit(gpu: &GpuAssetServer, handle: MaterialHandle, m: &engine_gltf_loader::UnlitMaterial) -> MaterialData {
    let slots = gpu.material_textures(handle);
    let base_color_tex = slots.iter().find(|(r, _)| r == "base_color").map(|(_, h)| gpu.texture_slot(*h)).unwrap_or(0);
    MaterialData {
        base_color: m.base_color,
        emissive: [0.0; 4],
        metallic: 0.0,
        roughness: 1.0,
        _pad: [0.0; 2],
        tex_indices0: [base_color_tex, 0, 0, 0],
        tex_indices1: [0; 4],
    }
}
