use super::shader_registry::{ShaderDef, ShaderRegistry, TextureSlot};

pub fn register_builtin(reg: &mut ShaderRegistry) {
    reg.register(ShaderDef::from_bytes(
        "unlit",
        include_bytes!(concat!(env!("OUT_DIR"), "/mesh.vert.spv")).to_vec(),
        include_bytes!(concat!(env!("OUT_DIR"), "/mesh.frag.spv")).to_vec(),
    ));

    reg.register(
        ShaderDef::from_bytes(
            "diffuse",
            include_bytes!(concat!(env!("OUT_DIR"), "/mesh.vert.spv")).to_vec(),
            include_bytes!(concat!(env!("OUT_DIR"), "/mesh.frag.spv")).to_vec(),
        )
        .with_slot(TextureSlot::Diffuse),
    );

    reg.register(ShaderDef::from_bytes_vert_only(
        "shadow",
        include_bytes!(concat!(env!("OUT_DIR"), "/shadow.vert.spv")).to_vec(),
    ));

    reg.register(ShaderDef::from_bytes_vert_only(
        "depth_prepass",
        include_bytes!(concat!(env!("OUT_DIR"), "/depth_prepass.vert.spv")).to_vec(),
    ));

    reg.register(ShaderDef::from_bytes(
        "lighting",
        include_bytes!(concat!(env!("OUT_DIR"), "/post_process.vert.spv")).to_vec(),
        include_bytes!(concat!(env!("OUT_DIR"), "/lighting.frag.spv")).to_vec(),
    ));

    reg.register(ShaderDef::from_bytes(
        "post_process",
        include_bytes!(concat!(env!("OUT_DIR"), "/post_process.vert.spv")).to_vec(),
        include_bytes!(concat!(env!("OUT_DIR"), "/post_process.frag.spv")).to_vec(),
    ));

    reg.register(ShaderDef::from_bytes(
        "fsr_easu",
        include_bytes!(concat!(env!("OUT_DIR"), "/post_process.vert.spv")).to_vec(),
        include_bytes!(concat!(env!("OUT_DIR"), "/fsr_easu.frag.spv")).to_vec(),
    ));

    reg.register(ShaderDef::from_bytes(
        "fsr_rcas",
        include_bytes!(concat!(env!("OUT_DIR"), "/post_process.vert.spv")).to_vec(),
        include_bytes!(concat!(env!("OUT_DIR"), "/fsr_rcas.frag.spv")).to_vec(),
    ));
}
