import { useEffect, useState } from 'react'
import type { FormEvent } from 'react'
import './App.css'

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

type ApiError = {
  error?: {
    message?: string
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
    throw new Error(body.error?.message ?? 'Something went wrong. Try again.')
  }
  if (response.status === 204) return undefined as T
  return response.json() as Promise<T>
}

function slugify(value: string): string {
  return value
    .toLowerCase()
    .trim()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '')
    .slice(0, 63)
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

  useEffect(() => {
    api<Session>('/session')
      .then(setSession)
      .catch((caught: unknown) =>
        setError(caught instanceof Error ? caught.message : 'Unable to load your session.'),
      )
  }, [])

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
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : 'Unable to sign out.')
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
    <main className="shell dashboard-shell">
      <nav>
        <span className="wordmark">Nabu</span>
        <button className="text-button" type="button" onClick={signOut} disabled={busy}>
          Sign out
        </button>
      </nav>
      <section className="success-panel">
        <p className="step">Your space is ready</p>
        <h1>{session.blog.title}</h1>
        <p className="blog-address">
          {session.blog.slug}.{import.meta.env.VITE_ROOT_DOMAIN ?? 'nabu.test'}
        </p>
        <p className="lede">Next, we’ll add the place where your first post takes shape.</p>
      </section>
    </main>
  )
}

export default App
