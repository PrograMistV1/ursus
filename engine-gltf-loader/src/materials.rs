pub struct PbrMetallicRoughness {
    pub name: String,
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub emissive: [f32; 3],
}

pub struct UnlitMaterial {
    pub name: String,
    pub base_color: [f32; 4],
}
