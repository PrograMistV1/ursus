use crate::assets::{GpuTextureStore, MaterialStore, MeshStore, ShaderRegistry};
use crate::render::gfx::descriptor::DescriptorAllocator;
use crate::render::gfx::sampler::SamplerStore;
use crate::render::gfx::types::{
    BlendState, BufferUsage, DescriptorSetId, Format, PipelineId, PushConstantRange, SamplerId, VertexLayout,
};
use crate::render::gfx::PipelineCache;
use crate::render::gfx::TechniqueRegistry;
use crate::vulkan::gfx_pipeline::pipeline::PipelineDesc;
use crate::vulkan::MappedGpuBuffer;
use ash::vk;

pub struct GpuAssetServer {
    pub meshes: MeshStore,
    pub textures: GpuTextureStore,
    pub materials: MaterialStore,

    pub shaders: ShaderRegistry,
    pub techniques: TechniqueRegistry,

    pub descriptors: DescriptorAllocator,
    pub samplers: SamplerStore,
    pipeline_cache: PipelineCache,

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
        let shaders = ShaderRegistry::empty();
        let techniques = TechniqueRegistry::default();
        let pipeline_cache = PipelineCache::new(device.clone());
        let mut descriptors = DescriptorAllocator::new(device.clone());
        let meshes = MeshStore::new(device.clone(), physical_device, instance.clone(), command_pool, queue);
        let samplers = SamplerStore::new(device.clone());

        let textures = GpuTextureStore::new(
            device.clone(),
            physical_device,
            instance.clone(),
            command_pool,
            &mut descriptors,
            queue,
        )?;

        Ok(Self {
            meshes,
            textures,
            shaders,
            techniques,
            pipeline_cache,
            device,
            physical_device,
            instance,
            command_pool,
            descriptors,
            samplers,
            materials: MaterialStore::new(),
        })
    }

    pub fn bindless_set(&self) -> DescriptorSetId {
        self.textures.bindless().set_id
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
        let vk_sampler = self.samplers.handle(sampler);
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
        usage: BufferUsage,
        capacity: usize,
    ) -> anyhow::Result<MappedGpuBuffer<T>> {
        MappedGpuBuffer::new(&self.device, self.physical_device, &self.instance, usage.to_vk(), capacity)
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
