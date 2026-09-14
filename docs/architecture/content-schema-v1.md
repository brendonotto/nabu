# Content schema version 1

Nabu's first rich-content schema intentionally covers prose without media. The browser sends the
canonical Tiptap/ProseMirror JSON document; Axum validates it and generates the stored HTML
projection. Client-generated HTML is never accepted.

## Allowed nodes

- `doc`
- `paragraph`
- `heading` at levels 2 and 3
- `bulletList` and `orderedList`
- `listItem`
- `blockquote`
- `codeBlock`
- `horizontalRule`
- `hardBreak`
- `text`

## Allowed marks

- `bold`
- `italic`
- `strike`
- `underline`
- `code`
- `link`

Links require an absolute `http` or `https` URL. Rendered links receive `rel="nofollow noopener
noreferrer"`. No arbitrary attributes, inline styles, raw HTML, scripts, embeds, or media nodes are
accepted. Images, galleries, and short videos will extend a later schema version in M6 after the
storage and media-reference boundaries exist.

## Limits and compatibility

The existing 250,000-character post limit remains in force and is measured from text content.
Documents must have one `doc` root and a maximum nesting depth of 12. Version-1 documents created
by M2 already consist of `doc`, `paragraph`, and `text` nodes and remain valid without migration.
