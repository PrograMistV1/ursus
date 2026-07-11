use engine_core::assets::{ShaderDef, ShaderRegistry, TextureSlot};

pub fn register_builtin(reg: &mut ShaderRegistry) {
    reg.register_if_absent(
        ShaderDef::from_bytes(
            "diffuse",
            include_bytes!(concat!(env!("OUT_DIR"), "/mesh.vert.spv")).to_vec(),
            include_bytes!(concat!(env!("OUT_DIR"), "/mesh.frag.spv")).to_vec(),
        )
        .with_slot(TextureSlot::Diffuse)
        .with_slot(TextureSlot::Normal),
    );

    reg.register_if_absent(ShaderDef::from_bytes_vert_only(
        "shadow",
        include_bytes!(concat!(env!("OUT_DIR"), "/shadow.vert.spv")).to_vec(),
    ));

    reg.register_if_absent(ShaderDef::from_bytes_vert_only(
        "depth_prepass",
        include_bytes!(concat!(env!("OUT_DIR"), "/depth_prepass.vert.spv")).to_vec(),
    ));

    reg.register_if_absent(ShaderDef::from_bytes(
        "lighting",
        include_bytes!(concat!(env!("OUT_DIR"), "/post_process.vert.spv")).to_vec(),
        include_bytes!(concat!(env!("OUT_DIR"), "/lighting.frag.spv")).to_vec(),
    ));

    reg.register_if_absent(ShaderDef::from_bytes(
        "post_process",
        include_bytes!(concat!(env!("OUT_DIR"), "/post_process.vert.spv")).to_vec(),
        include_bytes!(concat!(env!("OUT_DIR"), "/post_process.frag.spv")).to_vec(),
    ));

    reg.register_if_absent(ShaderDef::from_bytes(
        "fsr_easu",
        include_bytes!(concat!(env!("OUT_DIR"), "/post_process.vert.spv")).to_vec(),
        include_bytes!(concat!(env!("OUT_DIR"), "/fsr_easu.frag.spv")).to_vec(),
    ));

    reg.register_if_absent(ShaderDef::from_bytes(
        "fsr_rcas",
        include_bytes!(concat!(env!("OUT_DIR"), "/post_process.vert.spv")).to_vec(),
        include_bytes!(concat!(env!("OUT_DIR"), "/fsr_rcas.frag.spv")).to_vec(),
    ));

    reg.register_if_absent(ShaderDef::from_bytes(
        "loading",
        include_bytes!(concat!(env!("OUT_DIR"), "/post_process.vert.spv")).to_vec(),
        include_bytes!(concat!(env!("OUT_DIR"), "/loading_pipeline.frag.spv")).to_vec(),
    ));

    reg.register_if_absent(ShaderDef::from_bytes(
        "ui",
        include_bytes!(concat!(env!("OUT_DIR"), "/ui.vert.spv")).to_vec(),
        include_bytes!(concat!(env!("OUT_DIR"), "/ui.frag.spv")).to_vec(),
    ));
}
