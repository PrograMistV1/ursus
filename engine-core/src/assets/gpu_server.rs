use crate::assets::asset_registry::TextureHandle;
use crate::assets::material::MaterialPayload;
use crate::assets::{GpuTextureStore, MeshStore, ShaderRegistry};
use crate::components::mesh::MaterialHandle;
use crate::render::gfx::{
    sampler, BlendState, DescriptorAllocator, DescriptorSetId, Format, PushConstantRange, SamplerDesc, SamplerId,
    TechniqueRegistry, VertexLayout,
};
use crate::render::gfx::{PipelineCache, PipelineId};
use crate::vulkan::gfx_pipeline::pipeline::PipelineDesc;
use crate::vulkan::MappedGpuBuffer;
use ash::vk;
use std::collections::HashMap;

pub const BINDLESS_SLOT_WHITE: u32 = 0;

struct StoredSampler {
    handle: vk::Sampler,
}

pub struct GpuAssetServer {
    pub meshes: MeshStore,
    pub textures: GpuTextureStore,

    material_payloads: HashMap<MaterialHandle, Box<dyn MaterialPayload>>,
    material_textures: HashMap<MaterialHandle, Vec<(String, TextureHandle)>>,

    pub shaders: ShaderRegistry,
    pub techniques: TechniqueRegistry,

    pub descriptors: DescriptorAllocator,
    samplers: Vec<StoredSampler>,
    pipeline_cache: PipelineCache,
    bindless_set_id: DescriptorSetId,

    device: ash::Device,
    physical_device: vk::PhysicalDevice,
    instance: ash::Instance,
    command_pool: vk::CommandPool,
}

impl GpuAssetServer {
    pub fn new(
        device: ash::Device,
        physical_device: vk::PhysicalDevice,
        instance: ash::Instance,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
    ) -> anyhow::Result<Self> {
        let textures = GpuTextureStore::new(device.clone(), physical_device, instance.clone(), command_pool, queue)?;

        let shaders = ShaderRegistry::empty();
        let techniques = TechniqueRegistry::default();
        let pipeline_cache = PipelineCache::new(device.clone());
        let mut descriptors = DescriptorAllocator::new(device.clone());
        let meshes = MeshStore::new(device.clone(), physical_device, instance.clone(), command_pool, queue);

        let bindless = textures.bindless();
        let bindless_set_id = descriptors.register_external(bindless.layout, bindless.set, bindless.pool);

        log::info!("GpuAssetServer: white=slot0, next_slot={}", bindless.next_slot());

        Ok(Self {
            meshes,
            textures,
            material_payloads: HashMap::new(),
            material_textures: HashMap::new(),
            samplers: Vec::new(),
            shaders,
            techniques,
            pipeline_cache,
            device,
            physical_device,
            instance,
            command_pool,
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

    pub fn pipeline_cache(&self) -> &PipelineCache {
        &self.pipeline_cache
    }

    pub fn device(&self) -> &ash::Device {
        &self.device
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
