use crate::assets::cpu_server::TextureHandle;
use crate::assets::material::MaterialData;
use crate::assets::mesh::{CpuMesh, GpuMesh};
use crate::assets::shader_registry::TextureSlot;
use crate::assets::ShaderRegistry;
use crate::components::mesh::{MaterialHandle, MeshHandle};
use crate::render::gfx::{
    sampler, BindingKind, BlendState, DescriptorBindingDesc, DescriptorSetDesc, DescriptorSetId, Format,
    PushConstantRange, SamplerDesc, SamplerId, VertexLayout,
};
use crate::render::gfx::{PipelineCache, PipelineId};
use crate::render::world::RenderWorld;
use crate::vulkan::gfx_pipeline::pipeline::PipelineDesc;
use crate::vulkan::{BindlessSet, GpuTexture, MaterialBuffer};
use ash::vk;
use std::collections::HashMap;

pub const BINDLESS_SLOT_WHITE: u32 = 0;

enum GpuMeshState {
    Ready(GpuMesh),
    Failed,
}

struct StoredSampler {
    handle: vk::Sampler,
}

struct StoredDescriptorSet {
    layout: vk::DescriptorSetLayout,
    set: vk::DescriptorSet,
    pool: vk::DescriptorPool,
    bindings: Vec<DescriptorBindingDesc>,
}

pub struct GpuAssetServer {
    gpu_meshes: HashMap<MeshHandle, GpuMeshState>,
    texture_slots: HashMap<TextureHandle, u32>,
    gpu_textures: HashMap<u32, GpuTexture>,
    materials: Vec<MaterialData>,

    pub shaders: ShaderRegistry,
    pub material_buffer: MaterialBuffer,
    pub bindless: BindlessSet,
    pipeline_cache: PipelineCache,

    device: ash::Device,
    physical_device: vk::PhysicalDevice,
    instance: ash::Instance,
    command_pool: vk::CommandPool,
    queue: vk::Queue,

    samplers: Vec<StoredSampler>,
    descriptor_sets: Vec<StoredDescriptorSet>,
    bindless_set_id: DescriptorSetId,
    material_buffer_set_id: DescriptorSetId,
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

        let material_buffer = MaterialBuffer::new(&device, physical_device, &instance)?;
        let shaders = ShaderRegistry::empty();
        let pipeline_cache = PipelineCache::new(device.clone());

        log::info!("GpuAssetServer: white=slot0, text_renderer готов, next_slot={}", bindless.next_slot());

        let mut this = Self {
            gpu_meshes: HashMap::new(),
            texture_slots: HashMap::new(),
            gpu_textures: HashMap::new(),
            materials: Vec::new(),
            shaders,
            material_buffer,
            bindless,
            pipeline_cache,
            device,
            physical_device,
            instance,
            command_pool,
            queue,
            samplers: Vec::new(),
            descriptor_sets: Vec::new(),
            bindless_set_id: DescriptorSetId(0),
            material_buffer_set_id: DescriptorSetId(0),
        };

        let bindless_layout = this.bindless.layout;
        let bindless_set = this.bindless.set;
        let bindless_pool = this.bindless.pool;
        this.bindless_set_id = this.register_external_descriptor_set(bindless_layout, bindless_set, bindless_pool);

        let material_layout = this.material_buffer.layout;
        let material_set = this.material_buffer.set;
        let material_pool = this.material_buffer.pool;
        this.material_buffer_set_id =
            this.register_external_descriptor_set(material_layout, material_set, material_pool);

        Ok(this)
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

    pub fn create_descriptor_set(&mut self, desc: DescriptorSetDesc) -> anyhow::Result<DescriptorSetId> {
        let vk_bindings: Vec<vk::DescriptorSetLayoutBinding> = desc
            .bindings
            .iter()
            .map(|b| {
                let ty = match b.kind {
                    BindingKind::CombinedImageSampler => vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    BindingKind::UniformBuffer { .. } => vk::DescriptorType::UNIFORM_BUFFER,
                };
                vk::DescriptorSetLayoutBinding::default()
                    .binding(b.binding)
                    .descriptor_type(ty)
                    .descriptor_count(1)
                    .stage_flags(b.stage.to_vk())
            })
            .collect();

        let layout = unsafe {
            self.device.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&vk_bindings),
                None,
            )?
        };

        let pool_sizes: Vec<vk::DescriptorPoolSize> = desc
            .bindings
            .iter()
            .map(|b| {
                let ty = match b.kind {
                    BindingKind::CombinedImageSampler => vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                    BindingKind::UniformBuffer { .. } => vk::DescriptorType::UNIFORM_BUFFER,
                };
                vk::DescriptorPoolSize { ty, descriptor_count: 1 }
            })
            .collect();

        let (pool, set) =
            crate::vulkan::gfx_pipeline::builder::descriptor::alloc_single_set(&self.device, layout, &pool_sizes)?;

        let id = DescriptorSetId(self.descriptor_sets.len() as u32);
        self.descriptor_sets.push(StoredDescriptorSet { layout, set, pool, bindings: desc.bindings });
        Ok(id)
    }

    pub(crate) fn descriptor_set_layout(&self, id: DescriptorSetId) -> vk::DescriptorSetLayout {
        self.descriptor_sets[id.0 as usize].layout
    }

    pub fn bindless_set(&self) -> DescriptorSetId {
        self.bindless_set_id
    }

    pub fn material_buffer_set(&self) -> DescriptorSetId {
        self.material_buffer_set_id
    }

    pub fn descriptor_set_handle(&self, id: DescriptorSetId) -> vk::DescriptorSet {
        self.descriptor_sets[id.0 as usize].set
    }

    pub fn bind_uniform_buffer(&self, set: DescriptorSetId, binding: u32, buffer: vk::Buffer, size: vk::DeviceSize) {
        let stored = &self.descriptor_sets[set.0 as usize];
        debug_assert!(
            stored.bindings.iter().any(|b| b.binding == binding && matches!(b.kind, BindingKind::UniformBuffer { .. })),
            "bind_uniform_buffer: binding {} в этом сете не объявлен как UniformBuffer",
            binding
        );

        let buf_info = vk::DescriptorBufferInfo::default().buffer(buffer).offset(0).range(size);
        let write = vk::WriteDescriptorSet::default()
            .dst_set(stored.set)
            .dst_binding(binding)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .buffer_info(std::slice::from_ref(&buf_info));

        unsafe { self.device.update_descriptor_sets(std::slice::from_ref(&write), &[]) };
    }

    pub fn bind_sampled_image(
        &self,
        set: DescriptorSetId,
        binding: u32,
        view: vk::ImageView,
        layout: vk::ImageLayout,
        sampler: SamplerId,
    ) {
        let stored = &self.descriptor_sets[set.0 as usize];
        debug_assert!(
            stored.bindings.iter().any(|b| b.binding == binding && matches!(b.kind, BindingKind::CombinedImageSampler)),
            "bind_sampled_image: binding {} в этом сете не объявлен как CombinedImageSampler",
            binding
        );

        let image_info = vk::DescriptorImageInfo::default()
            .image_view(view)
            .image_layout(layout)
            .sampler(self.sampler_handle(sampler));
        let write = vk::WriteDescriptorSet::default()
            .dst_set(stored.set)
            .dst_binding(binding)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(std::slice::from_ref(&image_info));

        unsafe { self.device.update_descriptor_sets(std::slice::from_ref(&write), &[]) };
    }

    pub fn create_graphics_pipeline(
        &mut self,
        desc: &PipelineDesc,
        set_layouts: &[vk::DescriptorSetLayout],
    ) -> anyhow::Result<PipelineId> {
        self.pipeline_cache.create_graphics_pipeline(&self.device, desc, set_layouts)
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
        let layouts: Vec<vk::DescriptorSetLayout> =
            set_layouts.iter().map(|&id| self.descriptor_set_layout(id)).collect();

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
        vertex_layout: &VertexLayout,
        push_constant_ranges: &[PushConstantRange],
        depth_bias: Option<(f32, f32)>,
    ) -> anyhow::Result<PipelineId> {
        self.pipeline_cache.create_depth_only_pipeline(
            &self.device,
            vert_spv,
            vertex_layout,
            push_constant_ranges,
            depth_bias,
        )
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
                self.gpu_meshes.insert(handle, GpuMeshState::Ready(gpu));
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
        format: vk::Format,
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
            Format::from_vk(format),
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

    pub(crate) fn register_external_descriptor_set(
        &mut self,
        layout: vk::DescriptorSetLayout,
        set: vk::DescriptorSet,
        pool: vk::DescriptorPool,
    ) -> DescriptorSetId {
        let id = DescriptorSetId(self.descriptor_sets.len() as u32);
        self.descriptor_sets.push(StoredDescriptorSet { layout, set, pool, bindings: Vec::new() });
        id
    }

    pub fn register_material_gpu(
        &mut self,
        handle: MaterialHandle,
        base_color: [f32; 4],
        metallic: f32,
        roughness: f32,
        emissive: [f32; 4],
        texture_slots: Vec<(TextureSlot, TextureHandle)>,
        _name: String,
    ) {
        let idx = handle.0 as usize;
        if self.materials.len() <= idx {
            self.materials.resize(idx + 1, MaterialData::default_white());
        }
        let mut tex0 = [0u32; 4];
        let mut tex1 = [0u32; 4];
        for (slot, tex_handle) in texture_slots {
            let bindless_slot = self.texture_slot(tex_handle);
            match slot.index() {
                i @ 0..=3 => tex0[i] = bindless_slot,
                i => tex1[i - 4] = bindless_slot,
            }
        }
        self.materials[idx] = MaterialData {
            base_color,
            emissive,
            metallic,
            roughness,
            _pad: [0.0; 2],
            tex_indices0: tex0,
            tex_indices1: tex1,
        };
    }

    pub fn upload_materials_from_render_world(&self, _rw: &RenderWorld) {
        if !self.materials.is_empty() {
            self.material_buffer.upload(&self.materials);
        }
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
            for ds in &self.descriptor_sets {
                self.device.destroy_descriptor_pool(ds.pool, None);
                self.device.destroy_descriptor_set_layout(ds.layout, None);
            }
        }
    }
}
