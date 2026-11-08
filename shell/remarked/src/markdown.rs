//! Theme-native Markdown rendering for the note preview.
//!
//! Most note reads are previews. This renders the parser AST into ordinary GTK
//! labels and containers so the preview inherits shell colors, typography,
//! wrapping, and selection behavior without an HTML or WebKit surface.

use gtk::{glib, pango, prelude::*};

/// Rebuild `container` with a rendered view of `source`.
pub(crate) fn render(container: &gtk::Box, source: &str) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }

    match markdown::to_mdast(source, &markdown::ParseOptions::gfm()) {
        Ok(markdown::mdast::Node::Root(root)) => render_children(container, &root.children),
        Ok(node) => render_block(container, &node),
        Err(_) => container.append(&paragraph(source)),
    }
}

fn render_children(container: &gtk::Box, children: &[markdown::mdast::Node]) {
    for child in children {
        render_block(container, child);
    }
}

fn render_block(container: &gtk::Box, node: &markdown::mdast::Node) {
    use markdown::mdast::Node;

    match node {
        Node::Paragraph(paragraph_node) => {
            container.append(&paragraph(&inline_markup(&paragraph_node.children)))
        }
        Node::Heading(heading) => {
            let label = label(&format!(
                "<span weight=\"bold\" size=\"{}\">{}</span>",
                heading_size(heading.depth),
                inline_markup(&heading.children)
            ));
            label.add_css_class("remarked-markdown-heading");
            container.append(&label);
        }
        Node::Blockquote(blockquote) => {
            let quote = gtk::Box::new(gtk::Orientation::Vertical, 8);
            quote.add_css_class("remarked-markdown-quote");
            render_children(&quote, &blockquote.children);
            container.append(&quote);
        }
        Node::List(list) => container.append(&render_list(list)),
        Node::Code(code) => {
            let frame = gtk::Box::new(gtk::Orientation::Vertical, 0);
            frame.add_css_class("remarked-markdown-code-block");
            let code = label(&format!(
                "<tt>{}</tt>",
                glib::markup_escape_text(code.value.trim_end())
            ));
            code.add_css_class("remarked-markdown-code");
            code.set_selectable(true);
            frame.append(&code);
            container.append(&frame);
        }
        Node::ThematicBreak(_) => {
            let separator = gtk::Separator::new(gtk::Orientation::Horizontal);
            separator.add_css_class("remarked-markdown-rule");
            container.append(&separator);
        }
        Node::Table(table) => container.append(&render_table(table)),
        Node::Html(html) => container.append(&paragraph(&glib::markup_escape_text(&html.value))),
        Node::Definition(_) => {}
        Node::ListItem(_) | Node::TableRow(_) | Node::TableCell(_) => {}
        other => {
            let markup = inline_markup(std::slice::from_ref(other));
            if !markup.is_empty() {
                container.append(&paragraph(&markup));
            }
        }
    }
}

fn render_list(list: &markdown::mdast::List) -> gtk::Box {
    let list_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
    list_box.add_css_class("remarked-markdown-list");
    let start = list.start.unwrap_or(1);
    for (index, child) in list.children.iter().enumerate() {
        let markdown::mdast::Node::ListItem(item) = child else {
            continue;
        };
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let marker = if let Some(checked) = item.checked {
            if checked { "☑" } else { "☐" }.to_string()
        } else if list.ordered {
            format!("{}.", start + index as u32)
        } else {
            "•".to_string()
        };
        let marker = gtk::Label::new(Some(&marker));
        marker.set_xalign(0.0);
        marker.set_valign(gtk::Align::Start);
        marker.add_css_class("remarked-markdown-marker");
        row.append(&marker);

        let body = gtk::Box::new(gtk::Orientation::Vertical, 4);
        body.set_hexpand(true);
        render_children(&body, &item.children);
        row.append(&body);
        list_box.append(&row);
    }
    list_box
}

fn render_table(table: &markdown::mdast::Table) -> gtk::Grid {
    let grid = gtk::Grid::builder()
        .column_spacing(18)
        .row_spacing(8)
        .build();
    grid.add_css_class("remarked-markdown-table");
    for (row_index, row) in table.children.iter().enumerate() {
        let markdown::mdast::Node::TableRow(row) = row else {
            continue;
        };
        for (column_index, cell) in row.children.iter().enumerate() {
            let markdown::mdast::Node::TableCell(cell) = cell else {
                continue;
            };
            let markup = if row_index == 0 {
                format!("<b>{}</b>", inline_markup(&cell.children))
            } else {
                inline_markup(&cell.children)
            };
            grid.attach(&label(&markup), column_index as i32, row_index as i32, 1, 1);
        }
    }
    grid
}

fn paragraph(markup: &str) -> gtk::Label {
    let label = label(markup);
    label.add_css_class("remarked-markdown-paragraph");
    label
}

fn label(markup: &str) -> gtk::Label {
    let label = gtk::Label::new(None);
    label.set_markup(markup);
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.set_wrap_mode(pango::WrapMode::WordChar);
    label.set_selectable(true);
    label
}

fn heading_size(depth: u8) -> &'static str {
    match depth {
        1 => "x-large",
        2 => "large",
        3 => "medium",
        _ => "normal",
    }
}

fn inline_markup(children: &[markdown::mdast::Node]) -> String {
    use markdown::mdast::Node;

    let mut out = String::new();
    for child in children {
        match child {
            Node::Text(text) => out.push_str(&glib::markup_escape_text(&text.value)),
            Node::InlineCode(code) => {
                out.push_str("<tt>");
                out.push_str(&glib::markup_escape_text(&code.value));
                out.push_str("</tt>");
            }
            Node::Strong(strong) => {
                out.push_str("<b>");
                out.push_str(&inline_markup(&strong.children));
                out.push_str("</b>");
            }
            Node::Emphasis(emphasis) => {
                out.push_str("<i>");
                out.push_str(&inline_markup(&emphasis.children));
                out.push_str("</i>");
            }
            Node::Delete(delete) => {
                out.push_str("<s>");
                out.push_str(&inline_markup(&delete.children));
                out.push_str("</s>");
            }
            Node::Link(link) => {
                let text = inline_markup(&link.children);
                if link.url.is_empty() {
                    out.push_str(&text);
                } else {
                    out.push_str(&format!(
                        "<a href=\"{}\">{}</a>",
                        glib::markup_escape_text(&link.url),
                        if text.is_empty() {
                            glib::markup_escape_text(&link.url).to_string()
                        } else {
                            text
                        }
                    ));
                }
            }
            Node::LinkReference(link) => out.push_str(&inline_markup(&link.children)),
            Node::Break(_) => out.push('\n'),
            Node::Image(image) => out.push_str(&glib::markup_escape_text(&image.alt)),
            Node::FootnoteReference(reference) => {
                out.push_str(&format!(
                    "[^{}]",
                    glib::markup_escape_text(&reference.identifier)
                ));
            }
            Node::Html(html) => out.push_str(&glib::markup_escape_text(&html.value)),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod test;
