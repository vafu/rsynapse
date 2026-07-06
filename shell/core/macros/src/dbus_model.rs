use proc_macro2::{Ident, TokenStream};
use quote::{ToTokens, format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{
    Attribute, Fields, GenericArgument, Item, ItemStruct, ItemTrait, LitStr, Meta, Path, Result,
    ReturnType, Token, TraitItem, Type, parse_quote, parse2, punctuated::Punctuated,
};

pub fn expand(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let config = parse2::<DbusModelConfig>(attr)?;
    match parse2::<Item>(item)? {
        Item::Trait(item_trait) => expand_trait(config, item_trait),
        Item::Struct(item_struct) => expand_struct(config, item_struct),
        item => Err(syn::Error::new_spanned(
            item,
            "dbus_model supports trait or struct declarations",
        )),
    }
}

fn expand_trait(config: DbusModelConfig, mut item_trait: ItemTrait) -> Result<TokenStream> {
    let proxy = proxy_config(&config, &item_trait.attrs, &item_trait)?;
    let properties = trait_properties(&item_trait)?;
    rewrite_property_returns(&mut item_trait);
    let bus = config.bus.unwrap_or(Bus::Session).tokens();
    let module = config
        .module
        .unwrap_or_else(|| format_ident!("{}_dbus", to_snake_case(&item_trait.ident.to_string())));
    let generated = generated_model(module, bus, &proxy, properties);

    Ok(quote! {
        #item_trait
        #generated
    })
}

fn expand_struct(config: DbusModelConfig, item_struct: ItemStruct) -> Result<TokenStream> {
    let proxy = proxy_config(&config, &item_struct.attrs, &item_struct)?;
    let properties = struct_properties(&item_struct)?;
    let proxy_trait = proxy_trait_from_struct(&item_struct, &proxy, &properties);
    let bus = config.bus.unwrap_or(Bus::Session).tokens();
    let module = config
        .module
        .unwrap_or_else(|| format_ident!("{}_dbus", to_snake_case(&item_struct.ident.to_string())));
    let type_alias = type_alias_from_struct(&item_struct, &module);
    let generated = generated_model(module, bus, &proxy, properties);

    Ok(quote! {
        #proxy_trait
        #generated
        #type_alias
    })
}

fn generated_model(
    module: Ident,
    bus: TokenStream,
    proxy: &ProxyConfig,
    properties: Vec<Property>,
) -> TokenStream {
    let service = &proxy.default_service;
    let interface = &proxy.interface;
    let default_constructor = proxy.default_path.as_ref().map(|path| {
        quote! {
            pub fn new() -> Self {
                Self::at(
                    ::zbus::zvariant::OwnedObjectPath::try_from(#path.to_owned())
                        .expect("generated default D-Bus object path should be valid"),
                )
            }
        }
    });

    let methods = properties.iter().map(|property| {
        let output = property.output_type();
        let dbus_name_ref = &property.dbus_name;
        let source = property.source_expression(quote!(self.property(#dbus_name_ref)));
        let method = &property.method;

        quote! {
            pub fn #method(&self) -> ::shell_core::source::Source<#output> {
                #source
            }
        }
    });

    let generated = quote! {
        mod #module {
            #[derive(Clone, Debug, Eq, PartialEq)]
            pub struct Model {
                path: ::zbus::zvariant::OwnedObjectPath,
            }

            impl Model {
                #default_constructor

                pub fn at(path: ::zbus::zvariant::OwnedObjectPath) -> Self {
                    Self { path }
                }

                #(#methods)*

                fn property(&self, property: &'static str) -> ::shell_core::source::dbus::PropertyDescriptor {
                    let object = ::shell_core::source::dbus::ObjectDescriptor::parse(
                        #bus,
                        #service,
                        self.path.as_str(),
                        #interface,
                    )
                    .expect("generated D-Bus descriptor should be valid");
                    ::shell_core::source::dbus::PropertyDescriptor::new(object, property)
                }
            }
        }
    };

    generated
}

struct DbusModelConfig {
    module: Option<Ident>,
    bus: Option<Bus>,
    interface: Option<LitStr>,
    default_service: Option<LitStr>,
    default_path: Option<LitStr>,
}

impl Parse for DbusModelConfig {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let mut module = None;
        let mut bus = None;
        let mut interface = None;
        let mut default_service = None;
        let mut default_path = None;
        let entries = Punctuated::<ConfigEntry, Token![,]>::parse_terminated(input)?;

        for entry in entries {
            match entry {
                ConfigEntry::Module(value) => module = Some(value),
                ConfigEntry::Bus(value) => bus = Some(value),
                ConfigEntry::Interface(value) => interface = Some(value),
                ConfigEntry::DefaultService(value) => default_service = Some(value),
                ConfigEntry::DefaultPath(value) => default_path = Some(value),
            }
        }

        Ok(Self {
            module,
            bus,
            interface,
            default_service,
            default_path,
        })
    }
}

enum ConfigEntry {
    Module(Ident),
    Bus(Bus),
    Interface(LitStr),
    DefaultService(LitStr),
    DefaultPath(LitStr),
}

impl Parse for ConfigEntry {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let ident = input.parse::<Ident>()?;
        input.parse::<Token![=]>()?;
        match ident.to_string().as_str() {
            "module" => Ok(Self::Module(input.parse()?)),
            "bus" => Ok(Self::Bus(input.parse()?)),
            "interface" => Ok(Self::Interface(input.parse()?)),
            "default_service" => Ok(Self::DefaultService(input.parse()?)),
            "default_path" => Ok(Self::DefaultPath(input.parse()?)),
            _ => Err(syn::Error::new_spanned(
                ident,
                "expected module, bus, interface, default_service, or default_path",
            )),
        }
    }
}

#[derive(Clone, Copy)]
enum Bus {
    Session,
    System,
}

impl Bus {
    fn tokens(self) -> TokenStream {
        match self {
            Self::Session => quote!(::shell_core::source::dbus::Bus::Session),
            Self::System => quote!(::shell_core::source::dbus::Bus::System),
        }
    }
}

impl Parse for Bus {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        if input.peek(LitStr) {
            return parse_bus_name(&input.parse::<LitStr>()?.value(), input);
        }
        let ident = input.parse::<Ident>()?;
        parse_bus_name(&ident.to_string(), input)
    }
}

fn parse_bus_name(value: &str, input: ParseStream<'_>) -> Result<Bus> {
    match value {
        "session" | "Session" => Ok(Bus::Session),
        "system" | "System" => Ok(Bus::System),
        _ => Err(input.error("bus must be session or system")),
    }
}

struct ProxyConfig {
    interface: LitStr,
    default_service: LitStr,
    default_path: Option<LitStr>,
}

fn proxy_config(
    config: &DbusModelConfig,
    attrs: &[Attribute],
    error_span: &impl ToTokens,
) -> Result<ProxyConfig> {
    if config.interface.is_some()
        || config.default_service.is_some()
        || config.default_path.is_some()
    {
        return Ok(ProxyConfig {
            interface: config.interface.clone().ok_or_else(|| {
                syn::Error::new_spanned(error_span, "dbus_model requires interface = ...")
            })?,
            default_service: config.default_service.clone().ok_or_else(|| {
                syn::Error::new_spanned(error_span, "dbus_model requires default_service = ...")
            })?,
            default_path: config.default_path.clone(),
        });
    }

    proxy_config_from_attrs(attrs, error_span)
}

fn proxy_config_from_attrs(attrs: &[Attribute], error_span: &impl ToTokens) -> Result<ProxyConfig> {
    for attr in attrs {
        if !attr_path_ends_with(attr, "proxy") {
            continue;
        }

        let entries =
            attr.parse_args_with(Punctuated::<ProxyEntry, Token![,]>::parse_terminated)?;
        let mut interface = None;
        let mut default_service = None;
        let mut default_path = None;

        for entry in entries {
            match entry {
                ProxyEntry::Interface(value) => interface = Some(value),
                ProxyEntry::DefaultService(value) => default_service = Some(value),
                ProxyEntry::DefaultPath(value) => default_path = Some(value),
                ProxyEntry::Other => {}
            }
        }

        return Ok(ProxyConfig {
            interface: interface.ok_or_else(|| {
                syn::Error::new_spanned(attr, "dbus_model requires proxy interface = ...")
            })?,
            default_service: default_service.ok_or_else(|| {
                syn::Error::new_spanned(attr, "dbus_model requires proxy default_service = ...")
            })?,
            default_path,
        });
    }

    Err(syn::Error::new_spanned(
        error_span,
        "dbus_model requires interface/default_service or a stacked zbus #[proxy(...)] attribute",
    ))
}

enum ProxyEntry {
    Interface(LitStr),
    DefaultService(LitStr),
    DefaultPath(LitStr),
    Other,
}

impl Parse for ProxyEntry {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let ident = input.parse::<Ident>()?;
        input.parse::<Token![=]>()?;
        let value = input.parse::<LitStr>()?;
        match ident.to_string().as_str() {
            "interface" => Ok(Self::Interface(value)),
            "default_service" => Ok(Self::DefaultService(value)),
            "default_path" => Ok(Self::DefaultPath(value)),
            _ => Ok(Self::Other),
        }
    }
}

struct Property {
    method: Ident,
    dbus_name: String,
    source: PropertySource,
}

impl Property {
    fn output_type(&self) -> TokenStream {
        match &self.source {
            PropertySource::Required(ty) => quote! {
                #ty
            },
            PropertySource::Optional(ty) => quote! {
                ::std::option::Option<#ty>
            },
            PropertySource::Model(ty) => {
                let ty = model_type_in_generated_module(ty);
                quote! {
                    #ty
                }
            }
            PropertySource::OptionalModel(ty) => {
                let ty = model_type_in_generated_module(ty);
                quote! {
                    ::std::option::Option<#ty>
                }
            }
            PropertySource::ModelVec(ty) => {
                let ty = model_type_in_generated_module(ty);
                quote! {
                    Vec<#ty>
                }
            }
        }
    }

    fn proxy_type(&self) -> TokenStream {
        self.source.proxy_type()
    }

    fn source_expression(&self, descriptor: TokenStream) -> TokenStream {
        match &self.source {
            PropertySource::Required(ty) => quote! {
                ::shell_core::source::dbus::required_property_source::<#ty>(#descriptor)
            },
            PropertySource::Optional(ty) => quote! {
                ::shell_core::source::dbus::optional_property_source::<#ty>(#descriptor)
            },
            PropertySource::Model(ty) => {
                let ty = model_type_in_generated_module(ty);
                quote! {
                    ::shell_core::source::dbus::required_property_source::<::zbus::zvariant::OwnedObjectPath>(#descriptor)
                        .map(#ty::at)
                }
            }
            PropertySource::OptionalModel(ty) => {
                let ty = model_type_in_generated_module(ty);
                quote! {
                    ::shell_core::source::dbus::optional_property_source::<::zbus::zvariant::OwnedObjectPath>(#descriptor)
                        .map(|path| path.map(#ty::at))
                }
            }
            PropertySource::ModelVec(ty) => {
                let ty = model_type_in_generated_module(ty);
                quote! {
                    ::shell_core::source::dbus::required_property_source::<Vec<::zbus::zvariant::OwnedObjectPath>>(#descriptor)
                        .map(|paths| paths.into_iter().map(#ty::at).collect())
                }
            }
        }
    }
}

enum PropertySource {
    Required(Type),
    Optional(Type),
    Model(Type),
    OptionalModel(Type),
    ModelVec(Type),
}

impl PropertySource {
    fn proxy_type(&self) -> TokenStream {
        match self {
            Self::Required(ty) => quote! {
                #ty
            },
            Self::Optional(ty) => quote! {
                Vec<#ty>
            },
            Self::Model(_) => quote! {
                ::zbus::zvariant::OwnedObjectPath
            },
            Self::OptionalModel(_) => quote! {
                Vec<::zbus::zvariant::OwnedObjectPath>
            },
            Self::ModelVec(_) => quote! {
                Vec<::zbus::zvariant::OwnedObjectPath>
            },
        }
    }
}

fn trait_properties(item_trait: &ItemTrait) -> Result<Vec<Property>> {
    let mut properties = Vec::new();
    for item in &item_trait.items {
        let TraitItem::Fn(function) = item else {
            continue;
        };
        if !has_zbus_property_attr(&function.attrs) {
            continue;
        }

        properties.push(Property {
            method: function.sig.ident.clone(),
            dbus_name: property_name(&function.sig.ident.to_string()),
            source: property_source(
                logical_return_type(&function.sig.output).ok_or_else(|| {
                    syn::Error::new_spanned(
                        &function.sig.output,
                        "dbus_model property methods must return a value type",
                    )
                })?,
                false,
            ),
        });
    }
    Ok(properties)
}

fn struct_properties(item_struct: &ItemStruct) -> Result<Vec<Property>> {
    let Fields::Named(fields) = &item_struct.fields else {
        return Err(syn::Error::new_spanned(
            &item_struct.fields,
            "dbus_model structs must use named fields",
        ));
    };

    fields
        .named
        .iter()
        .map(|field| {
            let ident = field
                .ident
                .clone()
                .ok_or_else(|| syn::Error::new_spanned(field, "dbus_model fields must be named"))?;
            Ok(Property {
                dbus_name: property_name(&ident.to_string()),
                method: ident,
                source: property_source(field.ty.clone(), has_dbus_model_attr(&field.attrs)),
            })
        })
        .collect()
}

fn type_alias_from_struct(item_struct: &ItemStruct, module: &Ident) -> TokenStream {
    let vis = &item_struct.vis;
    let ident = &item_struct.ident;

    quote! {
        #vis type #ident = #module::Model;
    }
}

fn proxy_trait_from_struct(
    item_struct: &ItemStruct,
    proxy: &ProxyConfig,
    properties: &[Property],
) -> TokenStream {
    let ident = format_ident!("{}DbusProxy", item_struct.ident);
    let interface = &proxy.interface;
    let default_service = &proxy.default_service;
    let default_path = proxy
        .default_path
        .as_ref()
        .map(|path| quote!(, default_path = #path));
    let methods = properties.iter().map(|property| {
        let method = &property.method;
        let ty = property.proxy_type();

        quote! {
            #[zbus(property)]
            fn #method(&self) -> ::zbus::Result<#ty>;
        }
    });

    quote! {
        #[zbus::proxy(
            interface = #interface,
            default_service = #default_service
            #default_path
        )]
        trait #ident {
            #(#methods)*
        }
    }
}

fn rewrite_property_returns(item_trait: &mut ItemTrait) {
    for item in &mut item_trait.items {
        let TraitItem::Fn(function) = item else {
            continue;
        };
        if !has_zbus_property_attr(&function.attrs) {
            continue;
        }

        let Some(logical_type) = logical_return_type(&function.sig.output) else {
            continue;
        };
        let proxy_type = property_source(logical_type, false).proxy_type();

        function.sig.output = parse_quote!(-> ::zbus::Result<#proxy_type>);
    }
}

fn property_source(ty: Type, model_ref: bool) -> PropertySource {
    if model_ref {
        return model_property_source(ty);
    }

    option_inner_type(&ty)
        .map(PropertySource::Optional)
        .unwrap_or(PropertySource::Required(ty))
}

fn model_property_source(ty: Type) -> PropertySource {
    if let Some(inner) = option_inner_type(&ty) {
        return PropertySource::OptionalModel(inner);
    }

    if let Some(inner) = vec_inner_type(&ty) {
        return PropertySource::ModelVec(inner);
    }

    PropertySource::Model(ty)
}

fn model_type_in_generated_module(ty: &Type) -> TokenStream {
    quote!(super::#ty)
}

fn option_inner_type(ty: &Type) -> Option<Type> {
    let Type::Path(path) = ty else {
        return None;
    };
    let segment = path.path.segments.last()?;
    if segment.ident != "Option" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    let GenericArgument::Type(ty) = arguments.args.first()? else {
        return None;
    };
    Some(ty.clone())
}

fn vec_inner_type(ty: &Type) -> Option<Type> {
    let Type::Path(path) = ty else {
        return None;
    };
    let segment = path.path.segments.last()?;
    if segment.ident != "Vec" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    let GenericArgument::Type(ty) = arguments.args.first()? else {
        return None;
    };
    Some(ty.clone())
}

fn result_inner_type(output: &ReturnType) -> Option<Type> {
    let ReturnType::Type(_, ty) = output else {
        return None;
    };
    let Type::Path(path) = ty.as_ref() else {
        return None;
    };
    let segment = path.path.segments.last()?;
    if segment.ident != "Result" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    let GenericArgument::Type(ty) = arguments.args.first()? else {
        return None;
    };
    Some(ty.clone())
}

fn logical_return_type(output: &ReturnType) -> Option<Type> {
    if let Some(inner) = result_inner_type(output) {
        return Some(inner);
    }

    let ReturnType::Type(_, ty) = output else {
        return None;
    };
    Some(ty.as_ref().clone())
}

fn has_zbus_property_attr(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if !attr_path_ends_with(attr, "zbus") {
            return false;
        }
        attr.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
            .map(|entries| {
                entries.iter().any(|entry| match entry {
                    Meta::Path(path) => path.is_ident("property"),
                    _ => false,
                })
            })
            .unwrap_or(false)
    })
}

fn has_dbus_model_attr(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if !attr_path_ends_with(attr, "dbus") {
            return false;
        }
        attr.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
            .map(|entries| {
                entries.iter().any(|entry| match entry {
                    Meta::Path(path) => path.is_ident("model"),
                    _ => false,
                })
            })
            .unwrap_or(false)
    })
}

fn attr_path_ends_with(attr: &Attribute, ident: &str) -> bool {
    path_ends_with(attr.path(), ident)
}

fn path_ends_with(path: &Path, ident: &str) -> bool {
    path.segments
        .last()
        .is_some_and(|segment| segment.ident == ident)
}

fn property_name(method: &str) -> String {
    method
        .split('_')
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let mut chars = segment.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<String>()
}

fn to_snake_case(value: &str) -> String {
    let mut output = String::new();
    for (index, character) in value.chars().enumerate() {
        if character.is_uppercase() {
            if index > 0 {
                output.push('_');
            }
            output.extend(character.to_lowercase());
        } else {
            output.push(character);
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use quote::quote;

    use super::expand;

    #[test]
    fn expands_property_sources_from_zbus_proxy_trait() {
        let expanded = expand(
            quote!(module = root_sources),
            quote! {
                #[proxy(
                    interface = "org.rsynapse.Niri1",
                    default_service = "org.rsynapse.Niri",
                    default_path = "/org/rsynapse/Niri"
                )]
                trait NiriRoot {
                    #[zbus(property)]
                    fn windows(&self) -> Vec<OwnedObjectPath>;
                }
            },
        )
        .unwrap()
        .to_string();

        assert!(expanded.contains("mod root_sources"));
        assert!(expanded.contains("pub fn new"));
        assert!(expanded.contains("pub fn windows"));
        assert!(expanded.contains("required_property_source :: < Vec < OwnedObjectPath > >"));
        assert!(
            expanded
                .contains("fn windows (& self) -> :: zbus :: Result < Vec < OwnedObjectPath > >")
        );
        assert!(expanded.contains("\"Windows\""));
    }

    #[test]
    fn expands_dynamic_path_model_without_default_constructor() {
        let expanded = expand(
            quote!(module = window_sources),
            quote! {
                #[proxy(
                    interface = "org.rsynapse.Niri1.Window",
                    default_service = "org.rsynapse.Niri"
                )]
                trait NiriWindow {
                    #[zbus(property)]
                    fn app_id(&self) -> Option<String>;
                }
            },
        )
        .unwrap()
        .to_string();

        assert!(expanded.contains("mod window_sources"));
        assert!(expanded.contains("pub fn at"));
        assert!(expanded.contains("pub fn app_id"));
        assert!(expanded.contains("Source < :: std :: option :: Option < String > >"));
        assert!(expanded.contains("optional_property_source :: < String >"));
        assert!(expanded.contains("fn app_id (& self) -> :: zbus :: Result < Vec < String > >"));
        assert!(expanded.contains("\"AppId\""));
        assert!(!expanded.contains("required_property_source :: < Option < String > >"));
        assert!(!expanded.contains("pub fn new"));
    }

    #[test]
    fn accepts_explicit_result_return_for_zbus_style_traits() {
        let expanded = expand(
            quote!(module = window_sources),
            quote! {
                #[proxy(
                    interface = "org.rsynapse.Niri1.Window",
                    default_service = "org.rsynapse.Niri"
                )]
                trait NiriWindow {
                    #[zbus(property)]
                    fn focused(&self) -> zbus::Result<bool>;
                }
            },
        )
        .unwrap()
        .to_string();

        assert!(expanded.contains("pub fn focused"));
        assert!(expanded.contains("Source < bool >"));
        assert!(expanded.contains("required_property_source :: < bool >"));
        assert!(expanded.contains("fn focused (& self) -> :: zbus :: Result < bool >"));
    }

    #[test]
    fn expands_struct_fields_into_hidden_zbus_proxy_trait() {
        let expanded = expand(
            quote!(
                module = window_sources,
                interface = "org.rsynapse.Niri1.Window",
                default_service = "org.rsynapse.Niri"
            ),
            quote! {
                struct NiriWindow {
                    id: u64,
                    title: Option<String>,
                    app_id: Option<String>,
                    focused: bool,
                }
            },
        )
        .unwrap()
        .to_string();

        assert!(expanded.contains("trait NiriWindowDbusProxy"));
        assert!(expanded.contains("type NiriWindow = window_sources :: Model"));
        assert!(expanded.contains("# [zbus :: proxy"));
        assert!(expanded.contains("fn id (& self) -> :: zbus :: Result < u64 >"));
        assert!(expanded.contains("fn title (& self) -> :: zbus :: Result < Vec < String > >"));
        assert!(expanded.contains("fn app_id (& self) -> :: zbus :: Result < Vec < String > >"));
        assert!(expanded.contains("pub fn id"));
        assert!(expanded.contains("Source < u64 >"));
        assert!(expanded.contains("pub fn title"));
        assert!(expanded.contains("Source < :: std :: option :: Option < String > >"));
        assert!(expanded.contains("optional_property_source :: < String >"));
        assert!(expanded.contains("\"AppId\""));
    }

    #[test]
    fn maps_model_reference_fields_from_object_paths() {
        let expanded = expand(
            quote!(
                module = root_sources,
                interface = "org.rsynapse.Niri1",
                default_service = "org.rsynapse.Niri",
                default_path = "/org/rsynapse/Niri"
            ),
            quote! {
                struct NiriRoot {
                    #[dbus(model)]
                    windows: Vec<NiriWindow>,
                }
            },
        )
        .unwrap()
        .to_string();

        assert!(expanded.contains("fn windows (& self) -> :: zbus :: Result < Vec < :: zbus :: zvariant :: OwnedObjectPath > >"));
        assert!(expanded.contains("pub fn windows"));
        assert!(expanded.contains("Source < Vec < super :: NiriWindow > >"));
        assert!(expanded.contains("required_property_source"));
        assert!(expanded.contains("Vec < :: zbus :: zvariant :: OwnedObjectPath >"));
        assert!(expanded.contains(
            "map (| paths | paths . into_iter () . map (super :: NiriWindow :: at) . collect ())"
        ));
    }
}
