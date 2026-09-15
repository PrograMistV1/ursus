use crate::types::Type;

/// Built-in shader-stage input/output variables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinVar {
    FragCoord,     // gl_FragCoord, Vec4
    VertexIndex,   // gl_VertexIndex, Int
    InstanceIndex, // gl_InstanceIndex, Int
}

impl BuiltinVar {
    pub fn ty(self) -> Type {
        match self {
            BuiltinVar::FragCoord => Type::Vec4,
            BuiltinVar::VertexIndex => Type::Int,
            BuiltinVar::InstanceIndex => Type::Int,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
}

/// Built-in GLSL functions available as `Expr::Intrinsic` calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntrinsicFn {
    Clamp,     // clamp(x, min, max)
    Pow,       // pow(x, y)
    Min,       // min(x, y)
    Max,       // max(x, y)
    Mix,       // mix(x, y, a)
    Normalize, // normalize(x)
    Dot,       // dot(x, y)
    Cross,     // cross(x, y)
    Length,    // length(x)
    Sqrt,      // sqrt(x)
    Abs,       // abs(x)
    Floor,     // floor(x)
    Fract,     // fract(x)
}

/// One node of a shader expression tree.
///
/// TODO(functions): user-defined GLSL functions are inlined at
/// graph-construction time - the caller substitutes the function body
/// directly at each call site. Real SPIR-V `OpFunction`/`OpFunctionCall`
/// support is not implemented yet.
///
/// TODO(control-flow): no `if`/loop nodes yet - only straight-line
/// expression trees are supported.
#[derive(Debug, Clone)]
pub enum Expr {
    Constant(f32),

    Builtin(BuiltinVar),

    /// Reference to a declared resource binding by name (see
    /// `FragmentShader::bindings`). Resolved to the binding's SPIR-V
    /// variable at lowering time.
    BindingRef(&'static str),

    /// A field of the shader's push constant block, identified by byte
    /// offset.
    PushConstantField {
        offset: u32,
        ty: Type,
    },

    /// Component swizzle, e.g. `.xy`, `.rgb`. Validated against `base`'s
    /// type at lowering time.
    Swizzle {
        base: Box<Expr>,
        pattern: &'static str,
    },

    /// Binary arithmetic, component-wise for vector operands.
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },

    /// `texture(sampler, uv)`.
    Sample2D {
        texture: Box<Expr>,
        uv: Box<Expr>,
    },

    /// Vector constructor: `vec3(a, b, c)`, `vec4(vec3, 1.0)`, or the splat
    /// form `vec3(x)`. Total component count of `components` must equal
    /// `ty.component_count()`, checked at lowering time.
    VecConstruct {
        ty: Type,
        components: Vec<Expr>,
    },

    /// Call to a built-in GLSL function. Argument count/types are
    /// validated per-`IntrinsicFn` at lowering time.
    Intrinsic {
        func: IntrinsicFn,
        args: Vec<Expr>,
    },
}
