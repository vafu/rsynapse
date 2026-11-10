mod component;
mod config;
mod expand;
mod view;

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Fields, Ident, ItemMod, ItemStruct, Result, Type, parse_quote, parse2};

use config::{BindingsConfig, ModelConfig};
use expand::{ModuleMode, expand_locus_module, expand_model_impl};

pub fn expand(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let config = parse2::<BindingsConfig>(attr)?;
    let module = parse2::<ItemMod>(item)?;
    let visibility = module.vis;
    let module_ident = module.ident;

    if module.content.is_none() {
        return Err(syn::Error::new_spanned(
            module_ident,
            "source binding modules must use inline module syntax: mod bindings {}",
        ));
    }

    Ok(expand_locus_module(
        visibility,
        module_ident,
        Type::Path(syn::TypePath {
            qself: None,
            path: config.component,
        }),
        config.bindings,
        ModuleMode::MappedInput(config.message),
    ))
}

pub fn expand_component(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    component::expand(attr, item)
}

pub fn expand_model(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let config = parse2::<ModelConfig>(attr)?;
    let mut item = parse2::<ItemStruct>(item)?;
    let model_bindings = config::model_bindings(&item)?;
    let model = item.ident.clone();
    let fields = model_fields(&item)?;
    let module = config.module;

    strip_locus_field_attrs(&mut item);
    push_generated_model_fields(&mut item, &module)?;

    let generated = expand_model_impl(module, &model, &fields, &model_bindings);

    Ok(quote! {
        #item
        #generated
    })
}

fn model_fields(item: &ItemStruct) -> Result<Vec<(Ident, Type)>> {
    let Fields::Named(fields) = &item.fields else {
        return Err(syn::Error::new_spanned(
            item,
            "source models must use named fields",
        ));
    };

    fields
        .named
        .iter()
        .map(|field| {
            let ident = field.ident.clone().ok_or_else(|| {
                syn::Error::new_spanned(field, "source models must use named fields")
            })?;
            Ok((ident, field.ty.clone()))
        })
        .collect()
}

fn strip_locus_field_attrs(item: &mut ItemStruct) {
    let Fields::Named(fields) = &mut item.fields else {
        return;
    };

    for field in &mut fields.named {
        field.attrs.retain(|attr| {
            !attr.path().is_ident("locus")
                && !attr.path().is_ident("source")
                && !attr.path().is_ident("model")
        });
    }
}

fn push_generated_model_fields(item: &mut ItemStruct, module: &Ident) -> Result<()> {
    let Fields::Named(fields) = &mut item.fields else {
        return Err(syn::Error::new_spanned(
            item,
            "source models must use named fields",
        ));
    };

    for field in &fields.named {
        let Some(ident) = &field.ident else {
            continue;
        };
        if ident == "__shell" {
            return Err(syn::Error::new_spanned(
                ident,
                "source models reserve __shell",
            ));
        }
    }

    fields.named.push(parse_quote! {
        __shell: #module::Runtime
    });

    Ok(())
}

#[cfg(test)]
#[path = "test.rs"]
mod tests;
