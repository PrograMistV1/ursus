use std::process::Command;

fn main() {
    compile_shaders();
}

fn compile_shaders() {
    let out_dir = std::env::var("OUT_DIR").unwrap();

    let shaders = [
        ("shaders/triangle.vert", "triangle.vert.spv"),
        ("shaders/triangle.frag", "triangle.frag.spv"),
        ("shaders/mesh.vert", "mesh.vert.spv"),
        ("shaders/mesh.frag", "mesh.frag.spv"),
        ("shaders/post_process.vert", "post_process.vert.spv"),
        ("shaders/post_process.frag", "post_process.frag.spv"),
        ("shaders/lighting.frag", "lighting.frag.spv"),
        ("shaders/shadow.vert", "shadow.vert.spv"),
        ("shaders/fsr_easu.frag", "fsr_easu.frag.spv"),
        ("shaders/fsr_rcas.frag", "fsr_rcas.frag.spv"),
        ("shaders/loading_pipeline.frag", "loading_pipeline.frag.spv"),
        ("shaders/depth_prepass.vert", "depth_prepass.vert.spv"),
    ];

    for (src, dst) in &shaders {
        let dst_path = format!("{out_dir}/{dst}");

        println!("cargo:rerun-if-changed={src}");

        let status = Command::new("glslc")
            .args([src, "-o", &dst_path, "-I", "shaders"])
            .status()
            .expect("glslc not found — install Vulkan SDK");

        assert!(status.success(), "glslc failed: {src}");
    }
}
