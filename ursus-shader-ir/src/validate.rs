use crate::expr::{BinOp, BuiltinVar, Expr, IntrinsicFn};
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
    InvalidSample2D { texture_ty: Type, uv_ty: Type },

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

/// An `Expr` tree where every node has been annotated with its resolved
/// `Type`. Produced by `validate`, consumed by `lower` - lowering never
/// re-infers types, it just reads `ty` off each node.
#[derive(Debug, Clone)]
pub struct TypedExpr {
    pub ty: Type,
    pub kind: TypedExprKind,
}

#[derive(Debug, Clone)]
pub enum TypedExprKind {
    Constant(f32),
    Builtin(BuiltinVar),
    BindingRef(&'static str),
    PushConstantField {
        offset: u32,
    },
    Swizzle {
        base: Box<TypedExpr>,
        pattern: &'static str,
    },
    Binary {
        op: BinOp,
        lhs: Box<TypedExpr>,
        rhs: Box<TypedExpr>,
    },
    Sample2D {
        texture: Box<TypedExpr>,
        uv: Box<TypedExpr>,
    },
    VecConstruct {
        components: Vec<TypedExpr>,
    },
    Intrinsic {
        func: IntrinsicFn,
        args: Vec<TypedExpr>,
    },
}

/// Type-checks `shader.output` and, on success, returns the fully
/// type-annotated tree ready for `lower::lower`.
pub fn validate(shader: &FragmentShader) -> Result<TypedExpr, ValidationError> {
    infer(shader, &shader.output)
}

fn infer(shader: &FragmentShader, expr: &Expr) -> Result<TypedExpr, ValidationError> {
    match expr {
        Expr::Constant(v) => Ok(TypedExpr { ty: Type::Float, kind: TypedExprKind::Constant(*v) }),

        Expr::Builtin(b) => Ok(TypedExpr { ty: b.ty(), kind: TypedExprKind::Builtin(*b) }),

        Expr::BindingRef(name) => {
            let binding = shader
                .bindings
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, b)| b)
                .ok_or(ValidationError::UnknownBinding { name })?;
            Ok(TypedExpr { ty: binding_type(binding.kind), kind: TypedExprKind::BindingRef(name) })
        }

        Expr::PushConstantField { offset, ty } => {
            let pc = shader.push_constant.as_ref().ok_or(ValidationError::NoPushConstantBlock)?;
            let found = pc.fields.iter().any(|f| f.offset == *offset && f.ty == *ty);
            if !found {
                return Err(ValidationError::UnknownPushConstantField { offset: *offset });
            }
            Ok(TypedExpr { ty: *ty, kind: TypedExprKind::PushConstantField { offset: *offset } })
        }

        Expr::Swizzle { base, pattern } => {
            let base_typed = infer(shader, base)?;
            let result_ty = swizzle_result_type(base_typed.ty, pattern)
                .ok_or(ValidationError::InvalidSwizzle { base_ty: base_typed.ty, pattern })?;
            Ok(TypedExpr { ty: result_ty, kind: TypedExprKind::Swizzle { base: Box::new(base_typed), pattern } })
        }

        Expr::Binary { op, lhs, rhs } => {
            let lhs_typed = infer(shader, lhs)?;
            let rhs_typed = infer(shader, rhs)?;
            let result_ty = binary_result_type(lhs_typed.ty, rhs_typed.ty)
                .ok_or(ValidationError::BinaryTypeMismatch { op: *op, lhs_ty: lhs_typed.ty, rhs_ty: rhs_typed.ty })?;
            Ok(TypedExpr {
                ty: result_ty,
                kind: TypedExprKind::Binary { op: *op, lhs: Box::new(lhs_typed), rhs: Box::new(rhs_typed) },
            })
        }

        Expr::Sample2D { texture, uv } => {
            let texture_typed = infer(shader, texture)?;
            let uv_typed = infer(shader, uv)?;
            if texture_typed.ty != Type::Sampler2D || uv_typed.ty != Type::Vec2 {
                return Err(ValidationError::InvalidSample2D { texture_ty: texture_typed.ty, uv_ty: uv_typed.ty });
            }
            Ok(TypedExpr {
                ty: Type::Vec4,
                kind: TypedExprKind::Sample2D { texture: Box::new(texture_typed), uv: Box::new(uv_typed) },
            })
        }

        Expr::VecConstruct { ty, components } => {
            let components_typed: Vec<TypedExpr> =
                components.iter().map(|c| infer(shader, c)).collect::<Result<_, _>>()?;

            let mut total = 0u32;
            for c in &components_typed {
                if c.ty.is_opaque() {
                    return Err(ValidationError::VecConstructInvalidComponent { ty: *ty, component_ty: c.ty });
                }
                total += c.ty.component_count();
            }

            let target = ty.component_count();
            let is_splat = components_typed.len() == 1 && total == 1 && target > 1;
            if total != target && !is_splat {
                return Err(ValidationError::VecConstructArity { ty: *ty, provided_components: total });
            }

            Ok(TypedExpr { ty: *ty, kind: TypedExprKind::VecConstruct { components: components_typed } })
        }

        Expr::Intrinsic { func, args } => {
            let args_typed: Vec<TypedExpr> = args.iter().map(|a| infer(shader, a)).collect::<Result<_, _>>()?;
            let arg_types: Vec<Type> = args_typed.iter().map(|a| a.ty).collect();
            let result_ty = intrinsic_result_type(*func, &arg_types)?;
            Ok(TypedExpr { ty: result_ty, kind: TypedExprKind::Intrinsic { func: *func, args: args_typed } })
        }
    }
}

fn binding_type(kind: crate::shader::BindingKind) -> Type {
    match kind {
        crate::shader::BindingKind::CombinedImageSampler2D => Type::Sampler2D,
    }
}

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
        IntrinsicFn::Normalize | IntrinsicFn::Sqrt | IntrinsicFn::Length | IntrinsicFn::Floor | IntrinsicFn::Fract => {
            let [x] = arg_types else { return Err(arity_err(1)) };
            if x.is_opaque() || x.scalar() != Type::Float {
                return Err(type_err(0, *x));
            }
            if func == IntrinsicFn::Length {
                return Ok(Type::Float);
            }
            Ok(*x)
        }
        IntrinsicFn::Min | IntrinsicFn::Max => {
            let [x, y] = arg_types else { return Err(arity_err(2)) };
            if x != y {
                return Err(type_err(1, *y));
            }
            if x.is_opaque() {
                return Err(type_err(0, *x));
            }
            if !matches!(x.scalar(), Type::Float | Type::Int | Type::UInt) {
                return Err(type_err(0, *x));
            }
            if !matches!(y.scalar(), Type::Float | Type::Int | Type::UInt) {
                return Err(type_err(1, *y));
            }
            Ok(*x)
        }
        IntrinsicFn::Clamp => {
            let [x, lo, hi] = arg_types else {
                return Err(arity_err(3));
            };
            if x.is_opaque() || (*lo != x.scalar() && *lo != *x) || (*hi != x.scalar() && *hi != *x) {
                return Err(type_err(0, *x));
            }
            Ok(*x)
        }
        IntrinsicFn::Pow => {
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
        IntrinsicFn::Abs => {
            let [x] = arg_types else { return Err(arity_err(1)) };
            if !x.is_vector() && *x != Type::Float {
                return Err(type_err(0, *x));
            }
            Ok(Type::Float)
        }
    }
}
