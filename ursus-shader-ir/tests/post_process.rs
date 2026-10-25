use ursus_shader_ir::{
    Binding, BindingKind, BuiltinVar, Expr, FragmentShader, PushConstantDesc, Type, clamp, pow, sampler2d,
    validate::validate, vec3, vec4,
};

fn aces_inline(x: Expr) -> Expr {
    let a = vec3!(Expr::Constant(2.51));
    let b = vec3!(Expr::Constant(0.03));
    let c = vec3!(Expr::Constant(2.43));
    let d = vec3!(Expr::Constant(0.59));
    let e = vec3!(Expr::Constant(0.14));

    let numerator = x.clone() * ((a * x.clone()) + b);
    let denominator = (x.clone() * ((c * x) + d)) + e;
    let ratio = numerator / denominator;

    clamp(ratio, vec3!(Expr::Constant(0.0)), vec3!(Expr::Constant(1.0)))
}

fn build_post_process() -> FragmentShader {
    let push_constant = PushConstantDesc::new(&[
        ("texel_size", Type::Vec2),
        ("exposure", Type::Float),
        ("flags", Type::UInt),
    ]);

    let hdr_input_binding = ("hdrInput", Binding { set: 0, binding: 0, kind: BindingKind::CombinedImageSampler2D });

    let uv = Expr::Builtin(BuiltinVar::FragCoord).xy() * push_constant.field("texel_size");
    let hdr = sampler2d(Expr::BindingRef("hdrInput"), uv).rgb();
    let tonemapped = aces_inline(hdr * vec3!(push_constant.field("exposure")));

    let gamma_corrected = pow(tonemapped, vec3!(Expr::Constant(1.0 / 2.2)));

    let output = vec4!(gamma_corrected, Expr::Constant(1.0));

    FragmentShader { bindings: vec![hdr_input_binding], push_constant: Some(push_constant), output }
}

#[test]
fn post_process_type_checks_to_vec4() {
    let shader = build_post_process();
    let result_ty = validate(&shader).expect("post_process.frag IR tree should type-check");
    assert_eq!(result_ty.ty, Type::Vec4);
}
