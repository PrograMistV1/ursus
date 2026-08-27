use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

/// Step 0 stub: generates an empty impl so the derive macro compiles and
/// can be attached to types. Real std430 layout calculation and GpuData
/// generation land in step 1.
pub(crate) fn derive_gpu_material_impl(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let expanded = quote! {
        impl ::engine_materials::GpuMaterial for #name {
            type GpuData = ();

            fn pack(&self, _resolve_tex: &dyn Fn(u32) -> u32) -> Self::GpuData {
                ()
            }
        }
    };

    expanded.into()
}
