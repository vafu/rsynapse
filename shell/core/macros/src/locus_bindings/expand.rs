use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};
use syn::{Path, Type, Visibility};

use super::config::{BindingConfig, ModelBindings};

pub(super) enum ModuleMode {
    DirectInput,
    MappedInput(Path),
}

fn shell_subscriptions_ty() -> TokenStream {
    quote! {
        ::std::vec::Vec<
            ::shell_core::source::rxrust::subscription::SubscriptionGuard<
                ::shell_core::source::rxrust::prelude::BoxedSubscriptionSend,
            >,
        >
    }
}

pub(super) fn expand_locus_module(
    visibility: Visibility,
    module_ident: Ident,
    component: Type,
    bindings: Vec<BindingConfig>,
    mode: ModuleMode,
) -> TokenStream {
    let subscriptions_ty = shell_subscriptions_ty();
    let fields = bindings.iter().map(|binding| {
        let field = &binding.field;
        let ty = &binding.ty;
        quote! {
            pub #field: #ty,
        }
    });
    let defaults = bindings.iter().map(|binding| {
        let field = &binding.field;
        quote! {
            #field: ::std::default::Default::default(),
        }
    });
    let message_variants = bindings.iter().map(|binding| {
        let variant = &binding.variant;
        let ty = &binding.ty;
        quote! {
            #variant(::std::result::Result<#ty, ::std::string::String>),
        }
    });
    let field_variants = bindings.iter().map(|binding| {
        let variant = &binding.variant;
        quote! {
            #variant,
        }
    });
    let updates = bindings.iter().map(|binding| {
        let field = &binding.field;
        let variant = &binding.variant;
        let field_variant = &binding.variant;
        quote! {
            Msg::#variant(result) => {
                let update_ok = result.is_ok();
                let _span = ::shell_core::tracing::trace_span!(
                    "source_model.update_field",
                    model = ::std::any::type_name::<Self>(),
                    field = stringify!(#field),
                    variant = stringify!(#variant),
                    ok = update_ok,
                )
                .entered();
                match result {
                    ::std::result::Result::Ok(value) => {
                        self.#field = value;
                        self.changed.mark(Field::#field_variant);
                        self.last_error = ::std::option::Option::None;
                    }
                    ::std::result::Result::Err(error) => {
                        self.last_error = ::std::option::Option::Some(WatchError {
                            field: stringify!(#field),
                            error: error.to_string(),
                        });
                    }
                }
            }
        }
    });
    let watchers = bindings.iter().map(|binding| {
        let variant = &binding.variant;
        let source = &binding.source;
        let ty = &binding.ty;
        let input = match &mode {
            ModuleMode::DirectInput => quote! {
                Msg::#variant(result)
            },
            ModuleMode::MappedInput(message) => quote! {
                super::#message(Msg::#variant(result))
            },
        };

        quote! {
            {
                let update_sender = sender.clone();
                let error_sender = sender.clone();
                let source: ::shell_core::source::Observable<#ty, _> = #source;
                let subscription = {
                    use ::shell_core::source::rx::{
                        IntoBoxedSubscription as _, Observable as _, Subscription as _,
                    };

                    let subscription: ::shell_core::source::rxrust::prelude::BoxedSubscriptionSend =
                        source
                            .on_error(move |error| {
                                let result = ::std::result::Result::Err(error.to_string());
                                let _ = error_sender.input_sender().send(#input);
                            })
                            .subscribe(move |value| {
                        let result = ::std::result::Result::Ok(value);
                        let _ = update_sender.input_sender().send(#input);
                            })
                            .into_boxed();

                    subscription.unsubscribe_when_dropped()
                };
                subscriptions.push(subscription);
            }
        }
    });

    quote! {
        #visibility mod #module_ident {
            #[allow(unused_imports)]
            use super::*;

            #[derive(Debug, Clone, PartialEq, Eq)]
            pub struct WatchError {
                pub field: &'static str,
                pub error: ::std::string::String,
            }

            pub struct Model {
                #(#fields)*
                pub last_error: ::std::option::Option<WatchError>,
                changed: Changed,
                subscriptions: #subscriptions_ty,
            }

            impl ::std::default::Default for Model {
                fn default() -> Self {
                    Self {
                        #(#defaults)*
                        last_error: ::std::option::Option::None,
                        changed: Changed::default(),
                        subscriptions: <#subscriptions_ty>::new(),
                    }
                }
            }

            #[derive(Clone, Copy, Debug, Eq, PartialEq)]
            #[repr(u8)]
            pub enum Field {
                #(#field_variants)*
            }

            #[derive(Debug, Default)]
            struct Changed {
                mask: ::std::cell::Cell<u128>,
            }

            impl Changed {
                fn mark(&self, field: Field) {
                    self.mask.set(self.mask.get() | field.bit());
                }

                fn contains(&self, field: Field) -> bool {
                    self.mask.get() & field.bit() != 0
                }

                fn clear(&self) {
                    self.mask.set(0);
                }
            }

            impl Field {
                const fn bit(self) -> u128 {
                    1 << (self as u8)
                }
            }

            #[derive(Debug)]
            pub enum Msg {
                #(#message_variants)*
            }

            impl Model {
                pub fn changed(&self, field: Field) -> bool {
                    self.changed.contains(field)
                }

                pub fn clear_changed(&self) {
                    self.changed.clear();
                }

                pub fn set_subscriptions(
                    &mut self,
                    subscriptions: #subscriptions_ty,
                ) {
                    self.subscriptions = subscriptions;
                }

                pub fn update(&mut self, msg: Msg) {
                    let _span = ::shell_core::tracing::trace_span!(
                        "source_model.update",
                        model = ::std::any::type_name::<Self>(),
                    )
                    .entered();
                    match msg {
                        #(#updates)*
                    }
                }
            }

            pub fn start(
                sender: ::relm4::ComponentSender<super::#component>,
            ) -> #subscriptions_ty {
                let mut subscriptions = <#subscriptions_ty>::new();
                #(#watchers)*
                subscriptions
            }
        }
    }
}

pub(super) fn expand_model_impl(
    module_ident: Ident,
    model: &Ident,
    fields: &[(Ident, Type)],
    model_bindings: &ModelBindings,
) -> TokenStream {
    let subscriptions_ty = shell_subscriptions_ty();
    let bindings = &model_bindings.sources;
    let nested_models = &model_bindings.nested_models;
    let source_local_fields = fields
        .iter()
        .filter(|(field, _ty)| {
            !bindings.iter().any(|binding| binding.field == *field)
                && !nested_models.iter().any(|nested| nested.field == *field)
        })
        .map(|(field, _ty)| field)
        .collect::<Vec<_>>();
    let context_fields = fields
        .iter()
        .filter(|(field, _ty)| {
            !bindings.iter().any(|binding| binding.field == *field)
                && !nested_models.iter().any(|nested| nested.field == *field)
        })
        .collect::<Vec<_>>();
    let constructor_args = context_fields.iter().map(|(field, ty)| {
        quote! {
            #field: #ty
        }
    });
    let constructor_values = fields.iter().map(|(field, _ty)| {
        if bindings.iter().any(|binding| binding.field == *field) {
            quote! {
                #field: ::std::default::Default::default(),
            }
        } else if let Some(nested) = nested_models.iter().find(|nested| nested.field == *field) {
            let ty = &nested.ty;
            quote! {
                #field: <#ty as ::shell_core::model::SourceModel>::from_default_context(),
            }
        } else {
            quote! {
                #field,
            }
        }
    });
    let default_impl = if context_fields.is_empty() {
        quote! {
            impl ::std::default::Default for #model {
                fn default() -> Self {
                    Self::new()
                }
            }
        }
    } else {
        TokenStream::new()
    };
    let message_variants = bindings.iter().map(|binding| {
        let variant = &binding.variant;
        let ty = &binding.ty;
        quote! {
            #variant(::std::result::Result<#ty, ::std::string::String>),
        }
    });
    let context_update_variant = format_ident!("__ShellContext");
    let context_message_variant =
        source_model_context(fields, bindings, nested_models).map(|(_field, ty)| {
            quote! {
                #context_update_variant(::std::result::Result<#ty, ::std::string::String>),
            }
        });
    let nested_message_variants = nested_models.iter().map(|nested| {
        let variant = &nested.variant;
        let ty = &nested.ty;
        quote! {
            #variant(<#ty as ::shell_core::model::SourceModel>::Msg),
        }
    });
    let field_variants = bindings.iter().map(|binding| {
        let variant = &binding.variant;
        quote! {
            #variant,
        }
    });
    let nested_field_variants = nested_models.iter().map(|nested| {
        let variant = &nested.variant;
        quote! {
            #variant,
        }
    });
    let updates = bindings.iter().map(|binding| {
        let field = &binding.field;
        let variant = &binding.variant;
        let field_variant = &binding.variant;
        let module_ident = &module_ident;
        quote! {
            #module_ident::Msg::#variant(result) => {
                let update_ok = result.is_ok();
                let _span = ::shell_core::tracing::trace_span!(
                    "source_model.update_field",
                    model = ::std::any::type_name::<Self>(),
                    field = stringify!(#field),
                    variant = stringify!(#variant),
                    ok = update_ok,
                )
                .entered();
                match result {
                    ::std::result::Result::Ok(value) => {
                        self.#field = value;
                        self.__shell.mark(#module_ident::Field::#field_variant);
                        self.__shell.clear_error();
                    }
                    ::std::result::Result::Err(error) => {
                        self.__shell.set_error(#module_ident::WatchError {
                            field: stringify!(#field),
                            error: error.to_string(),
                        });
                    }
                }
            }
        }
    });
    let watchers = bindings.iter().map(|binding| {
        let variant = &binding.variant;
        let source = &binding.source;
        let ty = &binding.ty;
        let source_locals = source_local_fields.iter().map(|field| {
            quote! {
                #[allow(unused_variables)]
                let #field = &self.#field;
            }
        });

        quote! {
            {
                let update_sender = sender.clone();
                let error_sender = sender.clone();
                #(#source_locals)*
                let source: ::shell_core::source::Observable<#ty, _> = #source;
                let subscription = {
                    use ::shell_core::source::rx::{
                        IntoBoxedSubscription as _, Observable as _, Subscription as _,
                    };

                    let subscription: ::shell_core::source::rxrust::prelude::BoxedSubscriptionSend =
                        source
                            .on_error(move |error| {
                                let result = ::std::result::Result::Err(error.to_string());
                                let input: <Component as ::relm4::Component>::Input =
                                    #module_ident::Msg::#variant(result).into();
                                let _ = error_sender.input_sender().send(input);
                            })
                            .subscribe(move |value| {
                        let result = ::std::result::Result::Ok(value);
                        let input: <Component as ::relm4::Component>::Input =
                            #module_ident::Msg::#variant(result).into();
                        let _ = update_sender.input_sender().send(input);
                            })
                            .into_boxed();

                    subscription.unsubscribe_when_dropped()
                };
                subscriptions.push(subscription);
            }
        }
    });
    let async_watchers = bindings.iter().map(|binding| {
        let variant = &binding.variant;
        let source = &binding.source;
        let ty = &binding.ty;
        let source_locals = source_local_fields.iter().map(|field| {
            quote! {
                #[allow(unused_variables)]
                let #field = &self.#field;
            }
        });

        quote! {
            {
                let update_sender = sender.clone();
                let error_sender = sender.clone();
                #(#source_locals)*
                let source: ::shell_core::source::Observable<#ty, _> = #source;
                let subscription = {
                    use ::shell_core::source::rx::{
                        IntoBoxedSubscription as _, Observable as _, Subscription as _,
                    };

                    let subscription: ::shell_core::source::rxrust::prelude::BoxedSubscriptionSend =
                        source
                            .on_error(move |error| {
                                let result = ::std::result::Result::Err(error.to_string());
                                let input: <Component as ::relm4::component::AsyncComponent>::Input =
                                    #module_ident::Msg::#variant(result).into();
                                let _ = error_sender.input_sender().send(input);
                            })
                            .subscribe(move |value| {
                        let result = ::std::result::Result::Ok(value);
                        let input: <Component as ::relm4::component::AsyncComponent>::Input =
                            #module_ident::Msg::#variant(result).into();
                        let _ = update_sender.input_sender().send(input);
                            })
                            .into_boxed();

                    subscription.unsubscribe_when_dropped()
                };
                subscriptions.push(subscription);
            }
        }
    });
    let nested_watchers = nested_models.iter().map(|nested| {
        let variant = &nested.variant;
        let source = &nested.source;
        let ty = &nested.ty;
        let source_locals = source_local_fields.iter().map(|field| {
            quote! {
                #[allow(unused_variables)]
                let #field = &self.#field;
            }
        });

        quote! {
            {
                let update_sender = sender.clone();
                #(#source_locals)*
                let source = #source;
                let subscription_group =
                    <#ty as ::shell_core::model::SourceModel>::start_source_model(
                        source,
                        update_sender,
                        |msg| #module_ident::Msg::#variant(msg).into(),
                    );
                subscriptions.extend(subscription_group);
            }
        }
    });
    let nested_updates = nested_models.iter().map(|nested| {
        let field = &nested.field;
        let variant = &nested.variant;
        let ty = &nested.ty;
        let module_ident = &module_ident;
        quote! {
            #module_ident::Msg::#variant(msg) => {
                let _span = ::shell_core::tracing::trace_span!(
                    "source_model.update_nested",
                    model = ::std::any::type_name::<Self>(),
                    field = stringify!(#field),
                    variant = stringify!(#variant),
                )
                .entered();
                <#ty as ::shell_core::model::SourceModel>::update_source_model(
                    &mut self.#field,
                    msg,
                );
                self.__shell.mark(#module_ident::Field::#variant);
            }
        }
    });
    let context_update =
        source_model_context(fields, bindings, nested_models).map(|(field, _ty)| {
            let module_ident = &module_ident;
            quote! {
                #module_ident::Msg::#context_update_variant(result) => {
                    let update_ok = result.is_ok();
                    let _span = ::shell_core::tracing::trace_span!(
                        "source_model.update_context",
                        model = ::std::any::type_name::<Self>(),
                        field = stringify!(#field),
                        ok = update_ok,
                    )
                    .entered();
                    match result {
                        ::std::result::Result::Ok(value) => {
                            self.#field = value;
                            self.__shell.clear_error();
                        }
                        ::std::result::Result::Err(error) => {
                            self.__shell.set_error(#module_ident::WatchError {
                                field: stringify!(#field),
                                error: error.to_string(),
                            });
                        }
                    }
                }
            }
        });
    let source_model_impl = source_model_impl(model, fields, model_bindings, &module_ident);

    quote! {
        pub mod #module_ident {
            #[allow(unused_imports)]
            use super::*;

            #[derive(Clone, Copy, Debug, Eq, PartialEq)]
            #[repr(u8)]
            pub enum Field {
                #(#field_variants)*
                #(#nested_field_variants)*
            }

            #[derive(Debug, Default)]
            struct Changed {
                mask: ::std::cell::Cell<u128>,
            }

            impl Changed {
                fn mark(&self, field: Field) {
                    self.mask.set(self.mask.get() | field.bit());
                }

                fn contains(&self, field: Field) -> bool {
                    self.mask.get() & field.bit() != 0
                }

                fn clear(&self) {
                    self.mask.set(0);
                }
            }

            impl Field {
                const fn bit(self) -> u128 {
                    1 << (self as u8)
                }
            }

            #[derive(Debug, Clone, PartialEq, Eq)]
            pub struct WatchError {
                pub field: &'static str,
                pub error: ::std::string::String,
            }

            #[derive(Debug)]
            pub enum Msg {
                #(#message_variants)*
                #context_message_variant
                #(#nested_message_variants)*
            }

            #[derive(Default)]
            pub(super) struct Runtime {
                last_error: ::std::option::Option<WatchError>,
                changed: Changed,
                subscriptions: #subscriptions_ty,
            }

            impl Runtime {
                pub(super) fn changed(&self, field: Field) -> bool {
                    self.changed.contains(field)
                }

                pub(super) fn mark(&self, field: Field) {
                    self.changed.mark(field);
                }

                pub(super) fn clear_changed(&self) {
                    self.changed.clear();
                }

                pub(super) fn last_error(&self) -> ::std::option::Option<&WatchError> {
                    self.last_error.as_ref()
                }

                pub(super) fn clear_error(&mut self) {
                    self.last_error = ::std::option::Option::None;
                }

                pub(super) fn set_error(&mut self, error: WatchError) {
                    self.last_error = ::std::option::Option::Some(error);
                }

                pub(super) fn set_subscriptions(
                    &mut self,
                    subscriptions: #subscriptions_ty,
                ) {
                    self.subscriptions = subscriptions;
                }
            }
        }

        impl #model {
            pub fn new(#(#constructor_args),*) -> Self {
                Self {
                    #(#constructor_values)*
                    __shell: #module_ident::Runtime::default(),
                }
            }

            pub fn changed(&self, field: #module_ident::Field) -> bool {
                self.__shell.changed(field)
            }

            pub fn clear_changed(&self) {
                self.__shell.clear_changed();
            }

            pub fn last_error(&self) -> ::std::option::Option<&#module_ident::WatchError> {
                self.__shell.last_error()
            }

            pub fn set_subscriptions(
                &mut self,
                subscriptions: #subscriptions_ty,
            ) {
                self.__shell.set_subscriptions(subscriptions);
            }

            pub fn update(&mut self, msg: #module_ident::Msg) {
                let _span = ::shell_core::tracing::trace_span!(
                    "source_model.update",
                    model = ::std::any::type_name::<Self>(),
                )
                .entered();
                match msg {
                    #(#updates)*
                    #context_update
                    #(#nested_updates)*
                }
            }

            pub fn start<Component>(
                &self,
                sender: ::relm4::ComponentSender<Component>,
            ) -> #subscriptions_ty
            where
                Component: ::relm4::Component + 'static,
                <Component as ::relm4::Component>::Input:
                    ::std::convert::From<#module_ident::Msg> + Send,
                <Component as ::relm4::Component>::Output: Send,
                <Component as ::relm4::Component>::CommandOutput: Send,
            {
                let mut subscriptions = <#subscriptions_ty>::new();
                #(#watchers)*
                #(#nested_watchers)*
                subscriptions
            }

            pub fn start_async<Component>(
                &self,
                sender: ::relm4::AsyncComponentSender<Component>,
            ) -> #subscriptions_ty
            where
                Component: ::relm4::component::AsyncComponent + 'static,
                <Component as ::relm4::component::AsyncComponent>::Input:
                    ::std::convert::From<#module_ident::Msg> + Send,
                <Component as ::relm4::component::AsyncComponent>::Output: Send,
                <Component as ::relm4::component::AsyncComponent>::CommandOutput: Send,
            {
                let mut subscriptions = <#subscriptions_ty>::new();
                #(#async_watchers)*
                subscriptions
            }
        }

        #default_impl
        #source_model_impl
    }
}

fn source_model_impl(
    model: &Ident,
    fields: &[(Ident, Type)],
    model_bindings: &ModelBindings,
    module_ident: &Ident,
) -> TokenStream {
    let subscriptions_ty = shell_subscriptions_ty();
    let bindings = &model_bindings.sources;
    let nested_models = &model_bindings.nested_models;
    let context_fields = fields
        .iter()
        .filter(|(field, _ty)| {
            !bindings.iter().any(|binding| binding.field == *field)
                && !nested_models.iter().any(|nested| nested.field == *field)
        })
        .collect::<Vec<_>>();

    let [(context_field, context_ty)] = context_fields.as_slice() else {
        return TokenStream::new();
    };
    if !is_option_type(context_ty) {
        return TokenStream::new();
    }

    let context_update_variant = format_ident!("__ShellContext");
    let source_model_watchers = bindings.iter().map(|binding| {
        let variant = &binding.variant;
        let source = &binding.source;
        let ty = &binding.ty;

        quote! {
            {
                let update_sender = sender.clone();
                let error_sender = sender.clone();
                let map = map.clone();
                let error_map = map.clone();
                #[allow(unused_variables)]
                let #context_field = &context;
                let source: ::shell_core::source::Observable<#ty, _> = #source;
                let subscription = {
                    use ::shell_core::source::rx::{
                        IntoBoxedSubscription as _, Observable as _, Subscription as _,
                    };

                    let subscription: ::shell_core::source::rxrust::prelude::BoxedSubscriptionSend =
                        source
                            .on_error(move |error| {
                                let _ = error_sender.input_sender().send(error_map(#module_ident::Msg::#variant(
                                    ::std::result::Result::Err(error.to_string()),
                                )));
                            })
                            .subscribe(move |value| {
                        let _ = update_sender.input_sender().send(map(#module_ident::Msg::#variant(
                            ::std::result::Result::Ok(value),
                        )));
                            })
                            .into_boxed();

                    subscription.unsubscribe_when_dropped()
                };
                subscriptions.push(subscription);
            }
        }
    });
    let nested_source_model_watchers = nested_models.iter().map(|nested| {
        let variant = &nested.variant;
        let source = &nested.source;
        let ty = &nested.ty;

        quote! {
            {
                let update_sender = sender.clone();
                let map = map.clone();
                #[allow(unused_variables)]
                let #context_field = &context;
                let source = #source;
                let subscription_group =
                    <#ty as ::shell_core::model::SourceModel>::start_source_model(
                        source,
                        update_sender,
                        move |msg| map(#module_ident::Msg::#variant(msg)),
                    );
                subscriptions.extend(subscription_group);
            }
        }
    });

    quote! {
        impl ::shell_core::model::SourceModel for #model {
            type Context = #context_ty;
            type Msg = #module_ident::Msg;

            fn from_default_context() -> Self
            where
                Self::Context: ::std::default::Default,
            {
                Self::new(::std::default::Default::default())
            }

            fn update_source_model(&mut self, msg: Self::Msg) {
                Self::update(self, msg);
            }

            fn start_source_model<Component, E, Map>(
                source: ::shell_core::source::Observable<Self::Context, E>,
                sender: ::relm4::ComponentSender<Component>,
                map: Map,
            ) -> #subscriptions_ty
            where
                Component: ::relm4::Component + 'static,
                <Component as ::relm4::Component>::Input: Send,
                <Component as ::relm4::Component>::Output: Send,
                <Component as ::relm4::Component>::CommandOutput: Send,
                E: ::std::fmt::Display + Send + Sync + 'static,
                Map: Fn(Self::Msg) -> <Component as ::relm4::Component>::Input
                    + Clone
                    + Send
                    + 'static,
            {
                let mut subscriptions = <#subscriptions_ty>::new();
                let mut context_subscriptions = <#subscriptions_ty>::new();
                let update_sender = sender.clone();
                let error_sender = sender.clone();
                let update_map = map.clone();
                let error_map = map.clone();
                let subscription = {
                    use ::shell_core::source::rx::{
                        IntoBoxedSubscription as _, Observable as _, Subscription as _,
                    };

                    let subscription: ::shell_core::source::rxrust::prelude::BoxedSubscriptionSend =
                        source
                            .on_error(move |error| {
                                let _ = error_sender.input_sender().send(error_map.clone()(#module_ident::Msg::#context_update_variant(
                                    ::std::result::Result::Err(error.to_string()),
                                )));
                            })
                            .subscribe(move |context| {
                        context_subscriptions.clear();
                        let _ = update_sender.input_sender().send(update_map.clone()(#module_ident::Msg::#context_update_variant(
                            ::std::result::Result::Ok(context.clone()),
                        )));
                        context_subscriptions =
                            #model::start_for_source_context(context, update_sender.clone(), update_map.clone());
                            })
                            .into_boxed();

                    subscription.unsubscribe_when_dropped()
                };
                subscriptions.push(subscription);
                subscriptions
            }
        }

        impl #model {
            fn start_for_source_context<Component, Map>(
                context: <Self as ::shell_core::model::SourceModel>::Context,
                sender: ::relm4::ComponentSender<Component>,
                map: Map,
            ) -> #subscriptions_ty
            where
                Component: ::relm4::Component + 'static,
                <Component as ::relm4::Component>::Input: Send,
                <Component as ::relm4::Component>::Output: Send,
                <Component as ::relm4::Component>::CommandOutput: Send,
                Map: Fn(<Self as ::shell_core::model::SourceModel>::Msg) -> <Component as ::relm4::Component>::Input
                    + Clone
                    + Send
                    + 'static,
            {
                let mut subscriptions = <#subscriptions_ty>::new();
                #(#source_model_watchers)*
                #(#nested_source_model_watchers)*
                subscriptions
            }
        }
    }
}

fn source_model_context<'a>(
    fields: &'a [(Ident, Type)],
    bindings: &[BindingConfig],
    nested_models: &[super::config::NestedModelConfig],
) -> Option<(&'a Ident, &'a Type)> {
    let context_fields = fields
        .iter()
        .filter(|(field, _ty)| {
            !bindings.iter().any(|binding| binding.field == *field)
                && !nested_models.iter().any(|nested| nested.field == *field)
        })
        .collect::<Vec<_>>();

    let [(field, ty)] = context_fields.as_slice() else {
        return None;
    };

    if !is_option_type(ty) {
        return None;
    }

    Some((field, ty))
}

fn is_option_type(ty: &Type) -> bool {
    let Type::Path(type_path) = ty else {
        return false;
    };
    type_path
        .path
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "Option")
}
