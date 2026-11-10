use proc_macro2::Ident;
use quote::format_ident;
use syn::parse::{Parse, ParseStream};
use syn::{
    Expr, Fields, ItemStruct, Path, Result, Token, Type, parenthesized, punctuated::Punctuated,
};

pub(super) struct BindingsConfig {
    pub(super) component: Path,
    pub(super) message: Path,
    pub(super) bindings: Vec<BindingConfig>,
}

pub(super) struct ComponentConfig {
    pub(super) module: Ident,
    pub(super) state: Ident,
    pub(super) model: Option<Type>,
    pub(super) bindings: Vec<BindingConfig>,
}

pub(super) struct ModelConfig {
    pub(super) module: Ident,
}

pub(super) struct BindingConfig {
    pub(super) field: Ident,
    pub(super) variant: Ident,
    pub(super) ty: Type,
    pub(super) source: Expr,
}

pub(super) struct NestedModelConfig {
    pub(super) field: Ident,
    pub(super) variant: Ident,
    pub(super) ty: Type,
    pub(super) source: Expr,
}

pub(super) struct ModelBindings {
    pub(super) sources: Vec<BindingConfig>,
    pub(super) nested_models: Vec<NestedModelConfig>,
}

impl Parse for BindingsConfig {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let mut component = None;
        let mut message = None;
        let mut bindings = Vec::new();
        let entries = Punctuated::<ConfigEntry, Token![,]>::parse_terminated(input)?;

        for entry in entries {
            match entry {
                ConfigEntry::Component(path) => component = Some(path),
                ConfigEntry::Message(path) => message = Some(path),
                ConfigEntry::Binding(binding) => bindings.push(binding),
                ConfigEntry::Module(ident) => {
                    return Err(syn::Error::new_spanned(
                        ident,
                        "module is only supported by #[shell_macros::component]",
                    ));
                }
                ConfigEntry::State(ident) => {
                    return Err(syn::Error::new_spanned(
                        ident,
                        "state is only supported by #[shell_macros::component]",
                    ));
                }
                ConfigEntry::Model(ty) => {
                    return Err(syn::Error::new_spanned(
                        ty,
                        "model is only supported by #[shell_macros::component]",
                    ));
                }
            }
        }

        let component = component.ok_or_else(|| input.error("missing component = Type"))?;
        let message = message.ok_or_else(|| input.error("missing message = Enum::Variant"))?;
        if bindings.is_empty() {
            return Err(input.error("expected at least one source binding"));
        }
        validate_bindings(&bindings)?;

        Ok(Self {
            component,
            message,
            bindings,
        })
    }
}

impl Parse for ComponentConfig {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let mut module = None;
        let mut state = None;
        let mut model = None;
        let mut bindings = Vec::new();
        let entries = Punctuated::<ConfigEntry, Token![,]>::parse_terminated(input)?;

        for entry in entries {
            match entry {
                ConfigEntry::Module(ident) => module = Some(ident),
                ConfigEntry::State(ident) => state = Some(ident),
                ConfigEntry::Binding(binding) => bindings.push(binding),
                ConfigEntry::Model(ty) => model = Some(ty),
                ConfigEntry::Component(path) => {
                    return Err(syn::Error::new_spanned(
                        path,
                        "component is inferred from the annotated impl",
                    ));
                }
                ConfigEntry::Message(path) => {
                    return Err(syn::Error::new_spanned(
                        path,
                        "message is inferred from the component Input type",
                    ));
                }
            }
        }

        if model.is_some() && !bindings.is_empty() {
            return Err(input.error(
                "model = Type components read bindings from #[shell_macros::model] fields",
            ));
        }

        if bindings.is_empty() {
            if model.is_none() {
                return Err(input.error("expected model = Type or at least one source binding"));
            }
        } else {
            validate_bindings(&bindings)?;
        }

        let module = module.unwrap_or_else(|| format_ident!("sources"));
        let state = state.unwrap_or_else(|| module.clone());

        Ok(Self {
            module,
            state,
            model,
            bindings,
        })
    }
}

impl Parse for ModelConfig {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        if input.is_empty() {
            return Ok(Self {
                module: format_ident!("sources"),
            });
        }

        let mut module = None;
        let entries = Punctuated::<ConfigEntry, Token![,]>::parse_terminated(input)?;
        for entry in entries {
            match entry {
                ConfigEntry::Module(ident) => module = Some(ident),
                ConfigEntry::State(ident) => {
                    return Err(syn::Error::new_spanned(
                        ident,
                        "state is only supported by #[shell_macros::component]",
                    ));
                }
                ConfigEntry::Binding(binding) => {
                    return Err(syn::Error::new_spanned(
                        binding.field,
                        "typed model bindings belong on struct fields",
                    ));
                }
                ConfigEntry::Model(ty) => {
                    return Err(syn::Error::new_spanned(
                        ty,
                        "model is inferred from the annotated struct",
                    ));
                }
                ConfigEntry::Component(path) | ConfigEntry::Message(path) => {
                    return Err(syn::Error::new_spanned(
                        path,
                        "only module = ident is supported by #[shell_macros::model]",
                    ));
                }
            }
        }

        Ok(Self {
            module: module.unwrap_or_else(|| format_ident!("sources")),
        })
    }
}

enum ConfigEntry {
    Component(Path),
    Message(Path),
    Module(Ident),
    State(Ident),
    Model(Type),
    Binding(BindingConfig),
}

impl Parse for ConfigEntry {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let ident = input.parse::<Ident>()?;
        if input.peek(Token![=]) {
            input.parse::<Token![=]>()?;
            if ident == "model" {
                return Ok(Self::Model(input.parse()?));
            }
            let path = input.parse::<Path>()?;
            return match ident.to_string().as_str() {
                "component" => Ok(Self::Component(path)),
                "message" => Ok(Self::Message(path)),
                "module" => {
                    path.get_ident().cloned().map(Self::Module).ok_or_else(|| {
                        syn::Error::new_spanned(path, "module must be an identifier")
                    })
                }
                "state" => path
                    .get_ident()
                    .cloned()
                    .map(Self::State)
                    .ok_or_else(|| syn::Error::new_spanned(path, "state must be an identifier")),
                "model" => unreachable!("model is parsed before path entries"),
                _ => Err(syn::Error::new_spanned(
                    ident,
                    "expected component, message, module, state, or a typed binding",
                )),
            };
        }

        input.parse::<Token![:]>()?;
        let ty = input.parse::<Type>()?;
        input.parse::<Token![=]>()?;
        let source = parse_binding_expr(input)?;
        let variant = format_ident!("{}", upper_camel(&ident.to_string()));

        Ok(Self::Binding(BindingConfig {
            field: ident,
            variant,
            ty,
            source,
        }))
    }
}

pub(super) fn model_bindings(item: &ItemStruct) -> Result<ModelBindings> {
    let Fields::Named(fields) = &item.fields else {
        return Err(syn::Error::new_spanned(
            item,
            "source models must use named fields",
        ));
    };

    let mut bindings = Vec::new();
    let mut nested_models = Vec::new();
    for field in &fields.named {
        let field_ident = field.ident.clone().expect("named field");
        if let Some(source) = nested_model_source(field)? {
            let variant = format_ident!("{}", upper_camel(&field_ident.to_string()));
            nested_models.push(NestedModelConfig {
                field: field_ident,
                variant,
                ty: field.ty.clone(),
                source,
            });
            continue;
        }

        let Some(source) = provider_source(field)? else {
            continue;
        };
        let variant = format_ident!("{}", upper_camel(&field_ident.to_string()));
        bindings.push(BindingConfig {
            field: field_ident,
            variant,
            ty: field.ty.clone(),
            source,
        });
    }

    if bindings.is_empty() && nested_models.is_empty() {
        return Err(syn::Error::new_spanned(
            item,
            "source models require at least one #[source(...)], #[locus(source = ...)], or #[model(source = ...)] field",
        ));
    }

    validate_bindings(&bindings)?;
    validate_nested_models(&nested_models)?;
    Ok(ModelBindings {
        sources: bindings,
        nested_models,
    })
}

fn nested_model_source(field: &syn::Field) -> Result<Option<Expr>> {
    for attr in &field.attrs {
        if !attr.path().is_ident("model") {
            continue;
        }

        let mut source = None;
        attr.parse_nested_meta(|meta| {
            if !meta.path.is_ident("source") {
                return Err(meta.error("expected source = ..."));
            }
            meta.input.parse::<Token![=]>()?;
            source = Some(parse_binding_expr(meta.input)?);
            Ok(())
        })?;
        return Ok(source);
    }

    Ok(None)
}

fn provider_source(field: &syn::Field) -> Result<Option<Expr>> {
    for attr in &field.attrs {
        if attr.path().is_ident("source") {
            return Ok(Some(attr.parse_args()?));
        }

        if attr.path().is_ident("locus") {
            let mut source = None;
            attr.parse_nested_meta(|meta| {
                if !meta.path.is_ident("source") {
                    return Err(meta.error("expected source = ..."));
                }
                meta.input.parse::<Token![=]>()?;
                source = Some(parse_binding_expr(meta.input)?);
                Ok(())
            })?;
            return Ok(source);
        }
    }

    Ok(None)
}

fn validate_nested_models(nested_models: &[NestedModelConfig]) -> Result<()> {
    let mut fields = std::collections::HashSet::new();
    let mut variants = std::collections::HashSet::new();

    for nested in nested_models {
        if !fields.insert(nested.field.to_string()) {
            return Err(syn::Error::new_spanned(
                &nested.field,
                "duplicate nested model field",
            ));
        }
        if !variants.insert(nested.variant.to_string()) {
            return Err(syn::Error::new_spanned(
                &nested.field,
                "nested model fields must generate unique message variants",
            ));
        }
    }

    Ok(())
}

fn parse_binding_expr(input: ParseStream<'_>) -> Result<Expr> {
    if input.peek(syn::token::Paren) {
        let content;
        parenthesized!(content in input);
        return content.parse();
    }
    input.parse()
}

fn validate_bindings(bindings: &[BindingConfig]) -> Result<()> {
    if bindings.len() > 128 {
        let field = bindings
            .last()
            .map(|binding| &binding.field)
            .expect("bindings is not empty");
        return Err(syn::Error::new_spanned(
            field,
            "source models support at most 128 bindings; split the model when it grows beyond that",
        ));
    }

    let mut fields = std::collections::HashSet::new();
    let mut variants = std::collections::HashSet::new();

    for binding in bindings {
        if !fields.insert(binding.field.to_string()) {
            return Err(syn::Error::new_spanned(
                &binding.field,
                "duplicate source binding field",
            ));
        }
        if !variants.insert(binding.variant.to_string()) {
            return Err(syn::Error::new_spanned(
                &binding.field,
                "source binding fields must generate unique message variants",
            ));
        }
    }

    Ok(())
}

pub(super) fn upper_camel(value: &str) -> String {
    let mut out = String::new();
    for segment in value.split('_').filter(|segment| !segment.is_empty()) {
        let mut chars = segment.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.extend(chars);
        }
    }
    out
}
