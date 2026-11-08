use super::inline_markup;
use markdown::mdast::Node;

#[test]
fn escapes_text_and_renders_inline_styles() {
    let children = vec![
        Node::Text(markdown::mdast::Text {
            value: "<hello>".into(),
            position: None,
        }),
        Node::Strong(markdown::mdast::Strong {
            children: vec![Node::Text(markdown::mdast::Text {
                value: "bold".into(),
                position: None,
            })],
            position: None,
        }),
    ];
    assert_eq!(inline_markup(&children), "&lt;hello&gt;<b>bold</b>");
}

#[test]
fn renders_links_with_escaped_urls() {
    let link = Node::Link(markdown::mdast::Link {
        children: vec![Node::Text(markdown::mdast::Text {
            value: "docs".into(),
            position: None,
        })],
        position: None,
        url: "https://example.test/?a=1&b=2".into(),
        title: None,
    });
    assert_eq!(
        inline_markup(&[link]),
        "<a href=\"https://example.test/?a=1&amp;b=2\">docs</a>"
    );
}
