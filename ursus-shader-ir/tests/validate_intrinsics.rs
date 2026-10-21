use ursus_shader_ir::IntrinsicFn::{Floor, Fract, Length};
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
    length_on_bool_is_rejected => (Length, Type::Bool),
    floor_on_bool_is_rejected => (Floor, Type::Bool),
    fract_on_bool_is_rejected => (Fract, Type::Bool),

    // signed int
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
    ($($name:ident => ($func:expr, $ty:expr)),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                let func = $func;
                let ty = $ty;

                validate(&intrinsic_of(func, ty))
                    .unwrap_or_else(|err| {
                        panic!("{func:?} on {ty:?} should be accepted, got: {err:?}")
                    });
            }
        )+
    };
}

func_accepted! {
    // Length: float scalar and vectors
    length_on_float_is_accepted => (Length, Type::Float),
    length_on_vec2_is_accepted => (Length, Type::Vec2),
    length_on_vec3_is_accepted => (Length, Type::Vec3),
    length_on_vec4_is_accepted => (Length, Type::Vec4),

    // Floor: float scalar and vectors
    floor_on_float_is_accepted => (Floor, Type::Float),
    floor_on_vec2_is_accepted => (Floor, Type::Vec2),
    floor_on_vec3_is_accepted => (Floor, Type::Vec3),
    floor_on_vec4_is_accepted => (Floor, Type::Vec4),

    // Fract: float scalar and vectors
    fract_on_float_is_accepted => (Fract, Type::Float),
    fract_on_vec2_is_accepted => (Fract, Type::Vec2),
    fract_on_vec3_is_accepted => (Fract, Type::Vec3),
    fract_on_vec4_is_accepted => (Fract, Type::Vec4),
}
