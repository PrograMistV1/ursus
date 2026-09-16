use rspirv::binary::{Assemble, Disassemble};
use std::env::temp_dir;
use std::fs::{File, remove_file};
use std::io::Write;
use std::process::Command;
use ursus_shader_ir::lower::lower;
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
fn post_process_lowers_to_nonempty_spirv_module() {
    let shader = build_post_process();
    let typed = validate(&shader).expect("post_process.frag IR tree should type-check");
    let module = lower(&shader, &typed);
    let words = module.assemble();

    // A real module must contain at least the 5-word header plus some instructions.
    assert!(words.len() > 5, "expected more than just the SPIR-V header, got {} words", words.len());
}

/// Requires `spirv-val` (from the Vulkan SDK) to be on PATH. Skips instead of
/// failing if it isn't found, so this test doesn't block contributors without
/// the SDK installed - but it's the only test that actually confirms the
/// output is valid SPIR-V a real driver would accept, so run it locally
/// before trusting any lowering change.
#[test]
fn constant_white_fragment_shader_passes_spirv_val() {
    let Ok(spirv_val_check) = Command::new("spirv-val").arg("--version").output() else {
        eprintln!("spirv-val not found on PATH, skipping validation test");
        return;
    };
    if !spirv_val_check.status.success() {
        eprintln!("spirv-val --version failed, skipping validation test");
        return;
    }

    let shader = build_post_process();
    let typed = validate(&shader).expect("post_process.frag IR tree should type-check");
    let module = lower(&shader, &typed);
    let words = module.assemble();
    let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();

    let tmp_path = temp_dir().join("ursus_shader_ir_test_output.spv");
    let mut file = File::create(&tmp_path).expect("failed to create temp .spv file");
    file.write_all(&bytes).expect("failed to write .spv bytes");
    drop(file);

    let output = Command::new("spirv-val").arg(&tmp_path).output().expect("failed to run spirv-val");

    let _ = remove_file(&tmp_path);

    assert!(
        output.status.success(),
        "spirv-val rejected the generated module:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn constant_white_fragment_shader_disassembly_looks_right() {
    let shader = build_post_process();
    let typed = validate(&shader).expect("post_process.frag IR tree should type-check");
    let module = lower(&shader, &typed);
    let text = module.disassemble();

    println!("{text}");

    // Sanity-check the key pieces are present, without pinning the exact
    // id numbers rspirv assigns (those aren't part of the contract).
    assert!(text.contains("OpEntryPoint Fragment"), "missing Fragment entry point:\n{text}");
    assert!(text.contains("OpExecutionMode"), "missing execution mode:\n{text}");
    assert!(text.contains("OpTypeFloat 32"), "missing float type:\n{text}");
    assert!(text.contains("OpTypeVector"), "missing vec4 type:\n{text}");
    assert!(text.contains("OpDecorate"), "missing Location decoration:\n{text}");
    assert!(text.contains("OpCompositeConstruct"), "missing vec4 construction:\n{text}");
    assert!(text.contains("OpStore"), "missing store to output variable:\n{text}");
    assert!(text.contains("OpFunctionEnd"), "missing function end:\n{text}");
}
