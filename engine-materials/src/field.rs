use crate::value::MaterialValue;

/// The scalar/vector type of a single field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    Float,
    UInt,
    Vec2,
    Vec3,
    Vec4,
}

impl FieldType {
    /// Size in bytes of one value of this type.
    pub const fn size(self) -> usize {
        match self {
            Self::Float | Self::UInt => 4,
            Self::Vec2 => 8,
            Self::Vec3 => 12,
            Self::Vec4 => 16,
        }
    }

    /// std430 alignment rules (GLSL storage buffer layout): vec3 aligns as
    /// vec4, everything else aligns to its own size.
    pub const fn std430_align(self) -> usize {
        match self {
            Self::Float | Self::UInt => 4,
            Self::Vec2 => 8,
            Self::Vec3 | Self::Vec4 => 16,
        }
    }
}

/// Describes one named field a strategy exposes: its name and value type.
/// Says nothing about memory layout - offsets, strides, and buffer
/// placement are computed by whichever packing module consumes this
/// (see `pack::aos`).
#[derive(Debug, Clone, Copy)]
pub struct FieldDesc {
    pub name: &'static str,
    pub ty: FieldType,
}

impl FieldDesc {
    pub const fn new(name: &'static str, ty: FieldType) -> Self {
        Self { name, ty }
    }
}

/// Writes `value` into `dst` as raw little-endian bytes matching `ty`.
/// `dst` must be at least `ty.size()` bytes long.
pub fn write_field_bytes(ty: FieldType, value: &MaterialValue, dst: &mut [u8]) -> Option<()> {
    match ty {
        FieldType::Float => {
            let v = value.as_float()?;
            dst[..4].copy_from_slice(&v.to_le_bytes());
        }
        FieldType::UInt => {
            let v = match value {
                MaterialValue::Texture(t) => t.0,
                MaterialValue::Int(i) => *i as u32,
                _ => return None,
            };
            dst[..4].copy_from_slice(&v.to_le_bytes());
        }
        FieldType::Vec2 => {
            let v = value.as_vec4()?;
            dst[0..4].copy_from_slice(&v.x.to_le_bytes());
            dst[4..8].copy_from_slice(&v.y.to_le_bytes());
        }
        FieldType::Vec3 => {
            let v = value.as_vec3()?;
            dst[0..4].copy_from_slice(&v.x.to_le_bytes());
            dst[4..8].copy_from_slice(&v.y.to_le_bytes());
            dst[8..12].copy_from_slice(&v.z.to_le_bytes());
        }
        FieldType::Vec4 => {
            let v = value.as_vec4()?;
            dst[0..4].copy_from_slice(&v.x.to_le_bytes());
            dst[4..8].copy_from_slice(&v.y.to_le_bytes());
            dst[8..12].copy_from_slice(&v.z.to_le_bytes());
            dst[12..16].copy_from_slice(&v.w.to_le_bytes());
        }
    }
    Some(())
}
