use proc_macro2::{Delimiter, Group, Ident, TokenStream, TokenTree};
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{Expr, ImplItem, ItemImpl, LitStr, Pat, Result, Token, parse2};

use super::config::BindingConfig;

pub(super) enum ViewBindings<'a> {
    Known(&'a [BindingConfig]),
    Model,
}

#[derive(Clone)]
pub(super) enum StateAccess {
    Field(Ident),
    Model,
}

enum ViewBinding {
    Known { field: Ident, variant: Ident },
    Model { field: Ident, variant: Ident },
}

impl ViewBinding {
    const fn field(&self) -> &Ident {
        match self {
            Self::Known { field, .. } | Self::Model { field, .. } => field,
        }
    }

    const fn variant(&self) -> &Ident {
        match self {
            Self::Known { variant, .. } | Self::Model { variant, .. } => variant,
        }
    }
}

struct BindListAttr {
    source: Ident,
    row: Option<syn::Path>,
}

impl Parse for BindListAttr {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let source = input.parse::<Ident>()?;
        let mut row = None;

        while input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }

            let ident = input.parse::<Ident>()?;
            input.parse::<Token![=]>()?;
            let value = input.parse::<syn::Path>()?;
            match ident.to_string().as_str() {
                "backend" => {
                    return Err(syn::Error::new_spanned(
                        value,
                        "bind_list infers the backend from the widget type; explicit backend selection is not supported yet",
                    ));
                }
                "row" => row = Some(value),
                _ => {
                    return Err(syn::Error::new_spanned(ident, "expected row"));
                }
            }
        }

        Ok(Self { source, row })
    }
}

pub(super) fn transform_locus_view_attributes(
    item_impl: &mut ItemImpl,
    module_ident: &Ident,
    state: &StateAccess,
    bindings: ViewBindings<'_>,
) -> Result<()> {
    for item in &mut item_impl.items {
        let ImplItem::Macro(item_macro) = item else {
            continue;
        };
        if item_macro.mac.path.is_ident("view") {
            item_macro.mac.tokens = transform_tokens(
                item_macro.mac.tokens.clone(),
                module_ident,
                state,
                &bindings,
            )?;
        }
    }
    Ok(())
}

fn transform_tokens(
    tokens: TokenStream,
    module_ident: &Ident,
    state: &StateAccess,
    bindings: &ViewBindings<'_>,
) -> Result<TokenStream> {
    let mut output = Vec::new();
    let mut iter = tokens.into_iter().peekable();

    while let Some(token) = iter.next() {
        if let Some(list_attr) = bind_list_attr(&token, iter.peek())? {
            iter.next();
            let binding = view_binding(list_attr.source.clone(), bindings)?;
            let row = list_attr.row.expect("row is validated by bind_list_attr");
            append_bound_list_widget(&mut output, &mut iter, module_ident, state, binding, row)?;
            continue;
        }

        if let Some(field) = binding_attr_field(&token, iter.peek())? {
            iter.next();
            let binding = view_binding(field, bindings)?;
            append_locus_tracked_setter(&mut output, &mut iter, module_ident, state, binding)?;
            continue;
        }

        output.push(transform_token(token, module_ident, state, bindings)?);
    }

    Ok(output.into_iter().collect())
}

fn transform_token(
    token: TokenTree,
    module_ident: &Ident,
    state: &StateAccess,
    bindings: &ViewBindings<'_>,
) -> Result<TokenTree> {
    let TokenTree::Group(group) = token else {
        return Ok(token);
    };
    let mut transformed = Group::new(
        group.delimiter(),
        transform_tokens(group.stream(), module_ident, state, bindings)?,
    );
    transformed.set_span(group.span());
    Ok(TokenTree::Group(transformed))
}

fn view_binding(field: Ident, bindings: &ViewBindings<'_>) -> Result<ViewBinding> {
    match bindings {
        ViewBindings::Known(bindings) => {
            let binding = bindings
                .iter()
                .find(|binding| binding.field == field)
                .ok_or_else(|| {
                    syn::Error::new_spanned(field, "unknown source field in view attribute")
                })?;
            Ok(ViewBinding::Known {
                field: binding.field.clone(),
                variant: binding.variant.clone(),
            })
        }
        ViewBindings::Model => {
            let variant = format_ident!("{}", super::config::upper_camel(&field.to_string()));
            Ok(ViewBinding::Model { field, variant })
        }
    }
}

fn binding_attr_field(current: &TokenTree, next: Option<&TokenTree>) -> Result<Option<Ident>> {
    let TokenTree::Punct(punct) = current else {
        return Ok(None);
    };
    if punct.as_char() != '#' {
        return Ok(None);
    }

    let Some(TokenTree::Group(group)) = next else {
        return Ok(None);
    };
    if group.delimiter() != Delimiter::Bracket {
        return Ok(None);
    }

    let mut attr_tokens = group.stream().into_iter();
    let Some(TokenTree::Ident(attr_name)) = attr_tokens.next() else {
        return Ok(None);
    };
    let attr = attr_name.to_string();
    if attr != "locus" && attr != "bind" {
        return Ok(None);
    }
    let expected = format!("#[{}(field)]", attr);
    let Some(TokenTree::Group(args)) = attr_tokens.next() else {
        return Err(syn::Error::new_spanned(attr_name, expected));
    };
    if args.delimiter() != Delimiter::Parenthesis {
        return Err(syn::Error::new_spanned(attr_name, expected));
    }
    let mut args = args.stream().into_iter();
    let Some(TokenTree::Ident(field)) = args.next() else {
        return Err(syn::Error::new_spanned(attr_name, expected));
    };
    if args.next().is_some() || attr_tokens.next().is_some() {
        return Err(syn::Error::new_spanned(
            field,
            "expected exactly one source field",
        ));
    }
    Ok(Some(field))
}

fn bind_list_attr(current: &TokenTree, next: Option<&TokenTree>) -> Result<Option<BindListAttr>> {
    let TokenTree::Punct(punct) = current else {
        return Ok(None);
    };
    if punct.as_char() != '#' {
        return Ok(None);
    }

    let Some(TokenTree::Group(group)) = next else {
        return Ok(None);
    };
    if group.delimiter() != Delimiter::Bracket {
        return Ok(None);
    }

    let mut attr_tokens = group.stream().into_iter();
    let Some(TokenTree::Ident(attr_name)) = attr_tokens.next() else {
        return Ok(None);
    };
    if attr_name != "bind_list" {
        return Ok(None);
    }

    let Some(TokenTree::Group(args)) = attr_tokens.next() else {
        return Err(syn::Error::new_spanned(
            attr_name,
            "#[bind_list(field, row = RowComponent)]",
        ));
    };
    if args.delimiter() != Delimiter::Parenthesis {
        return Err(syn::Error::new_spanned(
            attr_name,
            "#[bind_list(field, row = RowComponent)]",
        ));
    }
    if attr_tokens.next().is_some() {
        return Err(syn::Error::new_spanned(
            attr_name,
            "unexpected tokens after bind_list arguments",
        ));
    }

    let attr = parse2::<BindListAttr>(args.stream())?;
    if attr.row.is_none() {
        return Err(syn::Error::new_spanned(
            attr.source,
            "bind_list requires row = Component",
        ));
    }
    Ok(Some(attr))
}

fn append_locus_tracked_setter(
    output: &mut Vec<TokenTree>,
    iter: &mut std::iter::Peekable<impl Iterator<Item = TokenTree>>,
    module_ident: &Ident,
    state: &StateAccess,
    binding: ViewBinding,
) -> Result<()> {
    let mut setter_tokens = Vec::new();

    loop {
        let Some(token) = iter.next() else {
            return Err(syn::Error::new_spanned(
                binding.field(),
                "expected setter after source binding attribute",
            ));
        };
        let is_colon = matches!(&token, TokenTree::Punct(punct) if punct.as_char() == ':');
        setter_tokens.push(transform_token(
            token,
            module_ident,
            state,
            &ViewBindings::Known(&[]),
        )?);
        if is_colon {
            break;
        }
    }

    let mut adapter_tokens = Vec::new();
    let mut depth = 0usize;
    for token in iter.by_ref() {
        let is_top_level_comma =
            depth == 0 && matches!(&token, TokenTree::Punct(punct) if punct.as_char() == ',');
        if is_top_level_comma {
            break;
        }
        match &token {
            TokenTree::Group(_) => adapter_tokens.push(transform_token(
                token,
                module_ident,
                state,
                &ViewBindings::Known(&[]),
            )?),
            TokenTree::Punct(punct) if matches!(punct.as_char(), '(' | '[' | '{') => {
                depth += 1;
                adapter_tokens.push(token);
            }
            TokenTree::Punct(punct) if matches!(punct.as_char(), ')' | ']' | '}') && depth > 0 => {
                depth -= 1;
                adapter_tokens.push(token);
            }
            _ => adapter_tokens.push(token),
        }
    }

    let adapter: Expr = parse2(adapter_tokens.into_iter().collect())?;
    let field = binding.field();
    let value_expr = locus_setter_value_expr(adapter, state, field)?;
    output.extend(track_attribute(&binding, module_ident, state));
    output.extend(setter_tokens);
    output.extend(quote! { #value_expr, });
    Ok(())
}

fn append_bound_list_widget(
    output: &mut Vec<TokenTree>,
    iter: &mut std::iter::Peekable<impl Iterator<Item = TokenTree>>,
    module_ident: &Ident,
    state: &StateAccess,
    binding: ViewBinding,
    row: syn::Path,
) -> Result<()> {
    let mut widget_tokens = Vec::new();

    for token in iter.by_ref() {
        match token {
            TokenTree::Group(group) if group.delimiter() == Delimiter::Brace => {
                normalize_bound_list_widget_prefix(&mut widget_tokens, binding.field())?;
                let injected = bound_list_setter(module_ident, state, &binding, row)?;
                let body = transform_tokens(
                    group.stream(),
                    module_ident,
                    state,
                    &ViewBindings::Known(&[]),
                )?;
                let mut transformed = Group::new(
                    Delimiter::Brace,
                    quote! {
                        #injected
                        #body
                    },
                );
                transformed.set_span(group.span());
                widget_tokens.push(TokenTree::Group(transformed));
                output.extend(widget_tokens);
                return Ok(());
            }
            token => widget_tokens.push(transform_token(
                token,
                module_ident,
                state,
                &ViewBindings::Known(&[]),
            )?),
        }
    }

    Err(syn::Error::new_spanned(
        binding.field(),
        "bind_list must be placed directly before a widget body",
    ))
}

fn normalize_bound_list_widget_prefix(
    widget_tokens: &mut Vec<TokenTree>,
    source: &Ident,
) -> Result<()> {
    let Some(TokenTree::Ident(view)) = widget_tokens.first() else {
        return Err(syn::Error::new_spanned(
            source,
            "bind_list must be placed directly before a named widget",
        ));
    };
    let has_arrow = matches!(widget_tokens.get(1), Some(TokenTree::Punct(punct)) if punct.as_char() == '-')
        && matches!(widget_tokens.get(2), Some(TokenTree::Punct(punct)) if punct.as_char() == '>');
    if !has_arrow {
        return Err(syn::Error::new_spanned(
            source,
            "bind_list widget must use `name -> Widget` syntax",
        ));
    }

    let name = LitStr::new(&view.to_string(), view.span());
    widget_tokens.drain(0..3);
    let prefix = quote! {
        #[name = #name]
    };
    let mut normalized = prefix.into_iter().collect::<Vec<_>>();
    normalized.append(widget_tokens);
    *widget_tokens = normalized;

    Ok(())
}

fn bound_list_setter(
    module_ident: &Ident,
    state: &StateAccess,
    binding: &ViewBinding,
    row: syn::Path,
) -> Result<TokenStream> {
    let field = binding.field();
    let variant = binding.variant();
    let changed = match state {
        StateAccess::Field(state_ident) => quote! {
            model.#state_ident.changed(#module_ident::Field::#variant)
        },
        StateAccess::Model => quote! {
            model.changed(#module_ident::Field::#variant)
        },
    };
    let field_access = match state {
        StateAccess::Field(state_ident) => quote! {
            model.#state_ident.#field
        },
        StateAccess::Model => quote! {
            model.#field
        },
    };

    Ok(quote! {
        #[track(#changed)]
        set_component_list: ::shell_core::list::ComponentListUpdate::<#row>::new(&#field_access),
    })
}

fn track_attribute(
    binding: &ViewBinding,
    module_ident: &Ident,
    state: &StateAccess,
) -> TokenStream {
    match binding {
        ViewBinding::Known { variant, .. } | ViewBinding::Model { variant, .. } => {
            let binding_variant = variant;
            let changed = match state {
                StateAccess::Field(state_ident) => quote! {
                    model.#state_ident.changed(#module_ident::Field::#binding_variant)
                },
                StateAccess::Model => quote! {
                    model.changed(#module_ident::Field::#binding_variant)
                },
            };

            quote! {
                #[track(#changed)]
            }
        }
    }
}

fn locus_setter_value_expr(
    adapter: Expr,
    state: &StateAccess,
    field: &Ident,
) -> Result<TokenStream> {
    let field_access = match state {
        StateAccess::Field(state_ident) => quote! {
            model.#state_ident.#field
        },
        StateAccess::Model => quote! {
            model.#field
        },
    };

    let Expr::Closure(closure) = adapter else {
        return Ok(quote! {
            (#adapter)(&#field_access)
        });
    };

    if closure.inputs.len() != 1 {
        return Err(syn::Error::new_spanned(
            closure.or1_token,
            "source setter closures must accept exactly one field value",
        ));
    }

    let input = closure
        .inputs
        .first()
        .expect("closure input exists")
        .clone();
    validate_locus_value_pat(&input)?;
    let body = closure.body;

    Ok(quote! {
        {
            let #input = &#field_access;
            #body
        }
    })
}

fn validate_locus_value_pat(input: &Pat) -> Result<()> {
    if matches!(input, Pat::Type(_)) {
        return Ok(());
    }

    let Pat::Ident(_) = input else {
        return Err(syn::Error::new_spanned(
            input,
            "source setter closure parameters must be identifiers or typed patterns",
        ));
    };

    Ok(())
}
