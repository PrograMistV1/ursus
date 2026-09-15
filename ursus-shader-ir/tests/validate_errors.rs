use ursus_shader_ir::{
    BinOp, BuiltinVar, Expr, FragmentShader, IntrinsicFn, Type, validate::ValidationError, validate::validate,
};

fn shader_with_output(output: Expr) -> FragmentShader {
    FragmentShader { bindings: vec![], push_constant: None, output }
}

#[test]
fn swizzle_out_of_range_component_is_rejected() {
    // gl_FragCoord is Vec4, but .z is fine on Vec4 - use a Vec2 source instead
    // by swizzling down first, then trying an out-of-range component on it.
    let vec2_value = Expr::Swizzle { base: Box::new(Expr::Builtin(BuiltinVar::FragCoord)), pattern: "xy" };
    let invalid = Expr::Swizzle { base: Box::new(vec2_value), pattern: "z" }; // Vec2 has no .z

    let shader = shader_with_output(invalid);
    let err = validate(&shader).expect_err("swizzling .z on a Vec2 should fail");
    assert!(matches!(err, ValidationError::InvalidSwizzle { base_ty: Type::Vec2, pattern: "z" }));
}

#[test]
fn swizzle_mixing_positional_and_color_names_is_rejected() {
    let invalid = Expr::Swizzle { base: Box::new(Expr::Builtin(BuiltinVar::FragCoord)), pattern: "xg" }; // mixes xyzw/rgba

    let shader = shader_with_output(invalid);
    let err = validate(&shader).expect_err("mixing positional and color swizzle names should fail");
    assert!(matches!(err, ValidationError::InvalidSwizzle { pattern: "xg", .. }));
}

#[test]
fn binary_op_between_incompatible_vector_widths_is_rejected() {
    let vec3 = Expr::VecConstruct {
        ty: Type::Vec3,
        components: vec![Expr::Constant(1.0), Expr::Constant(2.0), Expr::Constant(3.0)],
    };
    let vec2 = Expr::VecConstruct { ty: Type::Vec2, components: vec![Expr::Constant(1.0), Expr::Constant(2.0)] };

    let invalid = Expr::Binary { op: BinOp::Add, lhs: Box::new(vec3), rhs: Box::new(vec2) };

    let shader = shader_with_output(invalid);
    let err = validate(&shader).expect_err("Vec3 + Vec2 should not type-check");
    assert!(matches!(
        err,
        ValidationError::BinaryTypeMismatch { op: BinOp::Add, lhs_ty: Type::Vec3, rhs_ty: Type::Vec2 }
    ));
}

#[test]
fn intrinsic_called_with_wrong_arity_is_rejected() {
    // dot(x) - only one argument, dot needs two.
    let invalid = Expr::Intrinsic { func: IntrinsicFn::Dot, args: vec![Expr::Constant(1.0)] };

    let shader = shader_with_output(invalid);
    let err = validate(&shader).expect_err("dot() with one argument should fail arity check");
    assert!(matches!(err, ValidationError::IntrinsicArity { func: IntrinsicFn::Dot, expected: 2, got: 1 }));
}

#[test]
fn intrinsic_dot_on_scalars_is_rejected() {
    // dot(1.0, 2.0) - same type on both sides, but neither is a vector.
    let invalid = Expr::Intrinsic { func: IntrinsicFn::Dot, args: vec![Expr::Constant(1.0), Expr::Constant(2.0)] };

    let shader = shader_with_output(invalid);
    let err = validate(&shader).expect_err("dot() on two scalars should fail - dot requires vector operands");
    assert!(matches!(err, ValidationError::IntrinsicTypeMismatch { func: IntrinsicFn::Dot, .. }));
}

#[test]
fn vec_construct_with_wrong_total_component_count_is_rejected() {
    // vec4 built from only two floats - needs 4 components (or the 1-component splat form), not 2.
    let invalid = Expr::VecConstruct { ty: Type::Vec4, components: vec![Expr::Constant(1.0), Expr::Constant(2.0)] };

    let shader = shader_with_output(invalid);
    let err = validate(&shader).expect_err("vec4 from 2 components (not 1 or 4) should fail");
    assert!(matches!(err, ValidationError::VecConstructArity { ty: Type::Vec4, provided_components: 2 }));
}

#[test]
fn vec_construct_splat_form_is_accepted() {
    // vec3(x) - single scalar filling all 3 components, should be valid.
    let splat = Expr::VecConstruct { ty: Type::Vec3, components: vec![Expr::Constant(1.0)] };

    let shader = shader_with_output(splat);
    let result_ty = validate(&shader).expect("vec3(scalar) splat form should type-check");
    assert_eq!(result_ty, Type::Vec3);
}

#[test]
fn push_constant_field_without_a_declared_block_is_rejected() {
    let invalid = Expr::PushConstantField { offset: 0, ty: Type::Float };

    let shader = shader_with_output(invalid); // push_constant: None
    let err = validate(&shader).expect_err("referencing a push constant field with no block declared should fail");
    assert!(matches!(err, ValidationError::NoPushConstantBlock));
}

#[test]
fn unknown_binding_reference_is_rejected() {
    let invalid = Expr::Sample2D {
        texture: Box::new(Expr::BindingRef("does_not_exist")),
        uv: Box::new(Expr::VecConstruct { ty: Type::Vec2, components: vec![Expr::Constant(0.0)] }),
    };

    let shader = shader_with_output(invalid);
    let err = validate(&shader).expect_err("referencing an undeclared binding should fail");
    assert!(matches!(err, ValidationError::UnknownBinding { name: "does_not_exist" }));
}
