use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Path};

pub(crate) fn derive_component_impl(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let mut deps: Vec<Path> = Vec::new();
    for attr in &input.attrs {
        if attr.path().is_ident("requires") {
            let parsed = attr
                .parse_args_with(syn::punctuated::Punctuated::<Path, syn::Token![,]>::parse_terminated)
                .expect("`#[requires(...)]` expects a comma-separated list of types.");
            deps.extend(parsed);
        }
    }

    let checks = deps.iter().map(|dep| {
        quote! {
            assert!(
                builder.has::<#dep>(),
                "{}::check: required dependency '{}' is missing (insert it earlier using `.insert()`).",
                stringify!(#name),
                stringify!(#dep)
            );
        }
    });

    let on_init_fn: Option<Path> = input
        .attrs
        .iter()
        .find(|a| a.path().is_ident("on_init"))
        .map(|a| a.parse_args::<Path>().expect("#[on_init(...)] expects a function path."));

    let init_impl = match &on_init_fn {
        Some(func) => quote! {
            impl ::ursus_ecs::ComponentInit for #name {
                fn on_init(component: &mut Self, builder: &hecs::EntityBuilder) {
                    #func(component, builder);
                }
            }
        },
        None => quote! {
            impl ::ursus_ecs::ComponentInit for #name {}
        },
    };

    let expanded = quote! {
        impl ::ursus_ecs::Component for #name {
            fn check(component: &mut Self, builder: &hecs::EntityBuilder) {
                #(#checks)*
                <#name as ::ursus_ecs::ComponentInit>::on_init(component, builder);
            }
        }

        #init_impl
    };

    expanded.into()
}
