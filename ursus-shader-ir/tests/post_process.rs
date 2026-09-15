use ursus_shader_ir::{
    BinOp, Binding, BindingKind, BuiltinVar, Expr, FragmentShader, IntrinsicFn, PushConstantDesc, Type,
    validate::validate,
};

fn mul(lhs: Expr, rhs: Expr) -> Expr {
    Expr::Binary { op: BinOp::Mul, lhs: Box::new(lhs), rhs: Box::new(rhs) }
}
fn div(lhs: Expr, rhs: Expr) -> Expr {
    Expr::Binary { op: BinOp::Div, lhs: Box::new(lhs), rhs: Box::new(rhs) }
}
fn add(lhs: Expr, rhs: Expr) -> Expr {
    Expr::Binary { op: BinOp::Add, lhs: Box::new(lhs), rhs: Box::new(rhs) }
}
fn swizzle(base: Expr, pattern: &'static str) -> Expr {
    Expr::Swizzle { base: Box::new(base), pattern }
}

fn aces_inline(x: Expr) -> Expr {
    let a = Expr::Constant(2.51);
    let b = Expr::Constant(0.03);
    let c = Expr::Constant(2.43);
    let d = Expr::Constant(0.59);
    let e = Expr::Constant(0.14);

    let numerator = mul(x.clone(), add(mul(a, x.clone()), b));
    let denominator = add(mul(x.clone(), add(mul(c, x), d)), e);
    let ratio = div(numerator, denominator);

    Expr::Intrinsic { func: IntrinsicFn::Clamp, args: vec![ratio, Expr::Constant(0.0), Expr::Constant(1.0)] }
}

fn build_post_process() -> FragmentShader {
    let push_constant = PushConstantDesc::new(&[
        ("texel_size", Type::Vec2),
        ("exposure", Type::Float),
        ("flags", Type::UInt),
    ]);

    let hdr_input_binding = ("hdrInput", Binding { set: 0, binding: 0, kind: BindingKind::CombinedImageSampler2D });

    let uv = mul(swizzle(Expr::Builtin(BuiltinVar::FragCoord), "xy"), push_constant.field("texel_size"));
    let hdr = swizzle(Expr::Sample2D { texture: Box::new(Expr::BindingRef("hdrInput")), uv: Box::new(uv) }, "rgb");
    let tonemapped = aces_inline(mul(hdr, push_constant.field("exposure")));

    let gamma_corrected = Expr::Intrinsic {
        func: IntrinsicFn::Pow,
        args: vec![
            tonemapped,
            Expr::VecConstruct { ty: Type::Vec3, components: vec![Expr::Constant(1.0 / 2.2)] },
        ],
    };

    let output = Expr::VecConstruct { ty: Type::Vec4, components: vec![gamma_corrected, Expr::Constant(1.0)] };

    FragmentShader { bindings: vec![hdr_input_binding], push_constant: Some(push_constant), output }
}

#[test]
fn post_process_type_checks_to_vec4() {
    let shader = build_post_process();
    let result_ty = validate(&shader).expect("post_process.frag IR tree should type-check");
    assert_eq!(result_ty, Type::Vec4);
}
