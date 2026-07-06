use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{ItemMod, parse2};

use super::*;
use crate::locus_bindings::config::ComponentConfig;

#[test]
fn parses_binding_config() {
    let config = parse2::<BindingsConfig>(quote! {
        component = Bar,
        message = BarMsg::Locus,
        selected_window_title: String = selected_window_title(),
    })
    .unwrap();

    assert_eq!(config.bindings.len(), 1);
    assert_eq!(config.bindings[0].field, "selected_window_title");
    assert_eq!(config.bindings[0].variant, "SelectedWindowTitle");
}

#[test]
fn expands_inline_module() {
    let attr = quote! {
        component = Bar,
        message = BarMsg::Locus,
        selected_window_title: String = selected_window_title(),
    };
    let item = quote! {
        mod locus {}
    };

    let expanded = expand(attr, item).unwrap();
    let _module: ItemMod = parse2(expanded).unwrap();
}

#[test]
fn expands_component_impl() {
    let attr = quote! {
        selected_window_title: String = selected_window_title(),
    };
    let item = quote! {
        impl SimpleComponent for Bar {
            type Init = BarInit;
            type Input = sources::Msg;
            type Output = ();

            view! {
                gtk::Window {}
            }

            fn init(
                init: Self::Init,
                root: Self::Root,
                sender: ComponentSender<Self>,
            ) -> ComponentParts<Self> {
                let model = Bar {
                    title: init.title,
                    sources: sources::Model::default(),
                };
                let widgets = view_output!();
                ComponentParts { model, widgets }
            }
        }
    };

    let expanded = expand_component(attr, item).unwrap();
    let source = expanded.to_string();
    assert!(source.contains("mod sources"));
    assert!(source.contains("model . sources . set_subscriptions (sources :: start"));
    assert!(source.contains("fn update"));
    assert!(source.contains(". on_error"));
    assert!(source.contains(". subscribe"));
    assert!(source.contains("shell_core :: source :: Observable < String"));
    assert!(source.contains("BoxedSubscriptionSend"));
    assert!(source.contains("subscriptions . push (subscription)"));
}

#[test]
fn expands_dbus_property_source() {
    let attr = quote! {
        battery_percent: f64 = BATTERY.bind(Battery::PERCENTAGE),
    };
    let item = component_item();

    let expanded = expand_component(attr, item).unwrap();
    let source = expanded.to_string();

    assert!(source.contains(". on_error"));
    assert!(source.contains(". subscribe"));
    assert!(source.contains("shell_core :: source :: Observable < f64"));
    assert!(source.contains("BATTERY . bind"));
}

#[test]
fn expands_mixed_source_sources() {
    let attr = quote! {
        selected_window_title: String = selected_window_title(),
        battery_percent: f64 = BATTERY.bind(Battery::PERCENTAGE),
    };
    let item = component_item();

    let expanded = expand_component(attr, item).unwrap();
    let source = expanded.to_string();

    assert_eq!(source.matches(". on_error").count(), 2);
}

#[test]
fn expands_locus_view_setters() {
    let attr = quote! {
        selected_window_title: String = selected_window_title(),
    };
    let item = quote! {
        impl SimpleComponent for Bar {
            type Init = BarInit;
            type Input = sources::Msg;
            type Output = ();

            view! {
                gtk::Window {
                    gtk::Label {
                        #[locus(selected_window_title)]
                        set_label: |title| title.as_str(),

                        #[locus(selected_window_title)]
                        set_css_classes: window_title_classes,
                    }
                }
            }

            fn init(
                init: Self::Init,
                root: Self::Root,
                sender: ComponentSender<Self>,
            ) -> ComponentParts<Self> {
                let model = Bar {
                    title: init.title,
                    sources: sources::Model::default(),
                };
                let widgets = view_output!();
                ComponentParts { model, widgets }
            }
        }
    };

    let expanded = expand_component(attr, item).unwrap();
    let source = expanded.to_string();
    assert!(source.contains("# [track"));
    assert!(source.contains("SelectedWindowTitle"));
    assert!(source.contains("let title = & model . sources . selected_window_title"));
    assert!(source.contains("window_title_classes"));
}

#[test]
fn expands_source_view_setters() {
    let attr = quote! {
        selected_window_title: String = selected_window_title(),
    };
    let item = quote! {
        impl SimpleComponent for Bar {
            type Init = BarInit;
            type Input = sources::Msg;
            type Output = ();

            view! {
                gtk::Window {
                    gtk::Label {
                        #[bind(selected_window_title)]
                        set_label: |title| title.as_str(),
                    }
                }
            }

            fn init(
                init: Self::Init,
                root: Self::Root,
                sender: ComponentSender<Self>,
            ) -> ComponentParts<Self> {
                let model = Bar {
                    title: init.title,
                    sources: sources::Model::default(),
                };
                let widgets = view_output!();
                ComponentParts { model, widgets }
            }
        }
    };

    let expanded = expand_component(attr, item).unwrap();
    let source = expanded.to_string();
    assert!(source.contains("# [track"));
    assert!(source.contains("SelectedWindowTitle"));
    assert!(source.contains("let title = & model . sources . selected_window_title"));
}

#[test]
fn expands_typed_model() {
    let item = quote! {
        pub struct BarLocus {
            #[source(selected_window_title())]
            pub selected_window_title: String,
            #[source(DISPLAY_DEVICE.bind(DisplayDevice::PERCENTAGE))]
            pub battery_percent: f64,
        }
    };

    let expanded = expand_model(TokenStream::new(), item).unwrap();
    let source = expanded.to_string();

    assert!(source.contains("pub struct BarLocus"));
    assert!(source.contains("pub mod sources"));
    assert!(source.contains("pub enum Msg"));
    assert!(source.contains("pub enum Field"));
    assert!(source.contains("SelectedWindowTitle"));
    assert!(source.contains("BatteryPercent"));
    assert!(source.contains("__shell : sources :: Runtime"));
    assert!(source.contains("last_error : :: std :: option :: Option < WatchError >"));
    assert!(source.contains("BoxedSubscriptionSend"));
    assert!(source.contains("subscriptions . push (subscription)"));
    assert!(source.contains("pub fn new () -> Self"));
    assert!(source.contains("impl :: std :: default :: Default for BarLocus"));
    assert!(source.contains("shell_core :: source :: Observable < String"));
    assert!(source.contains("shell_core :: source :: Observable < f64"));
    assert!(source.contains(". on_error"));
    assert!(source.contains(". subscribe"));
}

#[test]
fn expands_typed_model_sources_that_reference_model_fields() {
    let item = quote! {
        pub struct WindowTitleSources {
            pub window: String,
            #[source(window_title(window.clone()))]
            pub title: String,
        }
    };

    let expanded = expand_model(quote!(module = window_title_sources), item).unwrap();
    let source = expanded.to_string();

    assert!(source.contains("pub mod window_title_sources"));
    assert!(source.contains("pub fn new (window : String) -> Self"));
    assert!(!source.contains("impl :: std :: default :: Default for WindowTitleSources"));
    assert!(source.contains("pub fn start < Component > (& self"));
    assert!(source.contains("let window = & self . window"));
    assert!(
        source.contains(
            "let source : :: shell_core :: source :: Observable < String , _ > = window_title (window . clone ())"
        )
    );
}

#[test]
fn expands_typed_model_nested_source_fields() {
    let item = quote! {
        pub struct ProjectLabel {
            pub workspace: String,
            #[source(workspace_name(workspace.clone()))]
            pub workspace_name: String,
            #[model(source = workspace_project(workspace.clone()))]
            pub project: ProjectLabelProject,
        }
    };

    let expanded = expand_model(quote!(module = project_label_sources), item).unwrap();
    let source = expanded.to_string();

    assert!(source.contains("pub enum Msg"));
    assert!(source.contains("WorkspaceName"));
    assert!(source.contains(
        "Project (< ProjectLabelProject as :: shell_core :: model :: SourceModel > :: Msg)"
    ));
    assert!(source.contains(
        "< ProjectLabelProject as :: shell_core :: model :: SourceModel > :: from_default_context"
    ));
    assert!(source.contains("ProjectLabelProject as :: shell_core :: model :: SourceModel"));
    assert!(source.contains("start_source_model"));
    assert!(source.contains("workspace_project (workspace . clone ())"));
    assert!(source.contains("update_source_model"));
    assert!(source.contains("& mut self . project"));
}

#[test]
fn expands_source_model_trait_for_single_context_models() {
    let item = quote! {
        pub struct ProjectLabelProject {
            pub project: Option<String>,
            #[source(project_display_icon(project.clone()))]
            pub icon: Option<String>,
        }
    };

    let expanded = expand_model(quote!(module = project_label_project_sources), item).unwrap();
    let source = expanded.to_string();

    assert!(source.contains("impl :: shell_core :: model :: SourceModel for ProjectLabelProject"));
    assert!(source.contains("type Context = Option < String >"));
    assert!(source.contains("__ShellContext"));
    assert!(source.contains("start_for_source_context"));
    assert!(source.contains("project_display_icon (project . clone ())"));
}

#[test]
fn expands_typed_model_with_unused_local_state() {
    let item = quote! {
        pub struct Bar {
            #[source(DISPLAY_DEVICE.bind(DisplayDevice::PERCENTAGE))]
            pub battery_percent: f64,
            local_title: String,
        }
    };

    let expanded = expand_model(TokenStream::new(), item).unwrap();
    let source = expanded.to_string();

    assert!(source.contains("pub fn new (local_title : String) -> Self"));
    assert!(source.contains("# [allow (unused_variables)] let local_title = & self . local_title"));
}

#[test]
fn expands_component_bind_list() {
    let attr = quote! {
        model = Bar
    };
    let item = quote! {
        impl SimpleComponent for Bar {
            type Init = BarInit;
            type Input = sources::Msg;
            type Output = ();

            view! {
                gtk::Window {
                    #[bind_list(window_nodes, row = WindowTitle)]
                    window_list -> gtk::Box {}

                    gtk::Label {
                        #[bind(battery_percent)]
                        set_label: |percent| percent.to_string(),
                    }
                }
            }

            fn init(
                init: Self::Init,
                root: Self::Root,
                sender: ComponentSender<Self>,
            ) -> ComponentParts<Self> {
                let model = Bar::new();
                let widgets = view_output!();
                ComponentParts { model, widgets }
            }

            fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
                Bar::update(self, msg);
            }
        }
    };

    let expanded = expand_component(attr, item).unwrap();
    let source = expanded.to_string();

    assert!(source.contains("# [name = \"window_list\"]"));
    assert!(source.contains("# [track"));
    assert!(source.contains("model . changed (sources :: Field :: WindowNodes)"));
    assert!(source.contains("set_component_list"));
    assert!(source.contains("\"window_list\""));
    assert!(source.contains(":: shell_core :: list :: ComponentListUpdate"));
    assert!(source.contains("WindowTitle"));
    assert!(source.contains("& model . window_nodes"));
    assert!(source.contains("sources :: Field :: BatteryPercent"));
}

#[test]
fn rejects_explicit_bind_list_backend() {
    let attr = quote! {
        model = Bar
    };
    let item = quote! {
        impl SimpleComponent for Bar {
            type Init = BarInit;
            type Input = sources::Msg;
            type Output = ();

            view! {
                gtk::Window {
                    #[bind_list(window_nodes, backend = factory, row = WindowTitle)]
                    window_list -> gtk::Box {}
                }
            }

            fn init(
                init: Self::Init,
                root: Self::Root,
                sender: ComponentSender<Self>,
            ) -> ComponentParts<Self> {
                let model = Bar::new();
                let widgets = view_output!();
                ComponentParts { model, widgets }
            }
        }
    };

    let error = expand_component(attr, item).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("bind_list infers the backend from the widget type")
    );
}

#[test]
fn expands_legacy_locus_model_sources() {
    let item = quote! {
        pub struct BarLocus {
            #[locus(source = DISPLAY_DEVICE.bind(DisplayDevice::PERCENTAGE))]
            pub battery_percent: f64,
        }
    };

    let expanded = expand_model(TokenStream::new(), item).unwrap();
    let source = expanded.to_string();

    assert!(source.contains("BatteryPercent"));
    assert!(source.contains("shell_core :: source :: Observable < f64"));
}

#[test]
fn expands_model_component_impl() {
    let attr = quote! {
        model = BarLocus,
        state = sources
    };
    let item = quote! {
        impl SimpleComponent for Bar {
            type Init = BarInit;
            type Input = sources::Msg;
            type Output = ();

            view! {
                gtk::Window {
                    gtk::Label {
                    #[bind(selected_window_title)]
                        set_label: |title| title.as_str(),
                    }
                }
            }

            fn init(
                init: Self::Init,
                root: Self::Root,
                sender: ComponentSender<Self>,
            ) -> ComponentParts<Self> {
                let model = Bar {
                    title: init.title,
                    sources: BarLocus::default(),
                };
                let widgets = view_output!();
                ComponentParts { model, widgets }
            }
        }
    };

    let expanded = expand_component(attr, item).unwrap();
    let source = expanded.to_string();

    assert!(!source.contains("mod sources"));
    assert!(source.contains("model . sources . start"));
    assert!(source.contains("model . sources . set_subscriptions"));
    assert!(source.contains("sources :: Field :: SelectedWindowTitle"));
    assert!(source.contains("self . sources . update (msg)"));
    assert!(source.contains("fn update"));
}

#[test]
fn expands_self_model_component_impl() {
    let attr = quote! {
        model = WindowTitle,
        module = window_title_sources
    };
    let item = quote! {
        impl SimpleComponent for WindowTitle {
            type Init = WindowNode;
            type Input = window_title_sources::Msg;
            type Output = ();

            view! {
                gtk::Label {
                    #[bind(title)]
                    set_label: |title| title.as_str(),
                }
            }

            fn init(
                init: Self::Init,
                root: Self::Root,
                sender: ComponentSender<Self>,
            ) -> ComponentParts<Self> {
                let model = WindowTitle::new(init);
                let widgets = view_output!();
                ComponentParts { model, widgets }
            }
        }
    };

    let expanded = expand_component(attr, item).unwrap();
    let source = expanded.to_string();

    assert!(source.contains("let __shell_subscriptions = model . start"));
    assert!(source.contains("model . set_subscriptions"));
    assert!(source.contains("model . changed (window_title_sources :: Field :: Title)"));
    assert!(source.contains("let title = & model . title"));
    assert!(source.contains("WindowTitle :: update (self , msg)"));
    assert!(!source.contains("model . sources"));
}

#[test]
fn expands_async_model_component_impl() {
    let model = quote! {
        pub struct Bar {
            #[source(selected_window_title())]
            pub selected_window_title: String,
        }
    };
    let component = quote! {
        impl SimpleAsyncComponent for Bar {
            type Init = ();
            type Input = sources::Msg;
            type Output = ();

            view! {
                gtk::Window {}
            }

            async fn init(
                init: Self::Init,
                root: Self::Root,
                sender: AsyncComponentSender<Self>,
            ) -> AsyncComponentParts<Self> {
                let model = Bar::new();
                let widgets = view_output!();
                AsyncComponentParts { model, widgets }
            }
        }
    };

    let expanded_model = expand_model(TokenStream::new(), model).unwrap();
    let expanded_component = expand_component(quote!(model = Bar), component).unwrap();
    let source = quote! {
        #expanded_model
        #expanded_component
    }
    .to_string();

    assert!(source.contains("pub fn start_async < Component > (& self"));
    assert!(source.contains(":: relm4 :: AsyncComponentSender < Component >"));
    assert!(source.contains("Component : :: relm4 :: component :: AsyncComponent"));
    assert!(source.contains("model . start_async (sender . clone ())"));
    assert!(source.contains("async fn update"));
}

#[test]
fn expands_model_component_with_wrapped_input() {
    let attr = quote! {
        model = BarLocus,
        state = sources
    };
    let item = quote! {
        impl SimpleComponent for Bar {
            type Init = BarInit;
            type Input = BarMsg;
            type Output = ();

            view! {
                gtk::Window {
                    gtk::Label {
                    #[bind(selected_window_title)]
                        set_label: |title| title.as_str(),
                    }
                }
            }

            fn init(
                init: Self::Init,
                root: Self::Root,
                sender: ComponentSender<Self>,
            ) -> ComponentParts<Self> {
                let model = Bar {
                    title: init.title,
                    sources: BarLocus::default(),
                };
                let widgets = view_output!();
                ComponentParts { model, widgets }
            }

            fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
                match msg {
                    BarMsg::Sources(msg) => self.sources.update(msg),
                    BarMsg::Refresh => {}
                }
            }
        }
    };

    let expanded = expand_component(attr, item).unwrap();
    let source = expanded.to_string();

    assert!(source.contains("model . sources . start"));
    assert!(source.contains("BarMsg :: Sources"));
    assert!(!source.contains("self . sources . update (msg) ;"));
}

#[test]
fn expands_model_start_for_wrapped_component_input() {
    let item = quote! {
        pub struct BarLocus {
            #[source(selected_window_title())]
            pub selected_window_title: String,
        }
    };

    let expanded = expand_model(TokenStream::new(), item).unwrap();
    let source = expanded.to_string();

    assert!(source.contains("< Component as :: relm4 :: Component > :: Input"));
    assert!(source.contains("From < sources :: Msg >"));
    assert!(source.contains("+ Send"));
    assert!(source.contains("sources :: Msg :: SelectedWindowTitle (result) . into"));
}

#[test]
fn rejects_duplicate_binding_fields() {
    let error = component_parse_error(quote! {
        selected_window_title: String = selected_window_title(),
        selected_window_title: String = selected_window_title(),
    });

    assert!(error.to_string().contains("duplicate source binding field"));
}

#[test]
fn rejects_duplicate_generated_variants() {
    let error = component_parse_error(quote! {
        selected_window_title: String = selected_window_title(),
        selected__window_title: String = selected_window_title(),
    });

    assert!(
        error
            .to_string()
            .contains("source binding fields must generate unique message variants")
    );
}

#[test]
fn rejects_too_many_bindings_for_dirty_mask() {
    let bindings = (0..129).map(|index| {
        let field = format_ident!("field_{index}");
        quote! {
            #field: String = selected_window_title(),
        }
    });
    let error = component_parse_error(quote! {
        #(#bindings)*
    });

    assert!(
        error
            .to_string()
            .contains("source models support at most 128 bindings")
    );
}

#[test]
fn accepts_parenthesized_binding_expr() {
    let config = parse2::<BindingsConfig>(quote! {
        component = Bar,
        message = BarMsg::Locus,
        selected_window_title: String = (selected_window_title()),
    })
    .unwrap();
    let expected = quote! {
        selected_window_title()
    };

    let expr = &config.bindings[0].source;
    assert_eq!(quote!(#expr).to_string(), expected.to_string());
}

#[test]
fn treats_sources_as_generic_source_expressions() {
    let config = parse2::<ComponentConfig>(quote! {
        battery_percent: f64 = BATTERY.bind(Battery::PERCENTAGE),
    })
    .unwrap();

    let source = &config.bindings[0].source;
    assert_eq!(
        quote!(#source).to_string(),
        quote!(BATTERY.bind(Battery::PERCENTAGE)).to_string()
    );
}

fn component_parse_error(tokens: TokenStream) -> syn::Error {
    match parse2::<ComponentConfig>(tokens) {
        Ok(_) => panic!("expected component config parse error"),
        Err(error) => error,
    }
}

fn component_item() -> TokenStream {
    quote! {
        impl SimpleComponent for Bar {
            type Init = BarInit;
            type Input = sources::Msg;
            type Output = ();

            view! {
                gtk::Window {}
            }

            fn init(
                init: Self::Init,
                root: Self::Root,
                sender: ComponentSender<Self>,
            ) -> ComponentParts<Self> {
                let model = Bar {
                    title: init.title,
                    sources: sources::Model::default(),
                };
                let widgets = view_output!();
                ComponentParts { model, widgets }
            }
        }
    }
}
