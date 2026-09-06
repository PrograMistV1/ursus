use crate::render::gfx::descriptor::{DescriptorAllocator, DescriptorSetDesc};
use crate::render::gfx::types::{DescriptorSetId, ShaderStage};
use crate::vulkan::resources::growable_buffer::GrowableBuffer;
use ash::vk;
use engine_materials::MaterialHandle;
use std::collections::HashMap;

/// Render-thread GPU storage for packed material bytes, one `GrowableBuffer`
/// per stride (materials whose packed size differs get separate buffers -
/// see `engine_materials::pack::aos::AosLayout`, which this store mirrors
/// on the GPU side).
///
/// For V1, exactly one stride is expected to be in use at a time (`mesh.frag`
/// / `depth_prepass.frag` / `shadow.frag` all declare a single
/// `set = 1, binding = 0` `MaterialBuffer`), so `descriptor_set` binds the
/// first stride's buffer to that slot. Supporting multiple simultaneous
/// strides on the GPU side (multiple bindings, or a technique-driven choice
/// of which to bind) is future work - see the architecture doc's SoA/
/// multi-strategy discussion.
pub struct MaterialStore {
    buffers: HashMap<usize, GrowableBuffer>,
    bucket_bytes: HashMap<usize, Vec<u8>>,
    index_of: HashMap<MaterialHandle, (usize, usize)>, // handle -> (stride, index)

    descriptor_set: DescriptorSetId,
    bound_stride: Option<usize>,

    device: ash::Device,
    physical_device: vk::PhysicalDevice,
    instance: ash::Instance,
}

impl MaterialStore {
    pub fn new(
        device: ash::Device,
        physical_device: vk::PhysicalDevice,
        instance: ash::Instance,
        descriptors: &mut DescriptorAllocator,
    ) -> anyhow::Result<Self> {
        let desc = DescriptorSetDesc::new().with_storage_buffer::<()>(0, ShaderStage::Fragment);
        let descriptor_set = descriptors.create_set(desc)?;

        Ok(Self {
            buffers: HashMap::new(),
            bucket_bytes: HashMap::new(),
            index_of: HashMap::new(),
            descriptor_set,
            bound_stride: None,
            device,
            physical_device,
            instance,
        })
    }

    pub fn descriptor_set(&self) -> DescriptorSetId {
        self.descriptor_set
    }

    /// Writes `bytes` (this material's full packed data, `bytes.len()` is
    /// its stride) for `handle`, growing/creating the stride's buffer as
    /// needed, re-uploading that stride's whole bucket to the GPU, and - if
    /// this is the first stride seen, or the bound stride's buffer just
    /// reallocated - (re)binding `descriptor_set` to point at it.
    pub fn upload(
        &mut self,
        handle: MaterialHandle,
        bytes: &[u8],
        descriptors: &DescriptorAllocator,
    ) -> anyhow::Result<()> {
        let stride = bytes.len();
        let bucket = self.bucket_bytes.entry(stride).or_default();

        let index = match self.index_of.get(&handle) {
            Some(&(existing_stride, existing_index)) if existing_stride == stride => existing_index,
            _ => {
                let index = bucket.len() / stride.max(1);
                bucket.resize(bucket.len() + stride, 0);
                self.index_of.insert(handle, (stride, index));
                index
            }
        };

        let start = index * stride;
        bucket[start..start + stride].copy_from_slice(bytes);

        let buffer = match self.buffers.get_mut(&stride) {
            Some(b) => b,
            None => {
                let new_buffer = GrowableBuffer::new(
                    &self.device,
                    self.physical_device,
                    &self.instance,
                    vk::BufferUsageFlags::STORAGE_BUFFER,
                    stride.max(64),
                )?;
                self.buffers.entry(stride).or_insert(new_buffer)
            }
        };

        let reallocated = buffer.upload(bucket)?;

        if self.bound_stride != Some(stride) || reallocated {
            self.bind_descriptor(stride, descriptors)?;
        }

        Ok(())
    }

    fn bind_descriptor(&mut self, stride: usize, descriptors: &DescriptorAllocator) -> anyhow::Result<()> {
        if self.bound_stride.is_some() && self.bound_stride != Some(stride) {
            log::warn!(
                "MaterialStore: binding stride {stride} over previously bound stride {:?} - \
                 only one stride can be visible to shaders via the current single MaterialBuffer binding",
                self.bound_stride
            );
        }

        let buffer = self.buffers.get(&stride).expect("stride buffer must exist before binding");
        descriptors.bind_storage_buffer(
            self.descriptor_set,
            0,
            buffer.buffer(),
            buffer.capacity() as vk::DeviceSize,
        )?;
        self.bound_stride = Some(stride);
        Ok(())
    }

    pub fn index_of(&self, handle: MaterialHandle) -> Option<(usize, usize)> {
        self.index_of.get(&handle).copied()
    }

    pub fn buffer_for_stride(&self, stride: usize) -> Option<vk::Buffer> {
        self.buffers.get(&stride).map(|b| b.buffer())
    }

    pub fn strides(&self) -> impl Iterator<Item = usize> + '_ {
        self.buffers.keys().copied()
    }
}
