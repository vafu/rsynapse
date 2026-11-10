use proc_macro::TokenStream;

mod dbus_model;
mod locus_bindings;
mod view_model;

#[proc_macro_attribute]
pub fn bindings(attr: TokenStream, item: TokenStream) -> TokenStream {
    locus_bindings::expand(attr.into(), item.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro_attribute]
pub fn component(attr: TokenStream, item: TokenStream) -> TokenStream {
    locus_bindings::expand_component(attr.into(), item.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro_attribute]
pub fn model(attr: TokenStream, item: TokenStream) -> TokenStream {
    locus_bindings::expand_model(attr.into(), item.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro_attribute]
pub fn view_model(attr: TokenStream, item: TokenStream) -> TokenStream {
    view_model::expand(attr.into(), item.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro_attribute]
pub fn dbus_model(attr: TokenStream, item: TokenStream) -> TokenStream {
    dbus_model::expand(attr.into(), item.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
