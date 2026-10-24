use ursus_shader_ir::{
    clamp, pow, sampler2d, Binding, BindingKind, BuiltinVar, Expr, FragmentShader, PushConstantDesc, Type,
};

fn aces_inline(x: Expr) -> Expr {
    let a = ursus_shader_ir::vec3_splat(Expr::Constant(2.51));
    let b = ursus_shader_ir::vec3_splat(Expr::Constant(0.03));
    let c = ursus_shader_ir::vec3_splat(Expr::Constant(2.43));
    let d = ursus_shader_ir::vec3_splat(Expr::Constant(0.59));
    let e = ursus_shader_ir::vec3_splat(Expr::Constant(0.14));

    let numerator = x.clone() * ((a * x.clone()) + b);
    let denominator = (x.clone() * ((c * x) + d)) + e;
    let ratio = numerator / denominator;

    clamp(ratio, ursus_shader_ir::vec3_splat(Expr::Constant(0.0)), ursus_shader_ir::vec3_splat(Expr::Constant(1.0)))
}

fn fragment() -> FragmentShader {
    let push_constant = PushConstantDesc::new(&[
        ("texel_size", Type::Vec2),
        ("exposure", Type::Float),
        ("flags", Type::UInt),
    ]);

    let hdr_input_binding = ("hdrInput", Binding { set: 0, binding: 0, kind: BindingKind::CombinedImageSampler2D });

    let uv = Expr::Builtin(BuiltinVar::FragCoord).xy() * push_constant.field("texel_size");
    let hdr = sampler2d(Expr::BindingRef("hdrInput"), uv).rgb();
    let tonemapped = aces_inline(hdr * ursus_shader_ir::vec3_splat(push_constant.field("exposure")));

    let gamma_corrected = pow(tonemapped, ursus_shader_ir::vec3_splat(Expr::Constant(1.0 / 2.2)));

    let output = Expr::VecConstruct { ty: Type::Vec4, components: vec![gamma_corrected, Expr::Constant(1.0)] };

    FragmentShader { bindings: vec![hdr_input_binding], push_constant: Some(push_constant), output }
}

pub fn compile() -> anyhow::Result<Vec<u8>> {
    Ok(ursus_shader_ir::compile_fragment(&fragment())?)
}
