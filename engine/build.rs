use std::process::Command;

fn main() {
    compile_shaders();
}

fn compile_shaders() {
    let out_dir = std::env::var("OUT_DIR").unwrap();

    let shaders = [
        ("shaders/triangle.vert", "triangle.vert.spv"),
        ("shaders/triangle.frag", "triangle.frag.spv"),
    ];

    for (src, dst) in &shaders {
        let dst_path = format!("{out_dir}/{dst}");

        println!("cargo:rerun-if-changed={src}");

        let status = Command::new("glslc")
            .args([src, "-o", &dst_path])
            .status()
            .expect("glslc not found — install Vulkan SDK");

        assert!(status.success(), "glslc failed: {src}");
    }
}