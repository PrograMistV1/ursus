use crate::field::{FieldDesc, write_field_bytes};
use crate::value::MaterialValue;

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
