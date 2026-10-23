use Type::{IVec2, IVec3, IVec4, Int};
use ursus_shader_ir::IntrinsicFn::{Floor, Fract, Length, Max, Min, Sqrt};
use ursus_shader_ir::Type::{Bool, Float, UInt, UVec2, UVec3, UVec4, Vec2, Vec3, Vec4};
use ursus_shader_ir::validate::{ValidationError, validate};
use ursus_shader_ir::{Expr, FragmentShader, IntrinsicFn, PushConstantDesc, PushConstantFieldDesc, Type};

// TODO: tests for opaque operands (Sampler2D, SamplerCube, Sampler2DShadow)
// are postponed: intrinsic_of cannot build opaque fields via push constants.
// Revisit once sampler support is implemented in lower and BindingRef covers them.
fn intrinsic_of(func: IntrinsicFn, tys: &[Type]) -> FragmentShader {
    let mut fields: Vec<PushConstantFieldDesc> = vec![];
    let mut args: Vec<Expr> = vec![];
    let mut offset = 0;
    for ty in tys {
        if ty.is_opaque() {
            panic!("intrinsic_of: {func:?} on {tys:?} has opaque type {ty:?}, which cannot be used as a push constant");
        }
        fields.push(PushConstantFieldDesc { name: format!("x{}", fields.len()).leak(), offset, ty: *ty });
        args.push(Expr::PushConstantField { offset, ty: *ty });
        offset += ty.component_byte_size() * ty.component_count();
    }

    FragmentShader {
        bindings: vec![],
        push_constant: Some(PushConstantDesc { fields }),
        output: Expr::Intrinsic { func, args },
    }
}

macro_rules! func_rejected {
    ($($name:ident => ($func:expr, [$($ty:expr),+ $(,)?], $arg_i:expr)),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                let func = $func;
                let tys = [$($ty),+];
                let arg_index = $arg_i;

                let err = validate(&intrinsic_of(func, &tys))
                    .expect_err(&format!("{func:?} on {tys:?} should fail type check"));

                assert_eq!(err, ValidationError::IntrinsicTypeMismatch { func, arg_index, ty: tys[arg_index] });
            }
        )+
    };
}

func_rejected! {
    // Bool
    sqrt_on_bool_is_rejected => (Sqrt, [Bool], 0),
    length_on_bool_is_rejected => (Length, [Bool,], 0),
    floor_on_bool_is_rejected => (Floor, [Bool], 0),
    fract_on_bool_is_rejected => (Fract, [Bool], 0),
    min_on_bool_is_rejected => (Min, [Bool, Bool], 0),
    max_on_bool_is_rejected => (Min, [Bool, Bool], 0),

    // signed int
    sqrt_on_int_is_rejected => (Sqrt, [Int], 0),
    sqrt_on_ivec2_is_rejected => (Sqrt, [IVec2], 0),
    sqrt_on_ivec3_is_rejected => (Sqrt, [IVec3], 0),
    sqrt_on_ivec4_is_rejected => (Sqrt, [IVec4], 0),

    length_on_int_is_rejected => (Length, [Int], 0),
    length_on_ivec2_is_rejected => (Length, [IVec2], 0),
    length_on_ivec3_is_rejected => (Length, [IVec3], 0),
    length_on_ivec4_is_rejected => (Length, [IVec4], 0),

    floor_on_int_is_rejected => (Floor, [Int], 0),
    floor_on_ivec2_is_rejected => (Floor, [IVec2], 0),
    floor_on_ivec3_is_rejected => (Floor, [IVec3], 0),
    floor_on_ivec4_is_rejected => (Floor, [IVec4], 0),

    fract_on_int_is_rejected => (Fract, [Int], 0),
    fract_on_ivec2_is_rejected => (Fract, [IVec2], 0),
    fract_on_ivec3_is_rejected => (Fract, [IVec3], 0),
    fract_on_ivec4_is_rejected => (Fract, [IVec4], 0),

    // unsigned int
    sqrt_on_uint_is_rejected => (Sqrt, [UInt], 0),
    sqrt_on_uvec2_is_rejected => (Sqrt, [UVec2], 0),
    sqrt_on_uvec3_is_rejected => (Sqrt, [UVec3], 0),
    sqrt_on_uvec4_is_rejected => (Sqrt, [UVec4], 0),

    length_on_uint_is_rejected => (Length, [UInt], 0),
    length_on_uvec2_is_rejected => (Length, [UVec2], 0),
    length_on_uvec3_is_rejected => (Length, [UVec3], 0),
    length_on_uvec4_is_rejected => (Length, [UVec4], 0),

    floor_on_uint_is_rejected => (Floor, [UInt], 0),
    floor_on_uvec2_is_rejected => (Floor, [UVec2], 0),
    floor_on_uvec3_is_rejected => (Floor, [UVec3], 0),
    floor_on_uvec4_is_rejected => (Floor, [UVec4], 0),

    fract_on_uint_is_rejected => (Fract, [UInt], 0),
    fract_on_uvec2_is_rejected => (Fract, [UVec2], 0),
    fract_on_uvec3_is_rejected => (Fract, [UVec3], 0),
    fract_on_uvec4_is_rejected => (Fract, [UVec4], 0),

    // opaque
    /*sqrt_on_sampler2d_is_rejected => (Sqrt, [Sampler2D], 0),
    sqrt_on_sampler_cube_is_rejected => (Sqrt, [SamplerCube], 0),
    sqrt_on_sampler2d_shadow_is_rejected => (Sqrt, [Sampler2DShadow], 0),

    length_on_sampler2d_is_rejected => (Length, [Sampler2D], 0),
    length_on_sampler_cube_is_rejected => (Length, [SamplerCube], 0),
    length_on_sampler2d_shadow_is_rejected => (Length, [Sampler2DShadow], 0),

    floor_on_sampler2d_is_rejected => (Floor, [Sampler2D], 0),
    floor_on_sampler_cube_is_rejected => (Floor, [SamplerCube], 0),
    floor_on_sampler2d_shadow_is_rejected => (Floor, [Sampler2DShadow], 0),

    fract_on_sampler2d_is_rejected => (Fract, [Sampler2D], 0),
    fract_on_sampler_cube_is_rejected => (Fract, [SamplerCube], 0),
    fract_on_sampler2d_shadow_is_rejected => (Fract, [Sampler2DShadow], 0),

    min_on_sampler2d_is_rejected => (Min, [Sampler2D, Sampler2D], 0),
    min_on_sampler_cube_is_rejected => (Min, [SamplerCube, SamplerCube], 0),
    min_on_sampler2d_shadow_is_rejected => (Min, [Sampler2DShadow, Sampler2DShadow], 0),

    max_on_sampler2d_is_rejected => (Min, [Sampler2D, Sampler2D], 0),
    max_on_sampler_cube_is_rejected => (Min, [SamplerCube, SamplerCube], 0),
    max_on_sampler2d_shadow_is_rejected => (Min, [Sampler2DShadow, Sampler2DShadow], 0),*/

    // same_type
    min_y_int_with_float_x_is_rejected => (Min, [Float, Int], 1),
    max_y_int_with_float_x_is_rejected => (Max, [Float, Int], 1),
}

macro_rules! func_accepted {
    ($($name:ident => ($func:expr, [$($ty:expr),+ $(,)?], $resTy:expr)),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                let func = $func;
                let tys = [$($ty),+];
                let res_ty = $resTy;

                let result = validate(&intrinsic_of(func, &tys))
                    .expect(&format!("{func:?} on {tys:?} should pass type check"));
                assert_eq!(result.ty, res_ty, "{func:?} on {tys:?} should return {res_ty:?}, got: {:?}", result.ty);
            }
        )+
    };
}

func_accepted! {
    // Sqrt: float scalar and vectors
    sqrt_on_float_is_accepted => (Sqrt, [Float], Float),
    sqrt_on_vec2_is_accepted => (Sqrt, [Vec2], Vec2),
    sqrt_on_vec3_is_accepted => (Sqrt, [Vec3], Vec3),
    sqrt_on_vec4_is_accepted => (Sqrt, [Vec4], Vec4),

    // Length: float scalar and vectors
    length_on_float_is_accepted => (Length, [Float], Float),
    length_on_vec2_is_accepted => (Length, [Vec2], Float),
    length_on_vec3_is_accepted => (Length, [Vec3], Float),
    length_on_vec4_is_accepted => (Length, [Vec4], Float),

    // Floor: float scalar and vectors
    floor_on_float_is_accepted => (Floor, [Float], Float),
    floor_on_vec2_is_accepted => (Floor, [Vec2], Vec2),
    floor_on_vec3_is_accepted => (Floor, [Vec3], Vec3),
    floor_on_vec4_is_accepted => (Floor, [Vec4], Vec4),

    // Fract: float scalar and vectors
    fract_on_float_is_accepted => (Fract, [Float], Float),
    fract_on_vec2_is_accepted => (Fract, [Vec2], Vec2),
    fract_on_vec3_is_accepted => (Fract, [Vec3], Vec3),
    fract_on_vec4_is_accepted => (Fract, [Vec4], Vec4),

    // Min: float scalar and vectors
    min_on_float_is_accepted => (Min, [Float, Float], Float),
    min_on_vec2_is_accepted => (Min, [Vec2, Vec2], Vec2),
    min_on_vec3_is_accepted => (Min, [Vec3, Vec3], Vec3),
    min_on_vec4_is_accepted => (Min, [Vec4, Vec4], Vec4),

    // Max: float scalar and vectors
    max_on_float_is_accepted => (Max, [Float, Float], Float),
    max_on_vec2_is_accepted => (Max, [Vec2, Vec2], Vec2),
    max_on_vec3_is_accepted => (Max, [Vec3, Vec3], Vec3),
    max_on_vec4_is_accepted => (Max, [Vec4, Vec4], Vec4),
}
