use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;
use url::Url;

const MAX_DEPTH: usize = 12;
const MAX_TEXT_CHARS: usize = 250_000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Node {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    attrs: BTreeMap<String, Value>,
    #[serde(default)]
    content: Vec<Node>,
    #[serde(default)]
    marks: Vec<Mark>,
    text: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Mark {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    attrs: BTreeMap<String, Value>,
}

pub(crate) struct ValidatedContent {
    pub document: Value,
    pub rendered_html: String,
}

pub(crate) fn validate_and_render(document: Value) -> Result<ValidatedContent, &'static str> {
    let root: Node = serde_json::from_value(document.clone()).map_err(|_| "invalid structure")?;
    let mut text_chars = 0;
    validate_node(&root, Parent::Root, 0, &mut text_chars)?;
    if text_chars > MAX_TEXT_CHARS {
        return Err("content exceeds 250,000 characters");
    }

    let mut rendered_html = String::new();
    render_node(&root, &mut rendered_html);
    Ok(ValidatedContent {
        document,
        rendered_html,
    })
}

#[derive(Clone, Copy)]
enum Parent {
    Root,
    Block,
    List,
    ListItem,
    Inline,
    Code,
}

fn validate_node(
    node: &Node,
    parent: Parent,
    depth: usize,
    text_chars: &mut usize,
) -> Result<(), &'static str> {
    if depth > MAX_DEPTH {
        return Err("content nesting exceeds 12 levels");
    }
    validate_placement(&node.kind, parent)?;

    match node.kind.as_str() {
        "doc" => {
            require_empty_attrs(node)?;
            require_no_text_or_marks(node)?;
            for child in &node.content {
                validate_node(child, Parent::Block, depth + 1, text_chars)?;
            }
        }
        "paragraph" => {
            require_empty_attrs(node)?;
            require_no_text_or_marks(node)?;
            validate_children(node, Parent::Inline, depth, text_chars)?;
        }
        "heading" => {
            require_no_text_or_marks(node)?;
            if node.attrs.len() != 1
                || !matches!(node.attrs.get("level").and_then(Value::as_u64), Some(2 | 3))
            {
                return Err("headings must have level 2 or 3");
            }
            validate_children(node, Parent::Inline, depth, text_chars)?;
        }
        "bulletList" => {
            require_empty_attrs(node)?;
            require_no_text_or_marks(node)?;
            validate_children(node, Parent::List, depth, text_chars)?;
        }
        "orderedList" => {
            require_no_text_or_marks(node)?;
            if node
                .attrs
                .keys()
                .any(|key| !matches!(key.as_str(), "start" | "type"))
                || node
                    .attrs
                    .get("start")
                    .is_some_and(|start| start.as_u64().is_none())
                || node.attrs.get("type").is_some_and(|kind| !kind.is_null())
            {
                return Err("ordered lists have an invalid start value");
            }
            validate_children(node, Parent::List, depth, text_chars)?;
        }
        "listItem" => {
            require_empty_attrs(node)?;
            require_no_text_or_marks(node)?;
            if node.content.is_empty() {
                return Err("list items cannot be empty");
            }
            validate_children(node, Parent::ListItem, depth, text_chars)?;
        }
        "blockquote" => {
            require_empty_attrs(node)?;
            require_no_text_or_marks(node)?;
            validate_children(node, Parent::Block, depth, text_chars)?;
        }
        "codeBlock" => {
            require_no_text_or_marks(node)?;
            if !node.attrs.is_empty()
                && (node.attrs.len() != 1 || !node.attrs.contains_key("language"))
            {
                return Err("code blocks have invalid attributes");
            }
            validate_children(node, Parent::Code, depth, text_chars)?;
        }
        "horizontalRule" | "hardBreak" => {
            require_empty_attrs(node)?;
            require_no_text_or_marks(node)?;
            if !node.content.is_empty() {
                return Err("leaf nodes cannot have content");
            }
        }
        "text" => {
            require_empty_attrs(node)?;
            if !node.content.is_empty() {
                return Err("text nodes cannot have content");
            }
            if matches!(parent, Parent::Code) && !node.marks.is_empty() {
                return Err("code block text cannot have marks");
            }
            let text = node.text.as_deref().ok_or("text nodes require text")?;
            *text_chars = text_chars.saturating_add(text.chars().count());
            for mark in &node.marks {
                validate_mark(mark)?;
            }
        }
        _ => return Err("content contains an unsupported node"),
    }
    Ok(())
}

fn validate_placement(kind: &str, parent: Parent) -> Result<(), &'static str> {
    let valid = match parent {
        Parent::Root => kind == "doc",
        Parent::Block => matches!(
            kind,
            "paragraph"
                | "heading"
                | "bulletList"
                | "orderedList"
                | "blockquote"
                | "codeBlock"
                | "horizontalRule"
        ),
        Parent::List => kind == "listItem",
        Parent::ListItem => matches!(kind, "paragraph" | "bulletList" | "orderedList"),
        Parent::Inline => matches!(kind, "text" | "hardBreak"),
        Parent::Code => kind == "text",
    };
    if valid {
        Ok(())
    } else {
        Err("content contains a node in an invalid position")
    }
}

fn validate_children(
    node: &Node,
    parent: Parent,
    depth: usize,
    text_chars: &mut usize,
) -> Result<(), &'static str> {
    for child in &node.content {
        validate_node(child, parent, depth + 1, text_chars)?;
    }
    Ok(())
}

fn validate_mark(mark: &Mark) -> Result<(), &'static str> {
    match mark.kind.as_str() {
        "bold" | "italic" | "strike" | "underline" | "code" if mark.attrs.is_empty() => Ok(()),
        "link" => {
            if mark
                .attrs
                .keys()
                .any(|key| !matches!(key.as_str(), "href" | "target" | "rel" | "class" | "title"))
            {
                return Err("links have unsupported attributes");
            }
            let href = mark
                .attrs
                .get("href")
                .and_then(Value::as_str)
                .ok_or("links require an href")?;
            let url = Url::parse(href).map_err(|_| "links require an absolute URL")?;
            if !matches!(url.scheme(), "http" | "https") {
                return Err("links require an http or https URL");
            }
            Ok(())
        }
        _ => Err("content contains an unsupported mark"),
    }
}

fn require_empty_attrs(node: &Node) -> Result<(), &'static str> {
    if node.attrs.is_empty() {
        Ok(())
    } else {
        Err("node has unsupported attributes")
    }
}

fn require_no_text_or_marks(node: &Node) -> Result<(), &'static str> {
    if node.text.is_none() && node.marks.is_empty() {
        Ok(())
    } else {
        Err("non-text nodes cannot contain text or marks")
    }
}

fn render_node(node: &Node, output: &mut String) {
    match node.kind.as_str() {
        "doc" => render_children(node, output),
        "paragraph" => render_wrapped(node, output, "p", None),
        "heading" => {
            let tag = if node.attrs.get("level").and_then(Value::as_u64) == Some(2) {
                "h2"
            } else {
                "h3"
            };
            render_wrapped(node, output, tag, None);
        }
        "bulletList" => render_wrapped(node, output, "ul", None),
        "orderedList" => {
            let attribute = node
                .attrs
                .get("start")
                .and_then(Value::as_u64)
                .filter(|start| *start != 1)
                .map(|start| format!(" start=\"{start}\""));
            render_wrapped(node, output, "ol", attribute.as_deref());
        }
        "listItem" => render_wrapped(node, output, "li", None),
        "blockquote" => render_wrapped(node, output, "blockquote", None),
        "codeBlock" => {
            output.push_str("<pre><code>");
            render_children(node, output);
            output.push_str("</code></pre>");
        }
        "horizontalRule" => output.push_str("<hr>"),
        "hardBreak" => output.push_str("<br>"),
        "text" => render_text(node, output),
        _ => unreachable!("validated nodes are rendered"),
    }
}

fn render_children(node: &Node, output: &mut String) {
    for child in &node.content {
        render_node(child, output);
    }
}

fn render_wrapped(node: &Node, output: &mut String, tag: &str, attribute: Option<&str>) {
    output.push('<');
    output.push_str(tag);
    if let Some(attribute) = attribute {
        output.push_str(attribute);
    }
    output.push('>');
    render_children(node, output);
    output.push_str("</");
    output.push_str(tag);
    output.push('>');
}

fn render_text(node: &Node, output: &mut String) {
    for mark in &node.marks {
        match mark.kind.as_str() {
            "bold" => output.push_str("<strong>"),
            "italic" => output.push_str("<em>"),
            "strike" => output.push_str("<s>"),
            "underline" => output.push_str("<u>"),
            "code" => output.push_str("<code>"),
            "link" => {
                let href = mark
                    .attrs
                    .get("href")
                    .and_then(Value::as_str)
                    .expect("validated link");
                output.push_str("<a href=\"");
                output.push_str(&html_escape::encode_double_quoted_attribute(href));
                output.push_str("\" rel=\"nofollow noopener noreferrer\">");
            }
            _ => unreachable!("validated marks are rendered"),
        }
    }
    output.push_str(&html_escape::encode_text(
        node.text.as_deref().unwrap_or_default(),
    ));
    for mark in node.marks.iter().rev() {
        output.push_str(match mark.kind.as_str() {
            "bold" => "</strong>",
            "italic" => "</em>",
            "strike" => "</s>",
            "underline" => "</u>",
            "code" => "</code>",
            "link" => "</a>",
            _ => unreachable!("validated marks are rendered"),
        });
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::validate_and_render;

    #[test]
    fn renders_supported_content_and_escapes_text_and_links() {
        let content = validate_and_render(json!({
            "type": "doc",
            "content": [
                {"type": "heading", "attrs": {"level": 2}, "content": [
                    {"type": "text", "text": "A <safe> heading", "marks": [{"type": "bold"}]}
                ]},
                {"type": "paragraph", "content": [
                    {"type": "text", "text": "Read more", "marks": [{
                        "type": "link", "attrs": {"href": "https://example.com/?a=\"b\""}
                    }]}
                ]}
            ]
        }))
        .unwrap();

        assert_eq!(
            content.rendered_html,
            "<h2><strong>A &lt;safe&gt; heading</strong></h2><p><a href=\"https://example.com/?a=&quot;b&quot;\" rel=\"nofollow noopener noreferrer\">Read more</a></p>"
        );
    }

    #[test]
    fn rejects_script_nodes_unsafe_links_and_unknown_attributes() {
        for document in [
            json!({"type": "doc", "content": [{"type": "script", "text": "alert(1)"}]}),
            json!({"type": "doc", "content": [{"type": "paragraph", "content": [{
                "type": "text", "text": "bad", "marks": [{"type": "link", "attrs": {"href": "javascript:alert(1)"}}]
            }]}]}),
            json!({"type": "doc", "onclick": "alert(1)"}),
        ] {
            assert!(validate_and_render(document).is_err());
        }
    }
}
