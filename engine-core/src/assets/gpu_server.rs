use crate::assets::asset_registry::TextureHandle;
use crate::assets::material::MaterialPayload;
use crate::assets::mesh::{CpuMesh, GpuMesh};
use crate::assets::ShaderRegistry;
use crate::components::mesh::{MaterialHandle, MeshHandle};
use crate::render::gfx::{
    sampler, BlendState, DescriptorAllocator, DescriptorSetId, Format, PushConstantRange, SamplerDesc, SamplerId,
    TechniqueRegistry, VertexLayout,
};
use crate::render::gfx::{PipelineCache, PipelineId};
use crate::vulkan::gfx_pipeline::pipeline::PipelineDesc;
use crate::vulkan::{BindlessSet, GpuTexture, MappedGpuBuffer};
use ash::vk;
use std::collections::HashMap;

pub const BINDLESS_SLOT_WHITE: u32 = 0;

enum GpuMeshState {
    Ready(Box<GpuMesh>),
    Failed,
}

struct StoredSampler {
    handle: vk::Sampler,
}

pub struct GpuAssetServer {
    gpu_meshes: HashMap<MeshHandle, GpuMeshState>,
    texture_slots: HashMap<TextureHandle, u32>,
    gpu_textures: HashMap<u32, GpuTexture>,

    material_payloads: HashMap<MaterialHandle, Box<dyn MaterialPayload>>,
    material_textures: HashMap<MaterialHandle, Vec<(String, TextureHandle)>>,

    pub shaders: ShaderRegistry,
    pub techniques: TechniqueRegistry,

    pub descriptors: DescriptorAllocator,
    samplers: Vec<StoredSampler>,
    pipeline_cache: PipelineCache,
    pub bindless: BindlessSet,
    bindless_set_id: DescriptorSetId,

    device: ash::Device,
    physical_device: vk::PhysicalDevice,
    instance: ash::Instance,
    command_pool: vk::CommandPool,
    queue: vk::Queue,
}

impl GpuAssetServer {
    pub fn new(
        device: ash::Device,
        physical_device: vk::PhysicalDevice,
        instance: ash::Instance,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
    ) -> anyhow::Result<Self> {
        let bindless = BindlessSet::new(&device, physical_device, &instance, command_pool, queue)?;
        assert_eq!(bindless.next_slot(), 1, "slot 0 must be white fallback");

        let shaders = ShaderRegistry::empty();
        let techniques = TechniqueRegistry::default();
        let pipeline_cache = PipelineCache::new(device.clone());
        let mut descriptors = DescriptorAllocator::new(device.clone());

        let bindless_set_id = descriptors.register_external(bindless.layout, bindless.set, bindless.pool);

        log::info!("GpuAssetServer: white=slot0, next_slot={}", bindless.next_slot());

        Ok(Self {
            gpu_meshes: HashMap::new(),
            texture_slots: HashMap::new(),
            gpu_textures: HashMap::new(),
            material_payloads: HashMap::new(),
            material_textures: HashMap::new(),
            samplers: Vec::new(),
            shaders,
            techniques,
            bindless,
            pipeline_cache,
            device,
            physical_device,
            instance,
            command_pool,
            queue,
            descriptors,
            bindless_set_id,
        })
    }

    pub fn create_sampler(&mut self, desc: SamplerDesc) -> anyhow::Result<SamplerId> {
        let handle = sampler::create_from_desc(&self.device, desc)?;
        let id = SamplerId(self.samplers.len() as u32);
        self.samplers.push(StoredSampler { handle });
        Ok(id)
    }

    pub(crate) fn sampler_handle(&self, id: SamplerId) -> vk::Sampler {
        self.samplers[id.0 as usize].handle
    }

    pub fn bindless_set(&self) -> DescriptorSetId {
        self.bindless_set_id
    }

    pub fn bind_uniform_buffer(&self, set: DescriptorSetId, binding: u32, buffer: vk::Buffer, size: vk::DeviceSize) {
        self.descriptors.bind_uniform_buffer(set, binding, buffer, size).expect("bind_uniform_buffer failed");
    }

    pub fn bind_mapped_uniform_buffer<T: Copy>(&self, set: DescriptorSetId, binding: u32, mapped: &MappedGpuBuffer<T>) {
        self.bind_uniform_buffer(set, binding, mapped.buffer, mapped.size());
    }

    pub fn bind_storage_buffer(&self, set: DescriptorSetId, binding: u32, buffer: vk::Buffer, size: vk::DeviceSize) {
        self.descriptors.bind_storage_buffer(set, binding, buffer, size).expect("bind_storage_buffer failed");
    }

    pub fn bind_mapped_storage_buffer<T: Copy>(&self, set: DescriptorSetId, binding: u32, mapped: &MappedGpuBuffer<T>) {
        self.bind_storage_buffer(set, binding, mapped.buffer, mapped.size());
    }

    pub fn bind_sampled_image(
        &self,
        set: DescriptorSetId,
        binding: u32,
        view: vk::ImageView,
        layout: vk::ImageLayout,
        sampler: SamplerId,
    ) {
        let vk_sampler = self.sampler_handle(sampler);
        self.descriptors.bind_sampled_image(set, binding, view, layout, vk_sampler).expect("bind_sampled_image failed");
    }

    pub fn create_graphics_pipeline(
        &mut self,
        desc: &PipelineDesc,
        set_layouts: &[DescriptorSetId],
    ) -> anyhow::Result<PipelineId> {
        let layouts: Vec<vk::DescriptorSetLayout> = set_layouts.iter().map(|&id| self.descriptors.layout(id)).collect();
        self.pipeline_cache.create_graphics_pipeline(&self.device, desc, &layouts)
    }

    pub fn create_fullscreen_pipeline(
        &mut self,
        vert_spv: &[u8],
        frag_spv: &[u8],
        color_formats: &[Format],
        set_layouts: &[DescriptorSetId],
        push_constant_ranges: &[PushConstantRange],
        blend_attachments: Option<&[BlendState]>,
    ) -> anyhow::Result<PipelineId> {
        let layouts: Vec<vk::DescriptorSetLayout> = set_layouts.iter().map(|&id| self.descriptors.layout(id)).collect();

        let vk_blend: Option<Vec<vk::PipelineColorBlendAttachmentState>> =
            blend_attachments.map(|states| states.iter().map(|s| s.to_vk()).collect());

        self.pipeline_cache.create_fullscreen_pipeline(
            &self.device,
            vert_spv,
            frag_spv,
            color_formats,
            &layouts,
            push_constant_ranges,
            vk_blend.as_deref(),
        )
    }

    pub fn create_depth_only_pipeline(
        &mut self,
        vert_spv: &[u8],
        frag_spv: Option<&[u8]>,
        vertex_layout: &VertexLayout,
        push_constant_ranges: &[PushConstantRange],
        set_layouts: &[DescriptorSetId],
        depth_bias: Option<(f32, f32)>,
    ) -> anyhow::Result<PipelineId> {
        let layouts: Vec<vk::DescriptorSetLayout> = set_layouts.iter().map(|&id| self.descriptors.layout(id)).collect();
        self.pipeline_cache.create_depth_only_pipeline(
            &self.device,
            vert_spv,
            frag_spv,
            vertex_layout,
            push_constant_ranges,
            &layouts,
            depth_bias,
        )
    }

    pub fn create_mapped_buffer<T: Copy>(
        &self,
        usage: crate::render::gfx::BufferUsage,
        capacity: usize,
    ) -> anyhow::Result<MappedGpuBuffer<T>> {
        MappedGpuBuffer::new(&self.device, self.physical_device, &self.instance, usage.to_vk(), capacity)
    }

    pub fn upload_mesh(&mut self, handle: MeshHandle, cpu_mesh: &CpuMesh) -> anyhow::Result<()> {
        match GpuMesh::upload(
            &self.device,
            self.physical_device,
            &self.instance,
            cpu_mesh,
            self.command_pool,
            self.queue,
        ) {
            Ok(gpu) => {
                self.gpu_meshes.insert(handle, GpuMeshState::Ready(Box::new(gpu)));
                Ok(())
            }
            Err(e) => {
                self.gpu_meshes.insert(handle, GpuMeshState::Failed);
                Err(e)
            }
        }
    }

    pub fn upload_texture(
        &mut self,
        handle: TextureHandle,
        pixels: &[u8],
        width: u32,
        height: u32,
        format: Format,
        name: &str,
    ) -> anyhow::Result<()> {
        let tex = GpuTexture::upload(
            &self.device,
            self.physical_device,
            &self.instance,
            self.command_pool,
            self.queue,
            pixels,
            width,
            height,
            format,
            name,
        )?;
        let slot = self.bindless.alloc_slot(tex.view);
        self.texture_slots.insert(handle, slot);
        self.gpu_textures.insert(slot, tex);
        log::debug!("Texture '{}': handle={} -> slot={}", name, handle.0, slot);
        Ok(())
    }

    pub fn texture_slot(&self, handle: TextureHandle) -> u32 {
        self.texture_slots.get(&handle).copied().unwrap_or(BINDLESS_SLOT_WHITE)
    }

    pub fn register_material_payload(
        &mut self,
        handle: MaterialHandle,
        payload: Box<dyn MaterialPayload>,
        texture_slots: Vec<(String, TextureHandle)>,
    ) {
        self.material_payloads.insert(handle, payload);
        self.material_textures.insert(handle, texture_slots);
    }

    pub fn get_material<T: 'static>(&self, handle: MaterialHandle) -> Option<&T> {
        self.material_payloads.get(&handle)?.as_ref().as_any().downcast_ref::<T>()
    }

    pub fn material_textures(&self, handle: MaterialHandle) -> &[(String, TextureHandle)] {
        self.material_textures.get(&handle).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn material_handles(&self) -> impl Iterator<Item = MaterialHandle> + '_ {
        self.material_payloads.keys().copied()
    }

    pub fn get_gpu_mesh(&self, handle: MeshHandle) -> Option<&GpuMesh> {
        match self.gpu_meshes.get(&handle)? {
            GpuMeshState::Ready(gpu) => Some(gpu),
            GpuMeshState::Failed => None,
        }
    }

    pub fn pipeline_cache(&self) -> &PipelineCache {
        &self.pipeline_cache
    }

    pub fn device(&self) -> &ash::Device {
        &self.device
    }

    pub fn is_mesh_ready(&self, handle: MeshHandle) -> bool {
        matches!(self.gpu_meshes.get(&handle), Some(GpuMeshState::Ready(_)))
    }

    pub fn command_pool(&self) -> vk::CommandPool {
        self.command_pool
    }
}

impl Drop for GpuAssetServer {
    fn drop(&mut self) {
        unsafe {
            for s in &self.samplers {
                self.device.destroy_sampler(s.handle, None);
            }
        }
    }
}
