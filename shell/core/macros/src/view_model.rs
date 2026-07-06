use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{
    Expr, FnArg, ImplItem, ImplItemFn, ItemImpl, Pat, Result, Stmt, Token, Type, parse_quote,
    parse2, punctuated::Punctuated,
};

pub fn expand(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let config = parse2::<ViewModelConfig>(attr)?;
    let mut item_impl = parse2::<ItemImpl>(item)?;
    let runtime = component_runtime(&item_impl);
    let module = config.module;
    let model = config.model;
    let source = config.source;
    let mut init_found = false;
    let mut input_found = false;
    let mut update_found = false;

    for item in &mut item_impl.items {
        match item {
            ImplItem::Type(ty) if ty.ident == "Input" => input_found = true,
            ImplItem::Fn(function) if function.sig.ident == "init" => {
                init_found = true;
                inject_subscription(function, &module, &source)?;
            }
            ImplItem::Fn(function) if function.sig.ident == "update" => {
                update_found = true;
            }
            _ => {}
        }
    }

    if !init_found {
        return Err(syn::Error::new_spanned(
            &item_impl.self_ty,
            "view_model components require an init function so the source can be started",
        ));
    }

    if !input_found {
        let input: ImplItem = parse_quote! {
            type Input = #module::Msg;
        };
        item_impl.items.insert(1.min(item_impl.items.len()), input);
    }

    if !update_found {
        item_impl.items.push(update_method(runtime));
    }

    let generated = generated_module(&module, &model);

    Ok(quote! {
        #generated
        #item_impl
    })
}

struct ViewModelConfig {
    module: Ident,
    model: Type,
    source: Expr,
}

impl Parse for ViewModelConfig {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let mut module = None;
        let mut model = None;
        let mut source = None;
        let entries = Punctuated::<ConfigEntry, Token![,]>::parse_terminated(input)?;

        for entry in entries {
            match entry {
                ConfigEntry::Module(value) => module = Some(value),
                ConfigEntry::Model(value) => model = Some(value),
                ConfigEntry::Source(value) => source = Some(value),
            }
        }

        Ok(Self {
            module: module.unwrap_or_else(|| format_ident!("sources")),
            model: model.ok_or_else(|| input.error("missing model = ViewModelType"))?,
            source: source.ok_or_else(|| input.error("missing source = expression"))?,
        })
    }
}

enum ConfigEntry {
    Module(Ident),
    Model(Type),
    Source(Expr),
}

impl Parse for ConfigEntry {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let ident = input.parse::<Ident>()?;
        input.parse::<Token![=]>()?;
        match ident.to_string().as_str() {
            "module" => {
                let path = input.parse::<syn::Path>()?;
                path.get_ident()
                    .cloned()
                    .map(Self::Module)
                    .ok_or_else(|| syn::Error::new_spanned(path, "module must be an identifier"))
            }
            "model" => Ok(Self::Model(input.parse()?)),
            "source" => Ok(Self::Source(input.parse()?)),
            _ => Err(syn::Error::new_spanned(
                ident,
                "expected module, model, or source",
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ComponentRuntime {
    Sync,
    Async,
}

fn component_runtime(item_impl: &ItemImpl) -> ComponentRuntime {
    let Some((_bang, path, _for)) = &item_impl.trait_ else {
        return ComponentRuntime::Sync;
    };
    let Some(segment) = path.segments.last() else {
        return ComponentRuntime::Sync;
    };

    match segment.ident.to_string().as_str() {
        "AsyncComponent" | "SimpleAsyncComponent" => ComponentRuntime::Async,
        _ => ComponentRuntime::Sync,
    }
}

fn inject_subscription(function: &mut ImplItemFn, module: &Ident, source: &Expr) -> Result<()> {
    let sender = sender_ident(function)?;

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
            model.__shell.set_subscription(#module::start(#source, #sender.input_sender().clone()));
        };
        function.block.stmts.insert(index + 1, statement);
        return Ok(());
    }

    Err(syn::Error::new_spanned(
        &function.sig,
        "view_model component init must bind the component model to a local named model",
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
        "view_model component init must have a sender parameter named sender",
    ))
}

fn update_method(runtime: ComponentRuntime) -> ImplItem {
    match runtime {
        ComponentRuntime::Sync => parse_quote! {
            fn update(&mut self, msg: Self::Input, _sender: ::relm4::ComponentSender<Self>) {
                self.__shell.update(&mut self.vm, msg);
            }
        },
        ComponentRuntime::Async => parse_quote! {
            async fn update(&mut self, msg: Self::Input, _sender: ::relm4::AsyncComponentSender<Self>) {
                self.__shell.update(&mut self.vm, msg);
            }
        },
    }
}

fn generated_module(module: &Ident, model: &Type) -> TokenStream {
    quote! {
        mod #module {
            use super::*;

            #[derive(Debug)]
            pub enum Msg {
                ViewModel(::std::result::Result<#model, ::std::string::String>),
            }

            #[derive(Default)]
            pub struct Runtime {
                subscription: ::std::option::Option<::shell_core::source::SourceSubscription>,
                last_error: ::std::option::Option<::std::string::String>,
            }

            impl Runtime {
                pub fn set_subscription(
                    &mut self,
                    subscription: ::shell_core::source::SourceSubscription,
                ) {
                    self.subscription = ::std::option::Option::Some(subscription);
                }

                pub fn last_error(&self) -> ::std::option::Option<&str> {
                    self.last_error.as_deref()
                }

                pub fn update(&mut self, target: &mut #model, msg: Msg) {
                    match msg {
                        Msg::ViewModel(::std::result::Result::Ok(value)) => {
                            *target = value;
                            self.last_error = ::std::option::Option::None;
                        }
                        Msg::ViewModel(::std::result::Result::Err(error)) => {
                            self.last_error = ::std::option::Option::Some(error);
                        }
                    }
                }
            }

            pub fn start(
                source: ::shell_core::source::Source<#model>,
                sender: ::relm4::Sender<Msg>,
            ) -> ::shell_core::source::SourceSubscription {
                let value_sender = sender.clone();
                source.subscribe(
                    move |value| value_sender.emit(Msg::ViewModel(::std::result::Result::Ok(value))),
                    move |error| sender.emit(Msg::ViewModel(::std::result::Result::Err(error))),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use quote::quote;

    use super::expand;

    #[test]
    fn expands_async_view_model_component() {
        let expanded = expand(
            quote!(model = WindowTilesVm, source = window_tiles_vm()),
            quote! {
                impl SimpleAsyncComponent for WindowTiles {
                    type Init = ();
                    type Output = ();

                    view! {
                        gtk::Box {}
                    }

                    async fn init(
                        _init: Self::Init,
                        root: Self::Root,
                        sender: AsyncComponentSender<Self>,
                    ) -> AsyncComponentParts<Self> {
                        let model = WindowTiles::new();
                        let widgets = view_output!();
                        AsyncComponentParts { model, widgets }
                    }
                }
            },
        )
        .unwrap()
        .to_string();

        assert!(expanded.contains("mod sources"));
        assert!(expanded.contains("type Input = sources :: Msg"));
        assert!(expanded.contains("model . __shell . set_subscription"));
        assert!(expanded.contains("async fn update"));
        assert!(expanded.contains("window_tiles_vm ()"));
    }

    #[test]
    fn expands_collection_view_model_type() {
        let expanded = expand(
            quote!(model = Vec<WindowTile>, source = window_tiles()),
            quote! {
                impl SimpleAsyncComponent for WindowTiles {
                    type Init = ();
                    type Output = ();

                    view! {
                        gtk::Box {}
                    }

                    async fn init(
                        _init: Self::Init,
                        root: Self::Root,
                        sender: AsyncComponentSender<Self>,
                    ) -> AsyncComponentParts<Self> {
                        let model = WindowTiles::new();
                        let widgets = view_output!();
                        AsyncComponentParts { model, widgets }
                    }
                }
            },
        )
        .unwrap()
        .to_string();

        assert!(expanded.contains("use super :: *"));
        assert!(expanded.contains("Source < Vec < WindowTile > >"));
        assert!(!expanded.contains("super :: Vec"));
    }
}
