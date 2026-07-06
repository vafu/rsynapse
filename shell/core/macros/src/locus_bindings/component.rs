use proc_macro2::{Ident, TokenStream};
use quote::quote;
use syn::{
    FnArg, ImplItem, ImplItemFn, ItemImpl, Pat, Result, Stmt, Visibility, parse_quote, parse2,
};

use super::{
    config::ComponentConfig,
    expand::{ModuleMode, expand_locus_module},
    view::{StateAccess, ViewBindings, transform_locus_view_attributes},
};

pub(super) fn expand(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let config = parse2::<ComponentConfig>(attr)?;
    let mut item_impl = parse2::<ItemImpl>(item)?;
    let component = item_impl.self_ty.clone();
    let runtime = component_runtime(&item_impl)?;
    let module_ident = config.module.clone();
    let state_ident = config.state.clone();
    let model_ty = config.model;
    let bindings = config.bindings;
    let state_access = if model_ty
        .as_ref()
        .is_some_and(|model_ty| same_type(model_ty, component.as_ref()))
    {
        StateAccess::Model
    } else {
        StateAccess::Field(state_ident.clone())
    };
    let view_bindings = match model_ty.as_ref() {
        Some(_) => ViewBindings::Model,
        None => ViewBindings::Known(&bindings),
    };
    transform_locus_view_attributes(&mut item_impl, &module_ident, &state_access, view_bindings)?;
    inject_post_view_clear(&mut item_impl, &state_access);
    let mut init_found = false;
    let mut update_found = false;

    for impl_item in &mut item_impl.items {
        let ImplItem::Fn(function) = impl_item else {
            continue;
        };

        if function.sig.ident == "init" {
            init_found = true;
            match model_ty.as_ref() {
                Some(model_ty) => {
                    inject_model_subscriptions(function, &state_access, model_ty, runtime)?
                }
                None => inject_start_call(function, &module_ident, &state_ident, runtime)?,
            }
        } else if function.sig.ident == "update" {
            update_found = true;
        }
    }

    if !init_found {
        return Err(syn::Error::new_spanned(
            &item_impl.self_ty,
            "locus component bindings require an init function so watchers can be started",
        ));
    }

    if !update_found {
        let update = update_method(&state_access, model_ty.as_ref(), runtime);
        item_impl.items.push(update);
    }

    let module = match model_ty {
        Some(_) => TokenStream::new(),
        None => expand_locus_module(
            Visibility::Inherited,
            module_ident,
            *component,
            bindings,
            ModuleMode::DirectInput,
        ),
    };

    Ok(quote! {
        #module
        #item_impl
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ComponentRuntime {
    Sync,
    Async,
}

fn component_runtime(item_impl: &ItemImpl) -> Result<ComponentRuntime> {
    let Some((_bang, path, _for)) = &item_impl.trait_ else {
        return Ok(ComponentRuntime::Sync);
    };

    let Some(segment) = path.segments.last() else {
        return Ok(ComponentRuntime::Sync);
    };

    match segment.ident.to_string().as_str() {
        "AsyncComponent" | "SimpleAsyncComponent" => Ok(ComponentRuntime::Async),
        "Component" | "SimpleComponent" => Ok(ComponentRuntime::Sync),
        _ => Ok(ComponentRuntime::Sync),
    }
}

fn same_type(left: &syn::Type, right: &syn::Type) -> bool {
    quote!(#left).to_string() == quote!(#right).to_string()
}

fn inject_post_view_clear(item_impl: &mut ItemImpl, state: &StateAccess) {
    let clear_statement: Stmt = match state {
        StateAccess::Field(state_ident) => parse_quote! {
            model.#state_ident.clear_changed();
        },
        StateAccess::Model => parse_quote! {
            model.clear_changed();
        },
    };

    for item in &mut item_impl.items {
        let ImplItem::Fn(function) = item else {
            continue;
        };
        if function.sig.ident == "post_view" {
            function.block.stmts.push(clear_statement);
            return;
        }
    }

    item_impl.items.push(parse_quote! {
        fn post_view() {
            #clear_statement
        }
    });
}

fn inject_start_call(
    function: &mut ImplItemFn,
    module_ident: &Ident,
    state_ident: &Ident,
    runtime: ComponentRuntime,
) -> Result<()> {
    let sender_ident = sender_ident(function)?;
    let start = match runtime {
        ComponentRuntime::Sync => quote! { start },
        ComponentRuntime::Async => quote! { start_async },
    };

    for index in 0..function.block.stmts.len() {
        let Stmt::Local(local) = &mut function.block.stmts[index] else {
            continue;
        };
        let Pat::Ident(model_ident) = &mut local.pat else {
            continue;
        };
        if model_ident.ident != "model" {
            continue;
        }

        if model_ident.mutability.is_none() {
            model_ident.mutability = Some(Default::default());
        }

        let statement: Stmt = parse_quote! {
            model.#state_ident.set_subscriptions(#module_ident::#start(#sender_ident.clone()));
        };
        function.block.stmts.insert(index + 1, statement);
        return Ok(());
    }

    Err(syn::Error::new_spanned(
        &function.sig,
        "locus component init must bind the component model to a local named model",
    ))
}

fn inject_model_subscriptions(
    function: &mut ImplItemFn,
    state: &StateAccess,
    _model_ty: &syn::Type,
    runtime: ComponentRuntime,
) -> Result<()> {
    let sender_ident = sender_ident(function)?;
    let start = match runtime {
        ComponentRuntime::Sync => quote! { start },
        ComponentRuntime::Async => quote! { start_async },
    };

    for index in 0..function.block.stmts.len() {
        let Stmt::Local(local) = &mut function.block.stmts[index] else {
            continue;
        };
        let Pat::Ident(model_ident) = &mut local.pat else {
            continue;
        };
        if model_ident.ident != "model" {
            continue;
        }

        if model_ident.mutability.is_none() {
            model_ident.mutability = Some(Default::default());
        }

        let start_statement: Stmt = match state {
            StateAccess::Field(state_ident) => parse_quote! {
                let __shell_subscriptions = model.#state_ident.#start(#sender_ident.clone());
            },
            StateAccess::Model => parse_quote! {
                let __shell_subscriptions = model.#start(#sender_ident.clone());
            },
        };
        let set_statement: Stmt = match state {
            StateAccess::Field(state_ident) => parse_quote! {
                model.#state_ident.set_subscriptions(__shell_subscriptions);
            },
            StateAccess::Model => parse_quote! {
                model.set_subscriptions(__shell_subscriptions);
            },
        };
        function.block.stmts.insert(index + 1, start_statement);
        function.block.stmts.insert(index + 2, set_statement);
        return Ok(());
    }

    Err(syn::Error::new_spanned(
        &function.sig,
        "locus component init must bind the component model to a local named model",
    ))
}

fn sender_ident(function: &ImplItemFn) -> Result<Ident> {
    for input in &function.sig.inputs {
        let FnArg::Typed(argument) = input else {
            continue;
        };
        let Pat::Ident(ident) = argument.pat.as_ref() else {
            continue;
        };
        if ident.ident == "sender" || ident.ident == "_sender" {
            return Ok(ident.ident.clone());
        }
    }

    Err(syn::Error::new_spanned(
        &function.sig,
        "locus component init must have a sender parameter named sender",
    ))
}

fn update_method(
    state: &StateAccess,
    model_ty: Option<&syn::Type>,
    runtime: ComponentRuntime,
) -> ImplItem {
    match state {
        StateAccess::Field(state_ident) => match runtime {
            ComponentRuntime::Sync => parse_quote! {
                fn update(&mut self, msg: Self::Input, _sender: ::relm4::ComponentSender<Self>) {
                    let _span = ::shell_core::tracing::trace_span!(
                        "component.update",
                        component = ::std::any::type_name::<Self>(),
                    )
                    .entered();
                    self.#state_ident.update(msg);
                }
            },
            ComponentRuntime::Async => parse_quote! {
                async fn update(&mut self, msg: Self::Input, _sender: ::relm4::AsyncComponentSender<Self>) {
                    let _span = ::shell_core::tracing::trace_span!(
                        "component.update",
                        component = ::std::any::type_name::<Self>(),
                    )
                    .entered();
                    self.#state_ident.update(msg);
                }
            },
        },
        StateAccess::Model => {
            let model_ty = model_ty.expect("model type exists for self model component");
            match runtime {
                ComponentRuntime::Sync => parse_quote! {
                    fn update(&mut self, msg: Self::Input, _sender: ::relm4::ComponentSender<Self>) {
                        let _span = ::shell_core::tracing::trace_span!(
                            "component.update",
                            component = ::std::any::type_name::<Self>(),
                        )
                        .entered();
                        #model_ty::update(self, msg);
                    }
                },
                ComponentRuntime::Async => parse_quote! {
                    async fn update(&mut self, msg: Self::Input, _sender: ::relm4::AsyncComponentSender<Self>) {
                        let _span = ::shell_core::tracing::trace_span!(
                            "component.update",
                            component = ::std::any::type_name::<Self>(),
                        )
                        .entered();
                        #model_ty::update(self, msg);
                    }
                },
            }
        }
    }
}
