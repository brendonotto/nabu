import './App.css'

function App() {
  return (
    <main>
      <p className="eyebrow">Nabu</p>
      <h1>Your private space is taking shape.</h1>
      <p className="lede">
        The authoring application is ready for its first feature: secure email sign-in.
      </p>
      <dl>
        <div>
          <dt>Backend</dt>
          <dd>Axum + PostgreSQL</dd>
        </div>
        <div>
          <dt>Frontend</dt>
          <dd>React + TypeScript</dd>
        </div>
        <div>
          <dt>Default</dt>
          <dd>Private by design</dd>
        </div>
      </dl>
    </main>
  )
}

export default App
