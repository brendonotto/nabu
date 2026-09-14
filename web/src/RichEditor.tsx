import Placeholder from '@tiptap/extension-placeholder'
import { EditorContent, useEditor } from '@tiptap/react'
import StarterKit from '@tiptap/starter-kit'
import { useEffect } from 'react'
import type { JSONContent } from '@tiptap/react'

type Props = {
  content: JSONContent
  revision: number
  onChange: (content: JSONContent) => void
}

function RichEditor({ content, revision, onChange }: Props) {
  const editor = useEditor({
    extensions: [
      StarterKit.configure({
        heading: { levels: [2, 3] },
        link: { openOnClick: false, protocols: ['http', 'https'] },
      }),
      Placeholder.configure({ placeholder: 'Begin wherever you are…' }),
    ],
    content,
    immediatelyRender: false,
    editorProps: {
      attributes: {
        class: 'rich-editor-content',
        'aria-label': 'Post content',
      },
    },
    onUpdate: ({ editor: updated }) => onChange(updated.getJSON()),
  })

  useEffect(() => {
    if (editor && JSON.stringify(editor.getJSON()) !== JSON.stringify(content)) {
      editor.commands.setContent(content)
    }
  }, [content, editor, revision])

  if (!editor) return <p className="loading">Preparing editor…</p>

  function setLink() {
    const previous = editor?.getAttributes('link').href as string | undefined
    const href = window.prompt('Link URL', previous ?? 'https://')
    if (href === null || !editor) return
    if (href === '') {
      editor.chain().focus().extendMarkRange('link').unsetLink().run()
      return
    }
    editor.chain().focus().extendMarkRange('link').setLink({ href }).run()
  }

  return (
    <div className="rich-editor">
      <div className="editor-toolbar" role="toolbar" aria-label="Text formatting">
        <button type="button" className={editor.isActive('bold') ? 'active' : ''} onClick={() => editor.chain().focus().toggleBold().run()} aria-label="Bold"><strong>B</strong></button>
        <button type="button" className={editor.isActive('italic') ? 'active' : ''} onClick={() => editor.chain().focus().toggleItalic().run()} aria-label="Italic"><em>I</em></button>
        <button type="button" className={editor.isActive('underline') ? 'active' : ''} onClick={() => editor.chain().focus().toggleUnderline().run()} aria-label="Underline"><u>U</u></button>
        <button type="button" className={editor.isActive('heading', { level: 2 }) ? 'active' : ''} onClick={() => editor.chain().focus().toggleHeading({ level: 2 }).run()}>Heading</button>
        <button type="button" className={editor.isActive('bulletList') ? 'active' : ''} onClick={() => editor.chain().focus().toggleBulletList().run()}>Bullets</button>
        <button type="button" className={editor.isActive('orderedList') ? 'active' : ''} onClick={() => editor.chain().focus().toggleOrderedList().run()}>Numbers</button>
        <button type="button" className={editor.isActive('blockquote') ? 'active' : ''} onClick={() => editor.chain().focus().toggleBlockquote().run()}>Quote</button>
        <button type="button" className={editor.isActive('link') ? 'active' : ''} onClick={setLink}>Link</button>
      </div>
      <EditorContent editor={editor} />
    </div>
  )
}

export default RichEditor
