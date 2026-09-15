use crate::expr::{BinOp, Expr, IntrinsicFn};
use crate::shader::FragmentShader;
use crate::types::Type;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// A `BindingRef` referenced a name not present in `FragmentShader::bindings`.
    UnknownBinding { name: &'static str },

    /// A swizzle pattern referenced a component that doesn't exist on the
    /// base type (e.g. `.z` on a `Vec2`), or was empty, or exceeded 4
    /// components, or mixed component sets (e.g. `.xr`).
    InvalidSwizzle { base_ty: Type, pattern: &'static str },

    /// `Binary` operands' types are not compatible: both sides must be the
    /// same type, or one side a vector and the other its scalar component
    /// type (component-wise scalar broadcast, as in GLSL).
    BinaryTypeMismatch { op: BinOp, lhs_ty: Type, rhs_ty: Type },

    /// `Sample2D`'s `texture` operand did not resolve to a sampler-kind
    /// binding, or `uv` was not a `Vec2`.
    InvalidSample2D { texture_ty: Option<Type>, uv_ty: Type },

    /// `VecConstruct`'s components did not sum to `ty`'s component count,
    /// under GLSL construction rules (N scalars, a single scalar splat, or
    /// a mix of vectors/scalars summing exactly).
    VecConstructArity { ty: Type, provided_components: u32 },

    /// `VecConstruct` was given a component whose type isn't a scalar or
    /// vector of scalars (e.g. a sampler).
    VecConstructInvalidComponent { ty: Type, component_ty: Type },

    /// `Intrinsic` was called with the wrong number of arguments for `func`.
    IntrinsicArity {
        func: IntrinsicFn,
        expected: usize,
        got: usize,
    },

    /// `Intrinsic` argument types weren't compatible with `func`'s
    /// signature (e.g. `dot` on scalars, `clamp` with mismatched vector
    /// widths).
    IntrinsicTypeMismatch {
        func: IntrinsicFn,
        arg_index: usize,
        ty: Type,
    },

    /// The shader declared a push constant field reference (`Expr::PushConstantField`)
    /// whose offset doesn't match any field in `FragmentShader::push_constant`.
    UnknownPushConstantField { offset: u32 },

    /// `Expr::PushConstantField` was used but the shader has no push
    /// constant block declared at all.
    NoPushConstantBlock,
}

/// Type-checks `shader.output` (and, transitively, every sub-expression),
/// returning the output's type on success.
///
/// This does not perform SPIR-V lowering - it only verifies the tree is
/// internally consistent (every node's inputs have types compatible with
/// what that node expects) before codegen is attempted.
pub fn validate(shader: &FragmentShader) -> Result<Type, ValidationError> {
    infer(shader, &shader.output)
}

fn infer(shader: &FragmentShader, expr: &Expr) -> Result<Type, ValidationError> {
    match expr {
        Expr::Constant(_) => Ok(Type::Float),

        Expr::Builtin(b) => Ok(b.ty()),

        Expr::BindingRef(name) => {
            let binding = shader
                .bindings
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, b)| b)
                .ok_or(ValidationError::UnknownBinding { name })?;
            Ok(binding_type(binding.kind))
        }

        Expr::PushConstantField { offset, ty } => {
            let pc = shader.push_constant.as_ref().ok_or(ValidationError::NoPushConstantBlock)?;
            let found = pc.fields.iter().any(|f| f.offset == *offset && f.ty == *ty);
            if found {
                Ok(*ty)
            } else {
                Err(ValidationError::UnknownPushConstantField { offset: *offset })
            }
        }

        Expr::Swizzle { base, pattern } => {
            let base_ty = infer(shader, base)?;
            swizzle_result_type(base_ty, pattern).ok_or(ValidationError::InvalidSwizzle { base_ty, pattern })
        }

        Expr::Binary { op, lhs, rhs } => {
            let lhs_ty = infer(shader, lhs)?;
            let rhs_ty = infer(shader, rhs)?;
            binary_result_type(lhs_ty, rhs_ty).ok_or(ValidationError::BinaryTypeMismatch { op: *op, lhs_ty, rhs_ty })
        }

        Expr::Sample2D { texture, uv } => {
            let texture_ty = infer(shader, texture)?;
            let uv_ty = infer(shader, uv)?;
            if texture_ty != Type::Sampler2D || uv_ty != Type::Vec2 {
                return Err(ValidationError::InvalidSample2D { texture_ty: Some(texture_ty), uv_ty });
            }
            Ok(Type::Vec4)
        }

        Expr::VecConstruct { ty, components } => {
            let mut total = 0u32;
            for c in components {
                let c_ty = infer(shader, c)?;
                if c_ty.is_opaque() {
                    return Err(ValidationError::VecConstructInvalidComponent { ty: *ty, component_ty: c_ty });
                }
                total += c_ty.component_count();
            }

            let target = ty.component_count();
            let is_splat = components.len() == 1 && total == 1 && target > 1;
            if total != target && !is_splat {
                return Err(ValidationError::VecConstructArity { ty: *ty, provided_components: total });
            }
            Ok(*ty)
        }

        Expr::Intrinsic { func, args } => {
            let arg_types: Vec<Type> = args.iter().map(|a| infer(shader, a)).collect::<Result<_, _>>()?;
            intrinsic_result_type(*func, &arg_types)
        }
    }
}

fn binding_type(kind: crate::shader::BindingKind) -> Type {
    match kind {
        crate::shader::BindingKind::CombinedImageSampler2D => Type::Sampler2D,
    }
}

/// Resolves a swizzle pattern (e.g. "xy", "rgb", "xxxx") against `base_ty`.
/// Accepts both positional (`xyzw`) and color (`rgba`) component names, but
/// not a mix of the two within one pattern (matches GLSL). Returns `None`
/// for empty patterns, patterns longer than 4 characters, out-of-range
/// component indices for `base_ty`, or mixed component-name sets.
fn swizzle_result_type(base_ty: Type, pattern: &str) -> Option<Type> {
    if !base_ty.is_vector() || pattern.is_empty() || pattern.len() > 4 {
        return None;
    }

    let max_index = base_ty.component_count();
    let positional = "xyzw";
    let color = "rgba";

    let uses_positional = pattern.chars().all(|c| positional.contains(c));
    let uses_color = pattern.chars().all(|c| color.contains(c));
    if !uses_positional && !uses_color {
        return None;
    }

    let set = if uses_positional { positional } else { color };
    for c in pattern.chars() {
        let index = set.find(c).unwrap() as u32;
        if index >= max_index {
            return None;
        }
    }

    Some(match pattern.len() {
        1 => base_ty.scalar(),
        2 => vec_of(base_ty.scalar(), 2),
        3 => vec_of(base_ty.scalar(), 3),
        4 => vec_of(base_ty.scalar(), 4),
        _ => unreachable!("pattern.len() already bounded to 1..=4 above"),
    })
}

fn vec_of(scalar: Type, n: u32) -> Type {
    match (scalar, n) {
        (Type::Float, 2) => Type::Vec2,
        (Type::Float, 3) => Type::Vec3,
        (Type::Float, 4) => Type::Vec4,
        (Type::Int, 2) => Type::IVec2,
        (Type::Int, 3) => Type::IVec3,
        (Type::Int, 4) => Type::IVec4,
        (Type::UInt, 2) => Type::UVec2,
        (Type::UInt, 3) => Type::UVec3,
        (Type::UInt, 4) => Type::UVec4,
        _ => panic!("vec_of: unsupported (scalar={scalar:?}, n={n})"),
    }
}

/// Result type of binary op, or `None` if `lhs_ty`/`rhs_ty` aren't
/// compatible. Matches GLSL's component-wise + scalar-broadcast rules:
/// same type on both sides, or a vector paired with its own scalar type
/// (e.g. `Vec3 * Float`, either order).
fn binary_result_type(lhs_ty: Type, rhs_ty: Type) -> Option<Type> {
    if lhs_ty == rhs_ty {
        return Some(lhs_ty);
    }
    if lhs_ty.is_vector() && rhs_ty == lhs_ty.scalar() {
        return Some(lhs_ty);
    }
    if rhs_ty.is_vector() && lhs_ty == rhs_ty.scalar() {
        return Some(rhs_ty);
    }
    None
}

fn intrinsic_result_type(func: IntrinsicFn, arg_types: &[Type]) -> Result<Type, ValidationError> {
    let arity_err = |expected: usize| ValidationError::IntrinsicArity { func, expected, got: arg_types.len() };
    let type_err = |i: usize, ty: Type| ValidationError::IntrinsicTypeMismatch { func, arg_index: i, ty };

    match func {
        IntrinsicFn::Clamp => {
            let [x, lo, hi] = arg_types else {
                return Err(arity_err(3));
            };
            if x.is_opaque() || *lo != x.scalar() && *lo != *x || *hi != x.scalar() && *hi != *x {
                return Err(type_err(0, *x));
            }
            Ok(*x)
        }
        IntrinsicFn::Pow | IntrinsicFn::Min | IntrinsicFn::Max => {
            let [x, y] = arg_types else { return Err(arity_err(2)) };
            if x != y || x.is_opaque() {
                return Err(type_err(1, *y));
            }
            Ok(*x)
        }
        IntrinsicFn::Mix => {
            let [x, y, a] = arg_types else { return Err(arity_err(3)) };
            if x != y || x.is_opaque() || (*a != *x && *a != x.scalar()) {
                return Err(type_err(2, *a));
            }
            Ok(*x)
        }
        IntrinsicFn::Normalize | IntrinsicFn::Sqrt | IntrinsicFn::Abs | IntrinsicFn::Floor | IntrinsicFn::Fract => {
            let [x] = arg_types else { return Err(arity_err(1)) };
            if x.is_opaque() {
                return Err(type_err(0, *x));
            }
            Ok(*x)
        }
        IntrinsicFn::Dot => {
            let [x, y] = arg_types else { return Err(arity_err(2)) };
            if x != y || !x.is_vector() {
                return Err(type_err(1, *y));
            }
            Ok(Type::Float)
        }
        IntrinsicFn::Cross => {
            let [x, y] = arg_types else { return Err(arity_err(2)) };
            if *x != Type::Vec3 || *y != Type::Vec3 {
                return Err(type_err(1, *y));
            }
            Ok(Type::Vec3)
        }
        IntrinsicFn::Length => {
            let [x] = arg_types else { return Err(arity_err(1)) };
            if !x.is_vector() && *x != Type::Float {
                return Err(type_err(0, *x));
            }
            Ok(Type::Float)
        }
    }
}
