use ursus_shader_ir::IntrinsicFn::{Floor, Fract, Length, Sqrt};
use ursus_shader_ir::validate::{ValidationError, validate};
use ursus_shader_ir::{Expr, FragmentShader, IntrinsicFn, PushConstantDesc, PushConstantFieldDesc, Type};

fn intrinsic_of(func: IntrinsicFn, ty: Type) -> FragmentShader {
    let push_constant = PushConstantDesc { fields: vec![PushConstantFieldDesc { name: "x", offset: 0, ty }] };

    let arg = Expr::PushConstantField { offset: 0, ty };

    FragmentShader {
        bindings: vec![],
        push_constant: Some(push_constant),
        output: Expr::Intrinsic { func, args: vec![arg] },
    }
}

macro_rules! func_rejected {
    ($($name:ident => ($func:expr, $ty:expr)),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                let func = $func;
                let ty = $ty;

                let err = validate(&intrinsic_of(func, ty))
                    .expect_err(&format!("{func:?} on {ty:?} should fail type check"));

                assert_eq!(
                    err,
                    ValidationError::IntrinsicTypeMismatch {
                        func,
                        arg_index: 0,
                        ty,
                    }
                );
            }
        )+
    };
}

func_rejected! {
    // Bool
    sqrt_on_bool_is_rejected => (Sqrt, Type::Bool),
    length_on_bool_is_rejected => (Length, Type::Bool),
    floor_on_bool_is_rejected => (Floor, Type::Bool),
    fract_on_bool_is_rejected => (Fract, Type::Bool),

    // signed int
    sqrt_on_int_is_rejected => (Sqrt, Type::Int),
    sqrt_on_ivec2_is_rejected => (Sqrt, Type::IVec2),
    sqrt_on_ivec3_is_rejected => (Sqrt, Type::IVec3),
    sqrt_on_ivec4_is_rejected => (Sqrt, Type::IVec4),

    length_on_int_is_rejected => (Length, Type::Int),
    length_on_ivec2_is_rejected => (Length, Type::IVec2),
    length_on_ivec3_is_rejected => (Length, Type::IVec3),
    length_on_ivec4_is_rejected => (Length, Type::IVec4),

    floor_on_int_is_rejected => (Floor, Type::Int),
    floor_on_ivec2_is_rejected => (Floor, Type::IVec2),
    floor_on_ivec3_is_rejected => (Floor, Type::IVec3),
    floor_on_ivec4_is_rejected => (Floor, Type::IVec4),

    fract_on_int_is_rejected => (Fract, Type::Int),
    fract_on_ivec2_is_rejected => (Fract, Type::IVec2),
    fract_on_ivec3_is_rejected => (Fract, Type::IVec3),
    fract_on_ivec4_is_rejected => (Fract, Type::IVec4),

    // unsigned int
    sqrt_on_uint_is_rejected => (Sqrt, Type::UInt),
    sqrt_on_uvec2_is_rejected => (Sqrt, Type::UVec2),
    sqrt_on_uvec3_is_rejected => (Sqrt, Type::UVec3),
    sqrt_on_uvec4_is_rejected => (Sqrt, Type::UVec4),

    length_on_uint_is_rejected => (Length, Type::UInt),
    length_on_uvec2_is_rejected => (Length, Type::UVec2),
    length_on_uvec3_is_rejected => (Length, Type::UVec3),
    length_on_uvec4_is_rejected => (Length, Type::UVec4),

    floor_on_uint_is_rejected => (Floor, Type::UInt),
    floor_on_uvec2_is_rejected => (Floor, Type::UVec2),
    floor_on_uvec3_is_rejected => (Floor, Type::UVec3),
    floor_on_uvec4_is_rejected => (Floor, Type::UVec4),

    fract_on_uint_is_rejected => (Fract, Type::UInt),
    fract_on_uvec2_is_rejected => (Fract, Type::UVec2),
    fract_on_uvec3_is_rejected => (Fract, Type::UVec3),
    fract_on_uvec4_is_rejected => (Fract, Type::UVec4),

    // opaque
    sqrt_on_sampler2d_is_rejected => (Sqrt, Type::Sampler2D),
    sqrt_on_sampler_cube_is_rejected => (Sqrt, Type::SamplerCube),
    sqrt_on_sampler2d_shadow_is_rejected => (Sqrt, Type::Sampler2DShadow),

    length_on_sampler2d_is_rejected => (Length, Type::Sampler2D),
    length_on_sampler_cube_is_rejected => (Length, Type::SamplerCube),
    length_on_sampler2d_shadow_is_rejected => (Length, Type::Sampler2DShadow),

    floor_on_sampler2d_is_rejected => (Floor, Type::Sampler2D),
    floor_on_sampler_cube_is_rejected => (Floor, Type::SamplerCube),
    floor_on_sampler2d_shadow_is_rejected => (Floor, Type::Sampler2DShadow),

    fract_on_sampler2d_is_rejected => (Fract, Type::Sampler2D),
    fract_on_sampler_cube_is_rejected => (Fract, Type::SamplerCube),
    fract_on_sampler2d_shadow_is_rejected => (Fract, Type::Sampler2DShadow),
}

macro_rules! func_accepted {
    ($($name:ident => ($func:expr, $ty:expr, $resTy:expr)),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                let func = $func;
                let ty = $ty;
                let res_ty = $resTy;

                let result = validate(&intrinsic_of(func, ty))
                    .expect(&format!("{func:?} on {ty:?} should pass type check"));
                assert_eq!(result.ty, res_ty, "{func:?} on {ty:?} should return {res_ty:?}, got: {:?}", result.ty);
            }
        )+
    };
}

func_accepted! {
    // Sqrt: float scalar and vectors
    sqrt_on_float_is_accepted => (Sqrt, Type::Float, Type::Float),
    sqrt_on_vec2_is_accepted => (Sqrt, Type::Vec2, Type::Vec2),
    sqrt_on_vec3_is_accepted => (Sqrt, Type::Vec3, Type::Vec3),
    sqrt_on_vec4_is_accepted => (Sqrt, Type::Vec4, Type::Vec4),

    // Length: float scalar and vectors
    length_on_float_is_accepted => (Length, Type::Float, Type::Float),
    length_on_vec2_is_accepted => (Length, Type::Vec2, Type::Float),
    length_on_vec3_is_accepted => (Length, Type::Vec3, Type::Float),
    length_on_vec4_is_accepted => (Length, Type::Vec4, Type::Float),

    // Floor: float scalar and vectors
    floor_on_float_is_accepted => (Floor, Type::Float, Type::Float),
    floor_on_vec2_is_accepted => (Floor, Type::Vec2, Type::Vec2),
    floor_on_vec3_is_accepted => (Floor, Type::Vec3, Type::Vec3),
    floor_on_vec4_is_accepted => (Floor, Type::Vec4, Type::Vec4),

    // Fract: float scalar and vectors
    fract_on_float_is_accepted => (Fract, Type::Float, Type::Float),
    fract_on_vec2_is_accepted => (Fract, Type::Vec2, Type::Vec2),
    fract_on_vec3_is_accepted => (Fract, Type::Vec3, Type::Vec3),
    fract_on_vec4_is_accepted => (Fract, Type::Vec4, Type::Vec4),
}
