use glam::{Vec2, Vec3, Vec4};

/// Stable, human-readable identifier for a material property.
/// Backed by a string literal - no registry needed, no per-frame allocation
/// (PropertyId is just a (ptr, len) pair, Copy).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PropertyId(pub &'static str);

impl PropertyId {
    pub const fn new(name: &'static str) -> Self {
        Self(name)
    }
}

impl std::fmt::Display for PropertyId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureRef(pub u32);

/// A single material property value. Strategies read these by PropertyId
/// via `Material::get`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MaterialValue {
    Bool(bool),
    Int(i32),
    Float(f32),
    Vec2(Vec2),
    Vec3(Vec3),
    Vec4(Vec4),
    /// Linear-space RGBA color. Kept distinct from Vec4 so tooling/UI can
    /// treat it differently (e.g. color picker) even though the payload
    /// is the same shape.
    Color(Vec4),
    Texture(TextureRef),
    UVec4([u32; 4]),
}

impl MaterialValue {
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f32> {
        match self {
            Self::Float(v) => Some(*v),
            Self::Int(v) => Some(*v as f32),
            _ => None,
        }
    }

    pub fn as_vec4(&self) -> Option<Vec4> {
        match self {
            Self::Vec4(v) | Self::Color(v) => Some(*v),
            Self::Vec3(v) => Some(v.extend(1.0)),
            Self::Vec2(v) => Some(Vec4::new(v.x, v.y, 0.0, 1.0)),
            _ => None,
        }
    }

    pub fn as_vec3(&self) -> Option<Vec3> {
        match self {
            Self::Vec4(v) | Self::Color(v) => Some(v.truncate()),
            Self::Vec3(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_texture(&self) -> Option<TextureRef> {
        match self {
            Self::Texture(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_uvec4(&self) -> Option<[u32; 4]> {
        match self {
            Self::UVec4(v) => Some(*v),
            _ => None,
        }
    }

    /// Human-readable type name, for error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Bool(_) => "Bool",
            Self::Int(_) => "Int",
            Self::Float(_) => "Float",
            Self::Vec2(_) => "Vec2",
            Self::Vec3(_) => "Vec3",
            Self::Vec4(_) => "Vec4",
            Self::Color(_) => "Color",
            Self::Texture(_) => "Texture",
            Self::UVec4(_) => "UVec4",
        }
    }
}
