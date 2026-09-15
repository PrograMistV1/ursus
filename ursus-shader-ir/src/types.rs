/// Scalar, vector, and opaque types used by the IR. Matrices and arrays are
/// not yet supported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Type {
    Bool,
    Int,
    UInt,
    Float,

    IVec2,
    IVec3,
    IVec4,

    UVec2,
    UVec3,
    UVec4,

    Vec2,
    Vec3,
    Vec4,

    Sampler2D,
    SamplerCube,
    Sampler2DShadow,
}

impl Type {
    pub fn is_vector(self) -> bool {
        matches!(
            self,
            Type::IVec2
                | Type::IVec3
                | Type::IVec4
                | Type::UVec2
                | Type::UVec3
                | Type::UVec4
                | Type::Vec2
                | Type::Vec3
                | Type::Vec4
        )
    }

    pub fn is_opaque(self) -> bool {
        matches!(self, Type::Sampler2D | Type::SamplerCube | Type::Sampler2DShadow)
    }

    /// Scalar component type of a vector type, or `self` for scalars.
    /// Panics for opaque types (samplers have no component type).
    pub fn scalar(self) -> Type {
        match self {
            Type::Bool => Type::Bool,
            Type::Int | Type::IVec2 | Type::IVec3 | Type::IVec4 => Type::Int,
            Type::UInt | Type::UVec2 | Type::UVec3 | Type::UVec4 => Type::UInt,
            Type::Float | Type::Vec2 | Type::Vec3 | Type::Vec4 => Type::Float,
            Type::Sampler2D | Type::SamplerCube | Type::Sampler2DShadow => {
                panic!("Type::scalar: {self:?} is opaque and has no scalar component type")
            }
        }
    }

    /// Number of components: 1 for scalars, 2/3/4 for vectors. Panics for
    /// opaque types.
    pub fn component_count(self) -> u32 {
        match self {
            Type::Bool | Type::Int | Type::UInt | Type::Float => 1,
            Type::IVec2 | Type::UVec2 | Type::Vec2 => 2,
            Type::IVec3 | Type::UVec3 | Type::Vec3 => 3,
            Type::IVec4 | Type::UVec4 | Type::Vec4 => 4,
            Type::Sampler2D | Type::SamplerCube | Type::Sampler2DShadow => {
                panic!("Type::component_count: {self:?} is opaque and has no component count")
            }
        }
    }

    /// Byte size of a single component (4 for all scalar/vector types
    /// currently supported - no f16/f64 yet).
    pub fn component_byte_size(self) -> u32 {
        4
    }
}
