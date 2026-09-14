import { useEffect, useRef, useState } from 'react'
import type { FormEvent } from 'react'
import type { JSONContent } from '@tiptap/react'
import './App.css'
import RichEditor from './RichEditor'

type Account = {
  id: string
  email: string
  display_name?: string
}

type Blog = {
  id: string
  slug: string
  title: string
}

type Session = {
  authenticated: boolean
  account?: Account
  blog?: Blog
  csrf_token?: string
}

type Challenge = {
  challenge_id: string
  expires_in_seconds: number
}

type PostSummary = {
  id: string
  slug: string
  title: string
  summary?: string
  revision: number
  updated_at: string
}

type Post = PostSummary & {
  content_json: JSONContent
  created_at: string
}

type ApiError = {
  error?: {
    code?: string
    message?: string
    current_revision?: number
  }
}

class ApiRequestError extends Error {
  code?: string
  currentRevision?: number

  constructor(body: ApiError) {
    super(body.error?.message ?? 'Something went wrong. Try again.')
    this.code = body.error?.code
    this.currentRevision = body.error?.current_revision
  }
}

async function api<T>(path: string, options?: RequestInit): Promise<T> {
  const headers = new Headers(options?.headers)
  if (options?.body) headers.set('Content-Type', 'application/json')
  const response = await fetch(`/api/v1${path}`, {
    credentials: 'same-origin',
    ...options,
    headers,
  })
  if (!response.ok) {
    const body = (await response.json().catch(() => ({}))) as ApiError
    throw new ApiRequestError(body)
  }
  if (response.status === 204) return undefined as T
  return response.json() as Promise<T>
}

function slugify(value: string, maxLength = 63): string {
  return value
    .toLowerCase()
    .trim()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '')
    .slice(0, maxLength)
}

function sameContent(first: Post, second: Post): boolean {
  return (
    first.title === second.title &&
    first.slug === second.slug &&
    (first.summary ?? '') === (second.summary ?? '') &&
    JSON.stringify(first.content_json) === JSON.stringify(second.content_json)
  )
}

function toSummary(post: Post): PostSummary {
  return {
    id: post.id,
    slug: post.slug,
    title: post.title,
    summary: post.summary,
    revision: post.revision,
    updated_at: post.updated_at,
  }
}

function App() {
  const [session, setSession] = useState<Session>()
  const [challenge, setChallenge] = useState<Challenge>()
  const [email, setEmail] = useState('')
  const [code, setCode] = useState('')
  const [title, setTitle] = useState('')
  const [slug, setSlug] = useState('')
  const [slugTouched, setSlugTouched] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [posts, setPosts] = useState<PostSummary[]>([])
  const [postsLoaded, setPostsLoaded] = useState(false)
  const [draft, setDraft] = useState<Post>()
  const [savedPost, setSavedPost] = useState<Post>()
  const [saveState, setSaveState] = useState<'idle' | 'saving' | 'saved' | 'error' | 'conflict'>(
    'idle',
  )
  const saving = useRef(false)
  const activePostId = useRef<string | undefined>(undefined)
  const hasUnsavedChanges = Boolean(draft && savedPost && !sameContent(draft, savedPost))

  useEffect(() => {
    api<Session>('/session')
      .then(setSession)
      .catch((caught: unknown) =>
        setError(caught instanceof Error ? caught.message : 'Unable to load your session.'),
      )
  }, [])

  useEffect(() => {
    if (!session?.blog) return
    api<PostSummary[]>('/posts')
      .then((loaded) => {
        setPosts(loaded)
        setPostsLoaded(true)
      })
      .catch((caught: unknown) => {
        setError(caught instanceof Error ? caught.message : 'Unable to load your posts.')
        setPostsLoaded(true)
      })
  }, [session?.blog])

  useEffect(() => {
    activePostId.current = draft?.id
  }, [draft?.id])

  useEffect(() => {
    if (
      !draft ||
      !savedPost ||
      !session?.csrf_token ||
      sameContent(draft, savedPost) ||
      saveState === 'conflict' ||
      saveState === 'error'
    ) {
      return
    }

    const snapshot = draft
    const csrfToken = session.csrf_token
    const timer = window.setTimeout(async () => {
      if (saving.current) return
      saving.current = true
      setSaveState('saving')
      try {
        const saved = await api<Post>(`/posts/${snapshot.id}`, {
          method: 'PUT',
          headers: { 'x-csrf-token': csrfToken },
          body: JSON.stringify({
            revision: savedPost.revision,
            title: snapshot.title,
            slug: snapshot.slug,
            summary: snapshot.summary ?? '',
            content_json: snapshot.content_json,
          }),
        })
        if (activePostId.current === saved.id) setSavedPost(saved)
        setDraft((current) => {
          if (!current || current.id !== saved.id) return current
          return sameContent(current, snapshot) ? saved : { ...current, revision: saved.revision }
        })
        setPosts((current) => [
          toSummary(saved),
          ...current.filter((post) => post.id !== saved.id),
        ])
        if (activePostId.current === saved.id) setSaveState('saved')
      } catch (caught) {
        if (activePostId.current === snapshot.id) {
          setSaveState(
            caught instanceof ApiRequestError && caught.code === 'stale_revision'
              ? 'conflict'
              : 'error',
          )
          setError(caught instanceof Error ? caught.message : 'Unable to save this post.')
        }
      } finally {
        saving.current = false
      }
    }, 800)

    return () => window.clearTimeout(timer)
  }, [draft, savedPost, saveState, session?.csrf_token])

  async function startEmail(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    setBusy(true)
    setError('')
    try {
      setChallenge(
        await api<Challenge>('/auth/email/start', {
          method: 'POST',
          body: JSON.stringify({ email }),
        }),
      )
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : 'Unable to send a code.')
    } finally {
      setBusy(false)
    }
  }

  async function verifyEmail(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (!challenge) return
    setBusy(true)
    setError('')
    try {
      setSession(
        await api<Session>('/auth/email/verify', {
          method: 'POST',
          body: JSON.stringify({ challenge_id: challenge.challenge_id, code }),
        }),
      )
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : 'Unable to verify that code.')
    } finally {
      setBusy(false)
    }
  }

  async function createBlog(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (!session?.csrf_token) return
    setBusy(true)
    setError('')
    try {
      const blog = await api<Blog>('/blog', {
        method: 'POST',
        headers: { 'x-csrf-token': session.csrf_token },
        body: JSON.stringify({ title, slug }),
      })
      setSession({ ...session, blog })
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : 'Unable to create your blog.')
    } finally {
      setBusy(false)
    }
  }

  async function signOut() {
    if (!session?.csrf_token) return
    setBusy(true)
    setError('')
    try {
      await api<void>('/auth/logout', {
        method: 'POST',
        headers: { 'x-csrf-token': session.csrf_token },
      })
      setSession({ authenticated: false })
      setChallenge(undefined)
      setCode('')
      setPosts([])
      setPostsLoaded(false)
      setDraft(undefined)
      setSavedPost(undefined)
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : 'Unable to sign out.')
    } finally {
      setBusy(false)
    }
  }

  async function createPost() {
    if (!session?.csrf_token) return
    setBusy(true)
    setError('')
    try {
      const post = await api<Post>('/posts', {
        method: 'POST',
        headers: { 'x-csrf-token': session.csrf_token },
        body: JSON.stringify({
          title: 'Untitled post',
          slug: `untitled-${crypto.randomUUID().slice(0, 8)}`,
        }),
      })
      setPosts((current) => [toSummary(post), ...current])
      setDraft(post)
      setSavedPost(post)
      setSaveState('saved')
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : 'Unable to create a post.')
    } finally {
      setBusy(false)
    }
  }

  async function openPost(postId: string) {
    setError('')
    try {
      const post = await api<Post>(`/posts/${postId}`)
      setDraft(post)
      setSavedPost(post)
      setSaveState('saved')
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : 'Unable to open that post.')
    }
  }

  function changeDraft(changes: Partial<Post>) {
    setDraft((current) => (current ? { ...current, ...changes } : current))
    if (saveState !== 'conflict') setSaveState('idle')
    setError('')
  }

  async function reloadPost() {
    if (!draft) return
    await openPost(draft.id)
  }

  async function deletePost() {
    if (!draft || !savedPost || !session?.csrf_token) return
    if (!window.confirm('Delete this draft? Its address will remain reserved.')) return
    setBusy(true)
    setError('')
    try {
      await api<void>(`/posts/${draft.id}`, {
        method: 'DELETE',
        headers: { 'x-csrf-token': session.csrf_token },
        body: JSON.stringify({ revision: savedPost.revision }),
      })
      setPosts((current) => current.filter((post) => post.id !== draft.id))
      setDraft(undefined)
      setSavedPost(undefined)
      setSaveState('idle')
    } catch (caught) {
      setSaveState(caught instanceof ApiRequestError && caught.code === 'stale_revision' ? 'conflict' : 'error')
      setError(caught instanceof Error ? caught.message : 'Unable to delete this post.')
    } finally {
      setBusy(false)
    }
  }

  if (!session) {
    return (
      <main className="shell" aria-live="polite">
        <p className="eyebrow">Nabu</p>
        <p className="loading">Preparing your space…</p>
        {error && <p className="error">{error}</p>}
      </main>
    )
  }

  if (!session.authenticated) {
    return (
      <main className="shell auth-shell">
        <section className="intro">
          <p className="eyebrow">Nabu</p>
          <h1>A quieter place to share what matters.</h1>
          <p className="lede">
            Create a private-first blog for your travels, photographs, videos, and everyday
            thoughts.
          </p>
        </section>

        <section className="card" aria-labelledby="sign-in-title">
          {!challenge ? (
            <form onSubmit={startEmail}>
              <p className="step">Author sign in</p>
              <h2 id="sign-in-title">Start with your email</h2>
              <p className="hint">We’ll send a six-digit code. No password needed.</p>
              <label htmlFor="email">Email address</label>
              <input
                id="email"
                name="email"
                type="email"
                autoComplete="email"
                required
                value={email}
                onChange={(event) => setEmail(event.target.value)}
                placeholder="you@example.com"
              />
              {error && <p className="error">{error}</p>}
              <button disabled={busy}>{busy ? 'Sending…' : 'Send my code'}</button>
            </form>
          ) : (
            <form onSubmit={verifyEmail}>
              <p className="step">Check your inbox</p>
              <h2 id="sign-in-title">Enter your code</h2>
              <p className="hint">
                We sent it to <strong>{email}</strong>. It expires in 10 minutes.
              </p>
              <label htmlFor="code">Six-digit code</label>
              <input
                className="code"
                id="code"
                name="code"
                type="text"
                inputMode="numeric"
                autoComplete="one-time-code"
                pattern="[0-9]{6}"
                maxLength={6}
                required
                value={code}
                onChange={(event) => setCode(event.target.value.replace(/\D/g, '').slice(0, 6))}
                placeholder="000000"
                autoFocus
              />
              {error && <p className="error">{error}</p>}
              <button disabled={busy || code.length !== 6}>
                {busy ? 'Verifying…' : 'Continue'}
              </button>
              <button
                className="text-button"
                type="button"
                onClick={() => {
                  setChallenge(undefined)
                  setCode('')
                  setError('')
                }}
              >
                Use a different email
              </button>
            </form>
          )}
        </section>
      </main>
    )
  }

  if (!session.blog) {
    return (
      <main className="shell onboarding-shell">
        <nav>
          <span className="wordmark">Nabu</span>
          <button className="text-button" type="button" onClick={signOut} disabled={busy}>
            Sign out
          </button>
        </nav>
        <section className="card onboarding-card">
          <p className="step">Welcome, {session.account?.email}</p>
          <h1>Name your space.</h1>
          <p className="hint">You can change the title later. The address stays with this blog.</p>
          <form onSubmit={createBlog}>
            <label htmlFor="title">Blog title</label>
            <input
              id="title"
              name="title"
              required
              maxLength={120}
              value={title}
              onChange={(event) => {
                setTitle(event.target.value)
                if (!slugTouched) setSlug(slugify(event.target.value))
              }}
              placeholder="The long way home"
              autoFocus
            />
            <label htmlFor="slug">Blog address</label>
            <div className="address-input">
              <input
                id="slug"
                name="slug"
                required
                minLength={3}
                maxLength={63}
                pattern="[a-z0-9](?:[a-z0-9-]*[a-z0-9])?"
                value={slug}
                onChange={(event) => {
                  setSlugTouched(true)
                  setSlug(slugify(event.target.value))
                }}
                placeholder="the-long-way-home"
              />
              <span>.{import.meta.env.VITE_ROOT_DOMAIN ?? 'nabu.test'}</span>
            </div>
            {error && <p className="error">{error}</p>}
            <button disabled={busy || title.trim() === '' || slug.length < 3}>
              {busy ? 'Creating…' : 'Create my blog'}
            </button>
          </form>
        </section>
      </main>
    )
  }

  return (
    <main className="author-shell">
      <nav>
        <div>
          <span className="wordmark">Nabu</span>
          <span className="nav-blog-title">{session.blog.title}</span>
        </div>
        <button
          className="text-button"
          type="button"
          onClick={signOut}
          disabled={busy || hasUnsavedChanges || saveState === 'saving'}
        >
          Sign out
        </button>
      </nav>
      <section className="author-workspace">
        <aside className="post-sidebar">
          <div className="post-sidebar-header">
            <div>
              <p className="step">Private drafts</p>
              <h1>Posts</h1>
            </div>
            <button
              type="button"
              onClick={createPost}
              disabled={busy || hasUnsavedChanges || saveState === 'saving'}
            >
              New post
            </button>
          </div>
          {!postsLoaded ? (
            <p className="empty-posts">Loading posts…</p>
          ) : posts.length === 0 ? (
            <p className="empty-posts">Your stories will gather here. Start with a new post.</p>
          ) : (
            <div className="post-list">
              {posts.map((post) => (
                <button
                  className={draft?.id === post.id ? 'post-row active' : 'post-row'}
                  type="button"
                  key={post.id}
                  onClick={() => openPost(post.id)}
                  disabled={hasUnsavedChanges || saveState === 'saving'}
                >
                  <strong>{post.title}</strong>
                  <span>{post.summary || 'Private draft'}</span>
                </button>
              ))}
            </div>
          )}
        </aside>

        {draft ? (
          <article className="editor-panel">
            <header className="editor-header">
              <p className="save-status" aria-live="polite">
                {saveState === 'saving' && 'Saving…'}
                {saveState === 'saved' && 'Saved'}
                {saveState === 'idle' && 'Unsaved changes'}
                {saveState === 'error' && 'Save failed'}
                {saveState === 'conflict' && 'Newer version available'}
              </p>
              <span className="revision">Revision {savedPost?.revision}</span>
            </header>
            {saveState === 'conflict' && (
              <div className="conflict" role="alert">
                <p>{error}</p>
                <button type="button" onClick={reloadPost}>Reload latest</button>
              </div>
            )}
            {error && saveState !== 'conflict' && <p className="error editor-error">{error}</p>}
            <div className="editor-fields">
              <label className="sr-only" htmlFor="post-title">Post title</label>
              <textarea
                className="post-title-input"
                id="post-title"
                rows={2}
                value={draft.title}
                maxLength={200}
                onChange={(event) => changeDraft({ title: event.target.value })}
                placeholder="Untitled post"
              />
              <label htmlFor="post-slug">Post address</label>
              <div className="post-address">
                <span>/</span>
                <input
                  id="post-slug"
                  value={draft.slug}
                  minLength={2}
                  maxLength={80}
                  pattern="[a-z0-9](?:[a-z0-9-]*[a-z0-9])?"
                  onChange={(event) => changeDraft({ slug: slugify(event.target.value, 80) })}
                  aria-label="Post address slug"
                />
              </div>
              <label htmlFor="post-summary">Short summary</label>
              <textarea
                className="summary-input"
                id="post-summary"
                value={draft.summary ?? ''}
                maxLength={500}
                onChange={(event) => changeDraft({ summary: event.target.value })}
                placeholder="A sentence to help you find this post later."
              />
              <label>Post</label>
              <RichEditor
                content={draft.content_json}
                revision={draft.revision}
                onChange={(content_json) => changeDraft({ content_json })}
              />
            </div>
            <footer className="editor-footer">
              <p>Private draft · only your blog’s authors can see this</p>
              <button
                className="danger-button"
                type="button"
                onClick={deletePost}
                disabled={busy || hasUnsavedChanges || saveState === 'saving'}
              >
                Delete draft
              </button>
            </footer>
          </article>
        ) : (
          <div className="editor-empty">
            <p className="eyebrow">A blank page, when you’re ready</p>
            <h1>Capture the moment before it moves on.</h1>
            <button type="button" onClick={createPost} disabled={busy}>Write a new post</button>
            {error && <p className="error">{error}</p>}
          </div>
        )}
      </section>
    </main>
  )
}

export default App
