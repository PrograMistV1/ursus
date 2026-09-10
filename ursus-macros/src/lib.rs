mod gpu_material;

use proc_macro::TokenStream;

#[proc_macro_derive(GpuMaterial)]
pub fn derive_gpu_material(input: TokenStream) -> TokenStream {
    gpu_material::derive_gpu_material_impl(input)
}
