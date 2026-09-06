use crate::field::{FieldDesc, write_field_bytes};
use crate::layout::MaterialLayout;
use crate::material::MaterialHandle;
use crate::strategy::ShadingStrategy;
use crate::value::MaterialValue;
use std::collections::HashMap;

/// Computes std430-compatible byte offsets for `fields`, in order, and the
/// total size of the packed struct, rounded up to a 16-byte multiple as
/// std430 requires for a struct used as an SSBO array element.
pub fn compute_layout(fields: &[FieldDesc]) -> (Vec<usize>, usize) {
    let mut offsets = Vec::with_capacity(fields.len());
    let mut cursor = 0usize;

    for field in fields {
        let align = field.ty.std430_align();
        cursor = cursor.next_multiple_of(align);
        offsets.push(cursor);
        cursor += field.ty.size();
    }

    let total = cursor.next_multiple_of(16);
    (offsets, total)
}

/// Packs `values` (in the same order as `fields`) into a single contiguous
/// byte buffer using std430 layout rules (array-of-structs).
///
/// Returns `None` if `values.len() != fields.len()` or if any value's type
/// doesn't match its field's declared type.
pub fn pack(fields: &[FieldDesc], values: &[MaterialValue]) -> Option<Vec<u8>> {
    if fields.len() != values.len() {
        return None;
    }

    let (offsets, total_size) = compute_layout(fields);
    let mut bytes = vec![0u8; total_size];

    for ((field, value), &offset) in fields.iter().zip(values.iter()).zip(offsets.iter()) {
        let end = offset + field.ty.size();
        write_field_bytes(field.ty, value, &mut bytes[offset..end])?;
    }

    Some(bytes)
}

/// Per-stride bucket of packed material bytes: every material sharing a
/// strategy with the same packed size lands in the same bucket, indexed by
/// its position in `order`.
#[derive(Default)]
struct Bucket {
    stride: usize,
    order: Vec<MaterialHandle>,
    index_of: HashMap<MaterialHandle, usize>,
    bytes: Vec<u8>,
}

impl Bucket {
    fn new(stride: usize) -> Self {
        Self { stride, order: Vec::new(), index_of: HashMap::new(), bytes: Vec::new() }
    }

    // TODO: if a material switches strategy to one with a different stride,
    // its old slot in this bucket is never reclaimed - the bytes stay
    // allocated and orphaned.
    fn write(&mut self, handle: MaterialHandle, data: &[u8]) {
        debug_assert_eq!(data.len(), self.stride);

        let index = *self.index_of.entry(handle).or_insert_with(|| {
            let i = self.order.len();
            self.order.push(handle);
            self.bytes.resize(self.bytes.len() + self.stride, 0);
            i
        });

        let start = index * self.stride;
        self.bytes[start..start + self.stride].copy_from_slice(data);
    }
}

/// `MaterialLayout` implementation that packs each material into a single
/// contiguous byte blob (array-of-structs) via `pack()`, grouping materials
/// by their packed size into separate buckets - materials using different
/// strategies with different field sets don't have to share a stride.
///
/// Holds plain bytes in memory; turning those bytes into an actual GPU
/// buffer (e.g. a Vulkan SSBO) is the caller's responsibility.
#[derive(Default)]
pub struct AosLayout {
    buckets: HashMap<usize, Bucket>,
}

impl AosLayout {
    pub fn new() -> Self {
        Self::default()
    }

    /// Raw bytes currently stored for the bucket at `stride`, if any
    /// material has been uploaded at that size yet.
    pub fn bucket_bytes(&self, stride: usize) -> Option<&[u8]> {
        self.buckets.get(&stride).map(|b| b.bytes.as_slice())
    }

    /// Index of `handle` within its stride's bucket - this is the
    /// `material_id` a shader would use to index into that bucket's buffer.
    pub fn index_of(&self, handle: MaterialHandle, stride: usize) -> Option<usize> {
        self.buckets.get(&stride)?.index_of.get(&handle).copied()
    }

    /// All strides currently in use, for callers that need to allocate one
    /// GPU buffer per stride.
    pub fn strides(&self) -> impl Iterator<Item = usize> + '_ {
        self.buckets.keys().copied()
    }
}

impl MaterialLayout for AosLayout {
    fn register_strategy(&mut self, strategy: &dyn ShadingStrategy) {
        let (_, stride) = compute_layout(strategy.fields());
        self.buckets.entry(stride).or_insert_with(|| Bucket::new(stride));
    }

    fn upload(&mut self, handle: MaterialHandle, strategy: &dyn ShadingStrategy, values: &[MaterialValue]) {
        let Some(bytes) = pack(strategy.fields(), values) else {
            log::error!(
                "AosLayout: failed to pack material {:?} for strategy '{}' - value count or types don't match fields",
                handle,
                strategy.name()
            );
            return;
        };

        let stride = bytes.len();
        let bucket = self.buckets.entry(stride).or_insert_with(|| Bucket::new(stride));
        bucket.write(handle, &bytes);
    }
}
