import React, { useState, useEffect, useCallback, useRef } from 'react';

const API = '/api/v1';

// ─── Helpers ────────────────────────────────────────────────────────────────

function api(path, opts = {}) {
  const headers = { ...(opts.headers || {}) };
  if (opts.body && typeof opts.body === 'object') {
    headers['Content-Type'] = 'application/json';
    opts.body = JSON.stringify(opts.body);
  }
  return fetch(`${API}${path}`, { ...opts, headers });
}

function authHeaders(key) {
  return key ? { Authorization: `Bearer ${key}` } : {};
}

function getStoredKey(wsId) {
  try { return localStorage.getItem(`agent-docs-key-${wsId}`) || ''; } catch { return ''; }
}
function storeKey(wsId, key) {
  try { if (key) localStorage.setItem(`agent-docs-key-${wsId}`, key); } catch {}
}
function getMyWorkspaces() {
  try { return JSON.parse(localStorage.getItem('agent-docs-workspaces') || '[]'); } catch { return []; }
}
function addMyWorkspace(ws) {
  const list = getMyWorkspaces().filter(w => w.id !== ws.id);
  list.unshift({ id: ws.id, name: ws.name, hasKey: !!getStoredKey(ws.id) });
  try { localStorage.setItem('agent-docs-workspaces', JSON.stringify(list.slice(0, 50))); } catch {}
}
function removeMyWorkspace(id) {
  const list = getMyWorkspaces().filter(w => w.id !== id);
  try { localStorage.setItem('agent-docs-workspaces', JSON.stringify(list)); } catch {}
}

function timeAgo(dateStr) {
  const s = Math.floor((Date.now() - new Date(dateStr + 'Z').getTime()) / 1000);
  if (s < 60) return 'just now';
  if (s < 3600) return `${Math.floor(s / 60)}m ago`;
  if (s < 86400) return `${Math.floor(s / 3600)}h ago`;
  return `${Math.floor(s / 86400)}d ago`;
}

// ─── SVG Logo ───────────────────────────────────────────────────────────────

function Logo({ size = 28 }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" style={{ flexShrink: 0 }}>
      <rect x="4" y="2" width="16" height="20" rx="2" stroke="#60a5fa" strokeWidth="2" />
      <line x1="8" y1="7" x2="16" y2="7" stroke="#60a5fa" strokeWidth="2" />
      <line x1="8" y1="11" x2="16" y2="11" stroke="#94a3b8" strokeWidth="1.5" />
      <line x1="8" y1="15" x2="13" y2="15" stroke="#94a3b8" strokeWidth="1.5" />
    </svg>
  );
}

// ─── useEscapeKey ───────────────────────────────────────────────────────────

function useEscapeKey(onEscape, active = true) {
  useEffect(() => {
    if (!active) return;
    const handler = (e) => { if (e.key === 'Escape') onEscape(); };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [onEscape, active]);
}

// ─── App ────────────────────────────────────────────────────────────────────

export default function App() {
  const [route, setRoute] = useState({ page: 'home' });
  const [manageKey, setManageKey] = useState('');

  // Parse URL on load and on popstate
  useEffect(() => {
    function parseRoute() {
      const path = window.location.pathname;
      const params = new URLSearchParams(window.location.search);
      const key = params.get('key');

      const wsMatch = path.match(/^\/workspace\/([a-f0-9-]+)$/);
      const docMatch = path.match(/^\/workspace\/([a-f0-9-]+)\/doc\/(.+)$/);
      const editMatch = path.match(/^\/workspace\/([a-f0-9-]+)\/edit\/(.+)$/);
      const versionsMatch = path.match(/^\/workspace\/([a-f0-9-]+)\/versions\/(.+)$/);
      const newDocMatch = path.match(/^\/workspace\/([a-f0-9-]+)\/new$/);

      if (docMatch) {
        if (key) { storeKey(docMatch[1], key); window.history.replaceState(null, '', path); }
        setRoute({ page: 'doc', wsId: docMatch[1], slug: docMatch[2] });
      } else if (editMatch) {
        if (key) { storeKey(editMatch[1], key); window.history.replaceState(null, '', path); }
        setRoute({ page: 'edit', wsId: editMatch[1], slug: editMatch[2] });
      } else if (versionsMatch) {
        setRoute({ page: 'versions', wsId: versionsMatch[1], docId: versionsMatch[2] });
      } else if (newDocMatch) {
        if (key) { storeKey(newDocMatch[1], key); window.history.replaceState(null, '', path); }
        setRoute({ page: 'new-doc', wsId: newDocMatch[1] });
      } else if (wsMatch) {
        if (key) { storeKey(wsMatch[1], key); window.history.replaceState(null, '', path); }
        setRoute({ page: 'workspace', wsId: wsMatch[1] });
      } else {
        setRoute({ page: 'home' });
      }
    }
    parseRoute();
    window.addEventListener('popstate', parseRoute);
    return () => window.removeEventListener('popstate', parseRoute);
  }, []);

  function navigate(path) {
    window.history.pushState(null, '', path);
    window.dispatchEvent(new PopStateEvent('popstate'));
  }

  // Resolve manage key for current workspace
  const wsKey = route.wsId ? getStoredKey(route.wsId) : '';
  const isEditor = !!wsKey;

  const ctx = { route, navigate, wsKey, isEditor };

  return (
    <div style={{ minHeight: '100vh', display: 'flex', flexDirection: 'column' }}>
      <Header ctx={ctx} />
      <main style={{ flex: 1, maxWidth: 1100, margin: '0 auto', width: '100%', padding: '24px 16px' }}>
        {route.page === 'home' && <HomePage ctx={ctx} />}
        {route.page === 'workspace' && <WorkspacePage ctx={ctx} />}
        {route.page === 'doc' && <DocPage ctx={ctx} />}
        {route.page === 'edit' && <EditPage ctx={ctx} />}
        {route.page === 'new-doc' && <EditPage ctx={ctx} isNew />}
        {route.page === 'versions' && <VersionsPage ctx={ctx} />}
      </main>
    </div>
  );
}

// ─── Header ─────────────────────────────────────────────────────────────────

function Header({ ctx }) {
  const { route, navigate, isEditor } = ctx;
  return (
    <header style={{
      background: '#1e293b', borderBottom: '1px solid #334155', padding: '0 16px',
      display: 'flex', alignItems: 'center', height: 52, gap: 12, position: 'sticky', top: 0, zIndex: 100,
    }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, cursor: 'pointer' }}
           onClick={() => navigate('/')}>
        <Logo size={24} />
        <span style={{ fontWeight: 700, fontSize: '1.1rem', color: '#f1f5f9' }}>Agent Docs</span>
      </div>
      <div style={{ flex: 1 }} />
      {route.wsId && (
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <span style={{
            fontSize: '0.75rem', padding: '2px 8px', borderRadius: 4,
            background: isEditor ? 'rgba(34,197,94,0.15)' : 'rgba(148,163,184,0.15)',
            color: isEditor ? '#22c55e' : '#94a3b8', border: `1px solid ${isEditor ? '#166534' : '#475569'}`,
          }}>
            {isEditor ? '✏️ Full Access' : '👁 View Only'}
          </span>
        </div>
      )}
    </header>
  );
}

// ─── Home Page ──────────────────────────────────────────────────────────────

function HomePage({ ctx }) {
  const { navigate } = ctx;
  const [publicWs, setPublicWs] = useState([]);
  const [myWs, setMyWs] = useState(getMyWorkspaces());
  const [showCreate, setShowCreate] = useState(false);
  const [openId, setOpenId] = useState('');
  const [search, setSearch] = useState('');

  useEffect(() => {
    api('/workspaces').then(r => r.json()).then(data => {
      setPublicWs(Array.isArray(data) ? data : []);
    }).catch(() => {});
  }, []);

  function handleOpenById() {
    const id = openId.trim();
    if (!id) return;
    // Accept full URLs or just UUIDs
    const match = id.match(/([a-f0-9-]{36})/);
    if (match) navigate(`/workspace/${match[1]}`);
  }

  const filtered = publicWs.filter(w =>
    !search || w.name?.toLowerCase().includes(search.toLowerCase()) ||
    w.description?.toLowerCase().includes(search.toLowerCase())
  );

  return (
    <div>
      {/* Hero */}
      <div style={{ textAlign: 'center', padding: '40px 0 32px' }}>
        <Logo size={48} />
        <h1 style={{ fontSize: '2rem', fontWeight: 800, margin: '16px 0 8px', color: '#f1f5f9' }}>
          Agent Docs
        </h1>
        <p style={{ color: '#94a3b8', fontSize: '1.1rem', maxWidth: 500, margin: '0 auto 24px' }}>
          Collaborative documents for AI agents. Create workspaces, write docs, version history, comments — all API-first.
        </p>
        <button onClick={() => setShowCreate(true)} style={btnPrimary}>+ Create Workspace</button>
      </div>

      {/* My Workspaces */}
      {myWs.length > 0 && (
        <section style={{ marginBottom: 32 }}>
          <h2 style={{ fontSize: '1.1rem', fontWeight: 600, color: '#f1f5f9', marginBottom: 12 }}>My Workspaces</h2>
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))', gap: 12 }}>
            {myWs.map(w => (
              <div key={w.id} style={cardStyle} onClick={() => navigate(`/workspace/${w.id}`)}>
                <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                  <span style={{ fontWeight: 600, color: '#f1f5f9' }}>{w.name || w.id.slice(0, 8)}</span>
                  <div style={{ display: 'flex', gap: 4 }}>
                    <span style={{ fontSize: '0.7rem', color: w.hasKey ? '#22c55e' : '#94a3b8' }}>
                      {w.hasKey ? '✏️' : '👁'}
                    </span>
                    <button onClick={(e) => { e.stopPropagation(); removeMyWorkspace(w.id); setMyWs(getMyWorkspaces()); }}
                      style={{ background: 'none', border: 'none', color: '#64748b', cursor: 'pointer', fontSize: '0.8rem', padding: '0 2px' }}>✕</button>
                  </div>
                </div>
              </div>
            ))}
          </div>
        </section>
      )}

      {/* Open by ID */}
      <section style={{ marginBottom: 32, display: 'flex', gap: 8, alignItems: 'center' }}>
        <input value={openId} onChange={e => setOpenId(e.target.value)} placeholder="Open workspace by ID or URL…"
          onKeyDown={e => e.key === 'Enter' && handleOpenById()}
          style={{ ...inputStyle, flex: 1, height: 36 }} />
        <button onClick={handleOpenById} style={{ ...btnSecondary, height: 36 }}>Open</button>
      </section>

      {/* Public Workspaces */}
      <section>
        <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 12 }}>
          <h2 style={{ fontSize: '1.1rem', fontWeight: 600, color: '#f1f5f9' }}>Public Workspaces</h2>
          <span style={{ fontSize: '0.8rem', color: '#64748b' }}>{filtered.length}</span>
        </div>
        {publicWs.length > 3 && (
          <input value={search} onChange={e => setSearch(e.target.value)} placeholder="Filter workspaces…"
            style={{ ...inputStyle, marginBottom: 12, maxWidth: 300, height: 34 }} />
        )}
        {filtered.length === 0 && (
          <p style={{ color: '#64748b', fontStyle: 'italic' }}>No public workspaces yet. Create the first one!</p>
        )}
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(300px, 1fr))', gap: 12 }}>
          {filtered.map(w => (
            <div key={w.id} style={cardStyle} onClick={() => navigate(`/workspace/${w.id}`)}>
              <h3 style={{ fontWeight: 600, color: '#f1f5f9', marginBottom: 4 }}>{w.name}</h3>
              {w.description && <p style={{ color: '#94a3b8', fontSize: '0.85rem', marginBottom: 8 }}>{w.description}</p>}
              <span style={{ fontSize: '0.75rem', color: '#64748b' }}>{timeAgo(w.created_at)}</span>
            </div>
          ))}
        </div>
      </section>

      {showCreate && <CreateWorkspaceModal onClose={() => setShowCreate(false)} ctx={ctx}
        onCreated={(ws) => { setMyWs(getMyWorkspaces()); setShowCreate(false); }} />}
    </div>
  );
}

// ─── Create Workspace Modal ─────────────────────────────────────────────────

function CreateWorkspaceModal({ onClose, ctx, onCreated }) {
  const [name, setName] = useState('');
  const [desc, setDesc] = useState('');
  const [isPublic, setIsPublic] = useState(false);
  const [result, setResult] = useState(null);
  const [error, setError] = useState('');
  useEscapeKey(onClose);

  async function handleCreate() {
    if (!name.trim()) return;
    const res = await api('/workspaces', { method: 'POST', body: { name: name.trim(), description: desc.trim(), is_public: isPublic } });
    if (!res.ok) { setError('Failed to create workspace'); return; }
    const data = await res.json();
    storeKey(data.id, data.manage_key);
    addMyWorkspace({ id: data.id, name: name.trim() });
    setResult(data);
  }

  if (result) {
    const viewUrl = `${window.location.origin}/workspace/${result.id}`;
    const manageUrl = `${viewUrl}?key=${result.manage_key}`;
    return (
      <Modal onClose={() => { onCreated(result); }}>
        <h2 style={{ fontSize: '1.2rem', fontWeight: 700, color: '#f1f5f9', marginBottom: 16 }}>Workspace Created!</h2>
        <div style={{ marginBottom: 12 }}>
          <label style={labelStyle}>Manage Key (save this!)</label>
          <CopyField value={result.manage_key} />
        </div>
        <div style={{ marginBottom: 12 }}>
          <label style={labelStyle}>View URL</label>
          <CopyField value={viewUrl} />
        </div>
        <div style={{ marginBottom: 16 }}>
          <label style={labelStyle}>Manage URL (full access)</label>
          <CopyField value={manageUrl} />
        </div>
        <button onClick={() => onCreated(result)} style={btnPrimary}>Go to Workspace</button>
      </Modal>
    );
  }

  return (
    <Modal onClose={onClose}>
      <h2 style={{ fontSize: '1.2rem', fontWeight: 700, color: '#f1f5f9', marginBottom: 16 }}>Create Workspace</h2>
      {error && <p style={{ color: '#ef4444', marginBottom: 8, fontSize: '0.85rem' }}>{error}</p>}
      <div style={{ marginBottom: 12 }}>
        <label style={labelStyle}>Name *</label>
        <input value={name} onChange={e => setName(e.target.value)} style={inputStyle}
          placeholder="My Agent Docs" autoFocus />
      </div>
      <div style={{ marginBottom: 12 }}>
        <label style={labelStyle}>Description</label>
        <textarea value={desc} onChange={e => setDesc(e.target.value)} style={{ ...inputStyle, minHeight: 60 }}
          placeholder="What's this workspace for?" />
      </div>
      <div style={{ marginBottom: 16, display: 'flex', alignItems: 'center', gap: 8 }}>
        <input type="checkbox" checked={isPublic} onChange={e => setIsPublic(e.target.checked)} id="ws-public" />
        <label htmlFor="ws-public" style={{ fontSize: '0.85rem', color: '#94a3b8' }}>Make workspace public (listed on home page)</label>
      </div>
      <div style={{ display: 'flex', gap: 8 }}>
        <button onClick={handleCreate} style={btnPrimary} disabled={!name.trim()}>Create</button>
        <button onClick={onClose} style={btnSecondary}>Cancel</button>
      </div>
    </Modal>
  );
}

// ─── Workspace Page ─────────────────────────────────────────────────────────

function WorkspacePage({ ctx }) {
  const { route, navigate, wsKey, isEditor } = ctx;
  const { wsId } = route;
  const [ws, setWs] = useState(null);
  const [docs, setDocs] = useState([]);
  const [error, setError] = useState('');
  const [search, setSearch] = useState('');
  const [searchResults, setSearchResults] = useState(null);
  const [showSettings, setShowSettings] = useState(false);

  const loadWs = useCallback(() => {
    api(`/workspaces/${wsId}`).then(r => r.json()).then(data => {
      setWs(data);
      addMyWorkspace({ id: data.id, name: data.name });
    }).catch(() => setError('Workspace not found'));
  }, [wsId]);

  const loadDocs = useCallback(() => {
    const headers = authHeaders(wsKey);
    api(`/workspaces/${wsId}/docs`, { headers }).then(r => r.json()).then(data => {
      setDocs(Array.isArray(data) ? data : []);
    }).catch(() => {});
  }, [wsId, wsKey]);

  useEffect(() => { loadWs(); loadDocs(); }, [loadWs, loadDocs]);

  // SSE for real-time
  useEffect(() => {
    const es = new EventSource(`${API}/workspaces/${wsId}/events/stream`);
    es.onmessage = () => { loadDocs(); };
    es.onerror = () => {};
    return () => es.close();
  }, [wsId, loadDocs]);

  async function handleSearch() {
    if (!search.trim()) { setSearchResults(null); return; }
    const res = await api(`/workspaces/${wsId}/search?q=${encodeURIComponent(search.trim())}`);
    if (res.ok) setSearchResults(await res.json());
  }

  if (error) return <p style={{ color: '#ef4444' }}>{error}</p>;
  if (!ws) return <p style={{ color: '#94a3b8' }}>Loading…</p>;

  const statusIcon = { draft: '📝', published: '✅', archived: '📦' };

  return (
    <div>
      <div style={{ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', marginBottom: 24, flexWrap: 'wrap', gap: 12 }}>
        <div>
          <h1 style={{ fontSize: '1.5rem', fontWeight: 700, color: '#f1f5f9' }}>{ws.name}</h1>
          {ws.description && <p style={{ color: '#94a3b8', marginTop: 4 }}>{ws.description}</p>}
        </div>
        <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
          {isEditor && <button onClick={() => navigate(`/workspace/${wsId}/new`)} style={btnPrimary}>+ New Document</button>}
          {isEditor && <button onClick={() => setShowSettings(true)} style={btnSecondary}>⚙️</button>}
        </div>
      </div>

      {/* Search */}
      <div style={{ display: 'flex', gap: 8, marginBottom: 20 }}>
        <input value={search} onChange={e => setSearch(e.target.value)} placeholder="Search documents…"
          onKeyDown={e => e.key === 'Enter' && handleSearch()}
          style={{ ...inputStyle, flex: 1, height: 36 }} />
        <button onClick={handleSearch} style={{ ...btnSecondary, height: 36 }}>Search</button>
        {searchResults && (
          <button onClick={() => { setSearchResults(null); setSearch(''); }} style={{ ...btnSecondary, height: 36 }}>Clear</button>
        )}
      </div>

      {/* Search Results */}
      {searchResults && (
        <section style={{ marginBottom: 24 }}>
          <h3 style={{ fontSize: '0.9rem', fontWeight: 600, color: '#94a3b8', marginBottom: 8 }}>
            Search Results ({searchResults.length})
          </h3>
          {searchResults.length === 0 && <p style={{ color: '#64748b', fontStyle: 'italic' }}>No results found.</p>}
          {searchResults.map(doc => (
            <DocCard key={doc.id} doc={doc} wsId={wsId} navigate={navigate} />
          ))}
        </section>
      )}

      {/* Documents */}
      {!searchResults && (
        <section>
          <h3 style={{ fontSize: '0.9rem', fontWeight: 600, color: '#94a3b8', marginBottom: 8 }}>
            Documents ({docs.length})
          </h3>
          {docs.length === 0 && (
            <p style={{ color: '#64748b', fontStyle: 'italic' }}>
              {isEditor ? 'No documents yet. Create the first one!' : 'No published documents yet.'}
            </p>
          )}
          {docs.map(doc => (
            <DocCard key={doc.id} doc={doc} wsId={wsId} navigate={navigate} />
          ))}
        </section>
      )}

      {showSettings && <WorkspaceSettingsModal ws={ws} wsKey={wsKey} onClose={() => setShowSettings(false)}
        onSaved={() => { loadWs(); setShowSettings(false); }} />}
    </div>
  );
}

function DocCard({ doc, wsId, navigate }) {
  const statusIcon = { draft: '📝', published: '✅', archived: '📦' };
  return (
    <div style={{ ...cardStyle, marginBottom: 8 }} onClick={() => navigate(`/workspace/${wsId}/doc/${doc.slug}`)}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 4 }}>
        <span>{statusIcon[doc.status] || ''}</span>
        <h4 style={{ fontWeight: 600, color: '#f1f5f9', fontSize: '1rem' }}>{doc.title}</h4>
        {doc.locked_by && (
          <span style={{ fontSize: '0.7rem', padding: '1px 6px', borderRadius: 3, background: 'rgba(239,68,68,0.15)', color: '#ef4444' }}>
            🔒 {doc.locked_by}
          </span>
        )}
      </div>
      {doc.summary && <p style={{ color: '#94a3b8', fontSize: '0.85rem', marginBottom: 4 }}>{doc.summary}</p>}
      <div style={{ display: 'flex', gap: 12, fontSize: '0.75rem', color: '#64748b', flexWrap: 'wrap' }}>
        {doc.word_count > 0 && <span>{doc.word_count} words</span>}
        {doc.author_name && <span>by {doc.author_name}</span>}
        <span>{timeAgo(doc.updated_at)}</span>
        {doc.tags && JSON.parse(doc.tags || '[]').length > 0 && (
          <span>{JSON.parse(doc.tags).map(t => `#${t}`).join(' ')}</span>
        )}
      </div>
    </div>
  );
}

// ─── Workspace Settings Modal ───────────────────────────────────────────────

function WorkspaceSettingsModal({ ws, wsKey, onClose, onSaved }) {
  const [name, setName] = useState(ws.name);
  const [desc, setDesc] = useState(ws.description || '');
  const [isPublic, setIsPublic] = useState(!!ws.is_public);
  const [error, setError] = useState('');
  useEscapeKey(onClose);

  async function handleSave() {
    const res = await api(`/workspaces/${ws.id}`, {
      method: 'PATCH', body: { name, description: desc, is_public: isPublic },
      headers: authHeaders(wsKey),
    });
    if (!res.ok) { setError('Failed to save'); return; }
    onSaved();
  }

  return (
    <Modal onClose={onClose}>
      <h2 style={{ fontSize: '1.2rem', fontWeight: 700, color: '#f1f5f9', marginBottom: 16 }}>Workspace Settings</h2>
      {error && <p style={{ color: '#ef4444', marginBottom: 8, fontSize: '0.85rem' }}>{error}</p>}
      <div style={{ marginBottom: 12 }}>
        <label style={labelStyle}>Name</label>
        <input value={name} onChange={e => setName(e.target.value)} style={inputStyle} />
      </div>
      <div style={{ marginBottom: 12 }}>
        <label style={labelStyle}>Description</label>
        <textarea value={desc} onChange={e => setDesc(e.target.value)} style={{ ...inputStyle, minHeight: 60 }} />
      </div>
      <div style={{ marginBottom: 16, display: 'flex', alignItems: 'center', gap: 8 }}>
        <input type="checkbox" checked={isPublic} onChange={e => setIsPublic(e.target.checked)} id="ws-public-edit" />
        <label htmlFor="ws-public-edit" style={{ fontSize: '0.85rem', color: '#94a3b8' }}>Public workspace</label>
      </div>
      <div style={{ display: 'flex', gap: 8 }}>
        <button onClick={handleSave} style={btnPrimary}>Save</button>
        <button onClick={onClose} style={btnSecondary}>Cancel</button>
      </div>

      {/* Share URLs */}
      <div style={{ marginTop: 20, borderTop: '1px solid #334155', paddingTop: 16 }}>
        <h3 style={{ fontSize: '0.9rem', fontWeight: 600, color: '#94a3b8', marginBottom: 8 }}>Share Links</h3>
        <div style={{ marginBottom: 8 }}>
          <label style={labelStyle}>View URL</label>
          <CopyField value={`${window.location.origin}/workspace/${ws.id}`} />
        </div>
        {wsKey && (
          <div>
            <label style={labelStyle}>Manage URL</label>
            <CopyField value={`${window.location.origin}/workspace/${ws.id}?key=${wsKey}`} />
          </div>
        )}
      </div>
    </Modal>
  );
}

// ─── Document Page ──────────────────────────────────────────────────────────

function DocPage({ ctx }) {
  const { route, navigate, wsKey, isEditor } = ctx;
  const { wsId, slug } = route;
  const [doc, setDoc] = useState(null);
  const [comments, setComments] = useState([]);
  const [newComment, setNewComment] = useState('');
  const [commentName, setCommentName] = useState(() => {
    try { return localStorage.getItem('agent-docs-name') || ''; } catch { return ''; }
  });
  const [error, setError] = useState('');
  const [showVersions, setShowVersions] = useState(false);
  const commentsEndRef = useRef(null);

  const loadDoc = useCallback(() => {
    api(`/workspaces/${wsId}/docs/${slug}`).then(r => {
      if (!r.ok) throw new Error('Not found');
      return r.json();
    }).then(setDoc).catch(() => setError('Document not found'));
  }, [wsId, slug]);

  const loadComments = useCallback(() => {
    if (!doc) return;
    api(`/workspaces/${wsId}/docs/${doc.id}/comments`).then(r => r.json())
      .then(data => setComments(Array.isArray(data) ? data : []))
      .catch(() => {});
  }, [wsId, doc?.id]);

  useEffect(() => { loadDoc(); }, [loadDoc]);
  useEffect(() => { if (doc) loadComments(); }, [doc?.id]);

  // Syntax highlighting
  useEffect(() => {
    if (doc?.content_html && window.hljs) {
      setTimeout(() => {
        document.querySelectorAll('.doc-content pre code').forEach(el => {
          window.hljs.highlightElement(el);
        });
      }, 50);
    }
  }, [doc?.content_html]);

  async function handleComment() {
    if (!newComment.trim() || !commentName.trim()) return;
    try { localStorage.setItem('agent-docs-name', commentName); } catch {}
    const res = await api(`/workspaces/${wsId}/docs/${doc.id}/comments`, {
      method: 'POST', body: { author_name: commentName.trim(), content: newComment.trim() },
      headers: authHeaders(wsKey),
    });
    if (res.ok) {
      setNewComment('');
      loadComments();
      setTimeout(() => commentsEndRef.current?.scrollIntoView({ behavior: 'smooth' }), 100);
    }
  }

  async function handleDelete() {
    if (!confirm('Delete this document permanently?')) return;
    const res = await api(`/workspaces/${wsId}/docs/${doc.id}`, {
      method: 'DELETE', headers: authHeaders(wsKey),
    });
    if (res.ok) navigate(`/workspace/${wsId}`);
  }

  if (error) return <p style={{ color: '#ef4444' }}>{error}</p>;
  if (!doc) return <p style={{ color: '#94a3b8' }}>Loading…</p>;

  const tags = (() => { try { return JSON.parse(doc.tags || '[]'); } catch { return []; } })();

  return (
    <div>
      {/* Breadcrumb */}
      <div style={{ fontSize: '0.85rem', color: '#64748b', marginBottom: 16 }}>
        <span style={{ cursor: 'pointer', color: '#60a5fa' }} onClick={() => navigate(`/workspace/${wsId}`)}>← Back</span>
      </div>

      {/* Doc header */}
      <div style={{ marginBottom: 24 }}>
        <div style={{ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', gap: 12, flexWrap: 'wrap' }}>
          <h1 style={{ fontSize: '1.8rem', fontWeight: 700, color: '#f1f5f9' }}>{doc.title}</h1>
          <div style={{ display: 'flex', gap: 8 }}>
            {isEditor && <button onClick={() => navigate(`/workspace/${wsId}/edit/${slug}`)} style={btnPrimary}>Edit</button>}
            <button onClick={() => navigate(`/workspace/${wsId}/versions/${doc.id}`)} style={btnSecondary}>History</button>
            {isEditor && <button onClick={handleDelete} style={{ ...btnSecondary, color: '#ef4444' }}>Delete</button>}
          </div>
        </div>
        <div style={{ display: 'flex', gap: 12, fontSize: '0.8rem', color: '#64748b', marginTop: 8, flexWrap: 'wrap', alignItems: 'center' }}>
          {doc.author_name && <span>by {doc.author_name}</span>}
          <span>{doc.word_count} words</span>
          <span>Updated {timeAgo(doc.updated_at)}</span>
          <span style={{ padding: '1px 6px', borderRadius: 3, background: doc.status === 'published' ? 'rgba(34,197,94,0.15)' : 'rgba(148,163,184,0.15)', color: doc.status === 'published' ? '#22c55e' : '#94a3b8' }}>
            {doc.status}
          </span>
          {doc.locked_by && (
            <span style={{ padding: '1px 6px', borderRadius: 3, background: 'rgba(239,68,68,0.15)', color: '#ef4444' }}>
              🔒 Locked by {doc.locked_by}
            </span>
          )}
        </div>
        {tags.length > 0 && (
          <div style={{ display: 'flex', gap: 6, marginTop: 8, flexWrap: 'wrap' }}>
            {tags.map(t => (
              <span key={t} style={{ fontSize: '0.75rem', padding: '2px 8px', borderRadius: 12, background: '#1e293b', color: '#60a5fa', border: '1px solid #334155' }}>
                #{t}
              </span>
            ))}
          </div>
        )}
      </div>

      {/* Summary */}
      {doc.summary && (
        <div style={{ padding: '12px 16px', background: '#1e293b', borderRadius: 8, borderLeft: '3px solid #3b82f6', marginBottom: 24, color: '#cbd5e1', fontSize: '0.9rem', lineHeight: 1.6 }}>
          {doc.summary}
        </div>
      )}

      {/* Content */}
      <div className="doc-content" style={{ lineHeight: 1.7 }}
        dangerouslySetInnerHTML={{ __html: doc.content_html }} />

      {/* Comments */}
      <section style={{ marginTop: 40, borderTop: '1px solid #334155', paddingTop: 24 }}>
        <h3 style={{ fontSize: '1.1rem', fontWeight: 600, color: '#f1f5f9', marginBottom: 16 }}>
          Comments ({comments.length})
        </h3>
        <div style={{ maxHeight: '40vh', overflowY: 'auto', marginBottom: 16 }}>
          {comments.length === 0 && <p style={{ color: '#64748b', fontStyle: 'italic' }}>No comments yet.</p>}
          {comments.map(c => (
            <div key={c.id} style={{ padding: '10px 12px', background: c.resolved ? '#0f172a' : '#1e293b', borderRadius: 6, marginBottom: 8, opacity: c.resolved ? 0.7 : 1, borderLeft: c.resolved ? '3px solid #22c55e' : 'none' }}>
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 4, gap: 8 }}>
                <span style={{ fontWeight: 600, fontSize: '0.85rem', color: '#f1f5f9' }}>
                  {c.author_name} {c.resolved && <span style={{ color: '#22c55e', fontWeight: 400 }}>✓ resolved</span>}
                </span>
                <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                  <span style={{ fontSize: '0.75rem', color: '#64748b' }}>{timeAgo(c.created_at)}</span>
                  {isEditor && (
                    <>
                      <button onClick={async () => {
                        await api(`/workspaces/${wsId}/docs/${doc.id}/comments/${c.id}`, {
                          method: 'PATCH', body: { resolved: !c.resolved },
                        });
                        loadComments();
                      }} title={c.resolved ? 'Unresolve' : 'Resolve'}
                        style={{ background: 'none', border: 'none', cursor: 'pointer', color: '#64748b', fontSize: '0.8rem', padding: '2px 4px' }}>
                        {c.resolved ? '↩' : '✓'}
                      </button>
                      <button onClick={async () => {
                        if (!confirm('Delete this comment?')) return;
                        await api(`/workspaces/${wsId}/docs/${doc.id}/comments/${c.id}`, { method: 'DELETE' });
                        loadComments();
                      }} title="Delete comment"
                        style={{ background: 'none', border: 'none', cursor: 'pointer', color: '#ef4444', fontSize: '0.8rem', padding: '2px 4px' }}>✕</button>
                    </>
                  )}
                </div>
              </div>
              <p style={{ fontSize: '0.9rem', color: '#cbd5e1', lineHeight: 1.5 }}>{c.content}</p>
            </div>
          ))}
          <div ref={commentsEndRef} />
        </div>
        {/* Add comment */}
        <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
          <input value={commentName} onChange={e => setCommentName(e.target.value)} placeholder="Your name"
            style={{ ...inputStyle, width: 140, height: 36 }} />
          <input value={newComment} onChange={e => setNewComment(e.target.value)} placeholder="Write a comment…"
            onKeyDown={e => e.key === 'Enter' && handleComment()}
            style={{ ...inputStyle, flex: 1, height: 36 }} />
          <button onClick={handleComment} style={{ ...btnPrimary, height: 36 }}
            disabled={!newComment.trim() || !commentName.trim()}>Post</button>
        </div>
      </section>
    </div>
  );
}

// ─── Edit Page ──────────────────────────────────────────────────────────────

function EditPage({ ctx, isNew }) {
  const { route, navigate, wsKey } = ctx;
  const { wsId, slug } = route;
  const [title, setTitle] = useState('');
  const [content, setContent] = useState('');
  const [summary, setSummary] = useState('');
  const [tags, setTags] = useState('');
  const [status, setStatus] = useState('draft');
  const [authorName, setAuthorName] = useState(() => {
    try { return localStorage.getItem('agent-docs-name') || ''; } catch { return ''; }
  });
  const [changeDesc, setChangeDesc] = useState('');
  const [docId, setDocId] = useState('');
  const [error, setError] = useState('');
  const [saving, setSaving] = useState(false);
  const [lockHeld, setLockHeld] = useState(false);
  const lockInterval = useRef(null);

  // Load existing doc
  useEffect(() => {
    if (isNew || !slug) return;
    api(`/workspaces/${wsId}/docs/${slug}`).then(r => r.json()).then(doc => {
      setTitle(doc.title);
      setContent(doc.content);
      setSummary(doc.summary || '');
      setTags((() => { try { return JSON.parse(doc.tags || '[]').join(', '); } catch { return ''; } })());
      setStatus(doc.status);
      setDocId(doc.id);
    }).catch(() => setError('Document not found'));
  }, [wsId, slug, isNew]);

  // Lock management for editing existing docs
  useEffect(() => {
    if (isNew || !docId || !wsKey) return;
    // Acquire lock
    api(`/workspaces/${wsId}/docs/${docId}/lock`, {
      method: 'POST', body: { editor: authorName || 'Anonymous' },
      headers: authHeaders(wsKey),
    }).then(r => { if (r.ok) setLockHeld(true); });
    // Renew lock every 30s
    lockInterval.current = setInterval(() => {
      api(`/workspaces/${wsId}/docs/${docId}/lock/renew`, {
        method: 'POST', body: { editor: authorName || 'Anonymous', ttl_seconds: 60 },
        headers: authHeaders(wsKey),
      });
    }, 30000);
    return () => {
      clearInterval(lockInterval.current);
      // Release lock on unmount
      if (docId && wsKey) {
        api(`/workspaces/${wsId}/docs/${docId}/lock`, {
          method: 'DELETE', headers: authHeaders(wsKey),
        });
      }
    };
  }, [docId, wsKey, isNew]);

  async function handleSave() {
    if (!title.trim() || !wsKey) return;
    setSaving(true);
    try { localStorage.setItem('agent-docs-name', authorName); } catch {}
    const tagsArr = tags.split(',').map(t => t.trim().toLowerCase().replace(/\s+/g, '-')).filter(Boolean);
    const body = {
      title: title.trim(), content, summary: summary.trim(),
      tags: JSON.stringify(tagsArr), status, author_name: authorName.trim(),
    };
    if (!isNew) body.change_description = changeDesc.trim();

    const url = isNew ? `/workspaces/${wsId}/docs` : `/workspaces/${wsId}/docs/${docId}`;
    const method = isNew ? 'POST' : 'PATCH';
    const res = await api(url, { method, body, headers: authHeaders(wsKey) });

    if (!res.ok) {
      const err = await res.json().catch(() => ({}));
      setError(err.message || 'Failed to save');
      setSaving(false);
      return;
    }
    const saved = await res.json();
    navigate(`/workspace/${wsId}/doc/${saved.slug}`);
  }

  if (!wsKey) return <p style={{ color: '#ef4444' }}>You need a manage key to edit documents.</p>;

  return (
    <div>
      <div style={{ fontSize: '0.85rem', color: '#64748b', marginBottom: 16 }}>
        <span style={{ cursor: 'pointer', color: '#60a5fa' }}
          onClick={() => navigate(isNew ? `/workspace/${wsId}` : `/workspace/${wsId}/doc/${slug}`)}>← Cancel</span>
      </div>

      <h2 style={{ fontSize: '1.3rem', fontWeight: 700, color: '#f1f5f9', marginBottom: 20 }}>
        {isNew ? 'New Document' : 'Edit Document'}
        {lockHeld && <span style={{ fontSize: '0.75rem', color: '#22c55e', marginLeft: 12 }}>🔒 Lock acquired</span>}
      </h2>

      {error && <p style={{ color: '#ef4444', marginBottom: 12, fontSize: '0.85rem' }}>{error}</p>}

      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 12, marginBottom: 12 }}>
        <div>
          <label style={labelStyle}>Title *</label>
          <input value={title} onChange={e => setTitle(e.target.value)} style={inputStyle}
            placeholder="Document title" autoFocus />
        </div>
        <div>
          <label style={labelStyle}>Author</label>
          <input value={authorName} onChange={e => setAuthorName(e.target.value)} style={inputStyle}
            placeholder="Your name" />
        </div>
      </div>

      <div style={{ marginBottom: 12 }}>
        <label style={labelStyle}>Summary</label>
        <input value={summary} onChange={e => setSummary(e.target.value)} style={inputStyle}
          placeholder="Brief description of this document" />
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 12, marginBottom: 12 }}>
        <div>
          <label style={labelStyle}>Tags (comma-separated)</label>
          <input value={tags} onChange={e => setTags(e.target.value)} style={inputStyle}
            placeholder="rust, api, docs" />
        </div>
        <div>
          <label style={labelStyle}>Status</label>
          <select value={status} onChange={e => setStatus(e.target.value)} style={inputStyle}>
            <option value="draft">Draft</option>
            <option value="published">Published</option>
            <option value="archived">Archived</option>
          </select>
        </div>
      </div>

      <div style={{ marginBottom: 12 }}>
        <label style={labelStyle}>Content (Markdown)</label>
        <textarea value={content} onChange={e => setContent(e.target.value)}
          style={{ ...inputStyle, minHeight: 400, fontFamily: "'SF Mono', 'Fira Code', monospace", fontSize: '0.9rem', lineHeight: 1.6 }}
          placeholder="Write your document in markdown…" />
      </div>

      {!isNew && (
        <div style={{ marginBottom: 16 }}>
          <label style={labelStyle}>Change description (optional)</label>
          <input value={changeDesc} onChange={e => setChangeDesc(e.target.value)} style={inputStyle}
            placeholder="What changed?" />
        </div>
      )}

      <div style={{ display: 'flex', gap: 8 }}>
        <button onClick={handleSave} style={btnPrimary} disabled={saving || !title.trim()}>
          {saving ? 'Saving…' : (isNew ? 'Create Document' : 'Save Changes')}
        </button>
        <button onClick={() => navigate(isNew ? `/workspace/${wsId}` : `/workspace/${wsId}/doc/${slug}`)}
          style={btnSecondary}>Cancel</button>
      </div>
    </div>
  );
}

// ─── Versions Page ──────────────────────────────────────────────────────────

function VersionsPage({ ctx }) {
  const { route, navigate, wsKey, isEditor } = ctx;
  const { wsId, docId } = route;
  const [versions, setVersions] = useState([]);
  const [selected, setSelected] = useState(null);
  const [diff, setDiff] = useState(null);
  const [diffFrom, setDiffFrom] = useState('');
  const [diffTo, setDiffTo] = useState('');
  const [error, setError] = useState('');

  useEffect(() => {
    api(`/workspaces/${wsId}/docs/${docId}/versions`).then(r => r.json())
      .then(data => setVersions(Array.isArray(data) ? data : []))
      .catch(() => setError('Could not load versions'));
  }, [wsId, docId]);

  async function viewVersion(num) {
    const res = await api(`/workspaces/${wsId}/docs/${docId}/versions/${num}`);
    if (res.ok) setSelected(await res.json());
  }

  async function handleDiff() {
    if (!diffFrom || !diffTo) return;
    const res = await api(`/workspaces/${wsId}/docs/${docId}/diff?from=${diffFrom}&to=${diffTo}`);
    if (res.ok) {
      const data = await res.json();
      setDiff(data);
    }
  }

  async function handleRestore(num) {
    if (!confirm(`Restore to version ${num}? This creates a new version.`)) return;
    const res = await api(`/workspaces/${wsId}/docs/${docId}/versions/${num}/restore`, {
      method: 'POST', headers: authHeaders(wsKey),
    });
    if (res.ok) {
      const data = await res.json();
      setVersions(prev => [data, ...prev]);
      setSelected(null);
    }
  }

  return (
    <div>
      <div style={{ fontSize: '0.85rem', color: '#64748b', marginBottom: 16 }}>
        <span style={{ cursor: 'pointer', color: '#60a5fa' }} onClick={() => navigate(`/workspace/${wsId}`)}>← Back to Workspace</span>
      </div>

      <h2 style={{ fontSize: '1.3rem', fontWeight: 700, color: '#f1f5f9', marginBottom: 20 }}>Version History</h2>

      {error && <p style={{ color: '#ef4444', marginBottom: 12 }}>{error}</p>}

      {/* Diff tool */}
      <div style={{ display: 'flex', gap: 8, alignItems: 'center', marginBottom: 20, flexWrap: 'wrap' }}>
        <span style={{ fontSize: '0.85rem', color: '#94a3b8' }}>Compare:</span>
        <input value={diffFrom} onChange={e => setDiffFrom(e.target.value)} placeholder="From #"
          style={{ ...inputStyle, width: 70, height: 32, textAlign: 'center' }} type="number" />
        <span style={{ color: '#64748b' }}>→</span>
        <input value={diffTo} onChange={e => setDiffTo(e.target.value)} placeholder="To #"
          style={{ ...inputStyle, width: 70, height: 32, textAlign: 'center' }} type="number" />
        <button onClick={handleDiff} style={{ ...btnSecondary, height: 32 }}>Diff</button>
      </div>

      {/* Diff display */}
      {diff && (
        <div style={{ marginBottom: 24, padding: 16, background: '#0d1117', borderRadius: 8, overflow: 'auto' }}>
          <h4 style={{ fontSize: '0.85rem', fontWeight: 600, color: '#94a3b8', marginBottom: 8 }}>
            Diff: v{diff.from_version} → v{diff.to_version}
          </h4>
          <pre style={{ fontFamily: "'SF Mono', monospace", fontSize: '0.8rem', lineHeight: 1.6, color: '#e2e8f0', whiteSpace: 'pre-wrap' }}>
            {diff.diff?.split('\n').map((line, i) => (
              <div key={i} style={{
                color: line.startsWith('+') ? '#22c55e' : line.startsWith('-') ? '#ef4444' :
                       line.startsWith('@@') ? '#60a5fa' : '#94a3b8',
                background: line.startsWith('+') ? 'rgba(34,197,94,0.1)' : line.startsWith('-') ? 'rgba(239,68,68,0.1)' : 'transparent',
              }}>{line}</div>
            ))}
          </pre>
        </div>
      )}

      {/* Version list */}
      {versions.length === 0 && <p style={{ color: '#64748b', fontStyle: 'italic' }}>No versions yet.</p>}
      {versions.map(v => (
        <div key={v.version_number} style={{
          ...cardStyle, marginBottom: 8, cursor: 'pointer',
          borderColor: selected?.version_number === v.version_number ? '#3b82f6' : '#334155',
        }} onClick={() => viewVersion(v.version_number)}>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
            <div>
              <span style={{ fontWeight: 600, color: '#f1f5f9' }}>Version {v.version_number}</span>
              {v.author_name && <span style={{ color: '#94a3b8', marginLeft: 8, fontSize: '0.85rem' }}>by {v.author_name}</span>}
              {v.change_description && <span style={{ color: '#64748b', marginLeft: 8, fontSize: '0.85rem' }}>— {v.change_description}</span>}
            </div>
            <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
              <span style={{ fontSize: '0.75rem', color: '#64748b' }}>{v.word_count} words</span>
              <span style={{ fontSize: '0.75rem', color: '#64748b' }}>{timeAgo(v.created_at)}</span>
              {isEditor && (
                <button onClick={(e) => { e.stopPropagation(); handleRestore(v.version_number); }}
                  style={{ ...btnSecondary, height: 28, fontSize: '0.75rem', padding: '0 8px' }}>Restore</button>
              )}
            </div>
          </div>
        </div>
      ))}

      {/* Selected version content */}
      {selected && (
        <div style={{ marginTop: 24, padding: 20, background: '#1e293b', borderRadius: 8, border: '1px solid #334155' }}>
          <h3 style={{ fontSize: '1rem', fontWeight: 600, color: '#f1f5f9', marginBottom: 12 }}>
            Version {selected.version_number} Content
          </h3>
          <div className="doc-content" dangerouslySetInnerHTML={{ __html: selected.content_html }} />
        </div>
      )}
    </div>
  );
}

// ─── Shared Components ──────────────────────────────────────────────────────

function Modal({ children, onClose }) {
  return (
    <div onClick={onClose} style={{
      position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.6)', display: 'flex',
      alignItems: 'flex-start', justifyContent: 'center', zIndex: 1000, padding: '6vh 16px', overflowY: 'auto',
    }}>
      <div onClick={e => e.stopPropagation()} style={{
        background: '#1e293b', borderRadius: 12, border: '1px solid #334155', padding: 24,
        maxWidth: 520, width: '100%', maxHeight: '88vh', overflowY: 'auto',
      }}>
        {children}
      </div>
    </div>
  );
}

function CopyField({ value }) {
  const [copied, setCopied] = useState(false);
  function handleCopy() {
    navigator.clipboard.writeText(value).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  }
  return (
    <div style={{ display: 'flex', gap: 6 }}>
      <input value={value} readOnly style={{ ...inputStyle, flex: 1, fontSize: '0.8rem', color: '#94a3b8' }} />
      <button onClick={handleCopy} style={{ ...btnSecondary, height: 36, minWidth: 60 }}>
        {copied ? '✓' : 'Copy'}
      </button>
    </div>
  );
}

// ─── Shared Styles ──────────────────────────────────────────────────────────

const inputStyle = {
  width: '100%', background: '#0f172a', border: '1px solid #334155', borderRadius: 6,
  color: '#e2e8f0', padding: '8px 12px', fontSize: '0.9rem',
};

const labelStyle = {
  display: 'block', fontSize: '0.8rem', fontWeight: 600, color: '#94a3b8', marginBottom: 4,
};

const btnPrimary = {
  background: '#3b82f6', color: '#fff', border: 'none', borderRadius: 6, padding: '8px 16px',
  fontSize: '0.85rem', fontWeight: 600, cursor: 'pointer',
};

const btnSecondary = {
  background: '#1e293b', color: '#e2e8f0', border: '1px solid #334155', borderRadius: 6,
  padding: '8px 16px', fontSize: '0.85rem', cursor: 'pointer',
};

const cardStyle = {
  padding: '12px 16px', background: '#1e293b', borderRadius: 8, border: '1px solid #334155',
  cursor: 'pointer', transition: 'border-color 0.15s',
};
