import { useEffect, useMemo, useRef, useState, type ReactElement } from 'react'
import { subscribeRuntimeMissing, subscribeSnapshot, subscribeUserError, tauriInvoke } from './api'
import { emptySnapshot, type AppSnapshot, type Bookmark, type ChannelNode } from './types'

const fallbackSnapshot: AppSnapshot = {
  ...emptySnapshot,
  connection: { ...emptySnapshot.connection, reason: '请在 Tauri 窗口中运行 MicaSpeak' },
}

function statusText(snapshot: AppSnapshot) {
  if (snapshot.connection.status === 'connected') return '已连接'
  if (snapshot.connection.status === 'connecting') return '连接中'
  return '未连接'
}

function RuntimeMissing() {
  return (
    <main className="runtime-page">
      <section className="runtime-panel">
        <div className="brand-mark">M</div>
        <h1>需要安装 WebView2 Runtime</h1>
        <p>当前 Windows 没有可用的 Evergreen WebView2 运行时。安装完成后重新启动 MicaSpeak。</p>
        <a className="primary-button" href="https://developer.microsoft.com/microsoft-edge/webview2/" target="_blank" rel="noreferrer">
          打开官方安装页面
        </a>
      </section>
    </main>
  )
}

function ChannelTree({ channels, onSelect }: { channels: ChannelNode[]; onSelect: (channel: ChannelNode) => void }) {
  const roots = channels.filter((channel) => channel.parent_id === null).sort((a, b) => a.order - b.order)
  const childrenOf = (parentId: number) => channels.filter((channel) => channel.parent_id === parentId).sort((a, b) => a.order - b.order)

  const render = (channel: ChannelNode, depth = 0): ReactElement => (
    <div key={channel.id}>
      <button className="channel-row" style={{ paddingLeft: 10 + depth * 16 }} onDoubleClick={() => onSelect(channel)} title="双击进入频道">
        <span className="chevron">{childrenOf(channel.id).length ? '▾' : '·'}</span>
        <span className="channel-name">{channel.name}</span>
        {channel.password && <span className="lock" aria-label="密码频道">⌑</span>}
      </button>
      {channel.clients.map((client) => (
        <div className={`client-row ${client.is_self ? 'self' : ''}`} key={`client-${client.id}`} style={{ paddingLeft: 26 + depth * 16 }}>
          <span className="avatar">{client.name.slice(0, 1)}</span>
          <span>{client.name}{client.is_self ? '（我）' : ''}</span>
        </div>
      ))}
      {childrenOf(channel.id).map((child) => render(child, depth + 1))}
    </div>
  )

  if (!roots.length) return <div className="empty-state">连接后将显示频道和在线用户</div>
  return <div className="tree">{roots.map((channel) => render(channel))}</div>
}

function ChannelPasswordModal({ name, error, busy, onSubmit, onCancel }: {
  name: string
  error: string
  busy: boolean
  onSubmit: (password: string) => void
  onCancel: () => void
}) {
  const [password, setPassword] = useState('')
  const inputRef = useRef<HTMLInputElement>(null)
  useEffect(() => { inputRef.current?.focus() }, [])
  return (
    <div className="modal-overlay" onClick={onCancel}>
      <div className="modal-card" onClick={(e) => e.stopPropagation()}>
        <strong>频道「{name}」需要密码</strong>
        <input
          ref={inputRef}
          type="password"
          value={password}
          placeholder="输入频道密码"
          onChange={(e) => setPassword(e.target.value)}
          onKeyDown={(e) => { if (e.key === 'Enter' && !busy && password) onSubmit(password) }}
        />
        {error && <div className="error-line">{error}</div>}
        <div className="modal-actions">
          <button className="secondary-button" onClick={onCancel} disabled={busy}>取消</button>
          <button className="primary-button" disabled={busy || !password} onClick={() => onSubmit(password)}>{busy ? '正在进入…' : '进入频道'}</button>
        </div>
      </div>
    </div>
  )
}

function ConnectForm({ snapshot, onConnect, onConnectBookmark, onSave, onDelete }: {
  snapshot: AppSnapshot
  onConnect: (address: string, nickname: string, password: string) => Promise<void>
  onConnectBookmark: (bookmark: Bookmark) => void
  onSave: (address: string, nickname: string, password: string) => Promise<void>
  onDelete: (id: string) => void
}) {
  const [address, setAddress] = useState('saintbb1234.ts3.uno:9987')
  const [nickname, setNickname] = useState('MicaSpeak')
  const [password, setPassword] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const connecting = snapshot.connection.status === 'connecting'
  const connectReason = snapshot.connection.status === 'disconnected' ? snapshot.connection.reason : null
  const submit = async (save: boolean) => {
    setBusy(true); setError('')
    try { await (save ? onSave(address, nickname, password) : onConnect(address, nickname, password)) }
    catch (err) { setError(String(err).replace(/^Error:\s*/, '')) }
    finally { setBusy(false) }
  }
  return (
    <section className="connect-page">
      <div className="brand-lockup"><div className="brand-mark">M</div><div><strong>MicaSpeak</strong><span>TS3 语音客户端</span></div></div>
      <div className="form-card">
        <label>服务器地址<input value={address} onChange={(e) => setAddress(e.target.value)} placeholder="主机:9987" /></label>
        <label>昵称<input value={nickname} onChange={(e) => setNickname(e.target.value)} placeholder="你的昵称" /></label>
        <label>服务器密码 <span className="optional">可选</span><input type="password" value={password} onChange={(e) => setPassword(e.target.value)} /></label>
        {(error || connectReason) && <div className="error-line">{error || connectReason}</div>}
        <button className="primary-button" disabled={busy || connecting || !address || !nickname} onClick={() => void submit(false)}>{busy || connecting ? '正在连接…' : '连接服务器'}</button>
        <button className="link-button" disabled={busy || connecting} onClick={() => void submit(true)}>保存为书签</button>
      </div>
      <div className="bookmarks">
        <div className="section-label">书签</div>
        {snapshot.bookmarks.length === 0 ? <div className="empty-state compact">还没有保存的服务器</div> : snapshot.bookmarks.map((bookmark) => (
          <BookmarkRow
            key={bookmark.id}
            bookmark={bookmark}
            onConnect={() => onConnectBookmark(bookmark)}
            onDelete={() => onDelete(bookmark.id)}
          />
        ))}
      </div>
      <div className="status-line">
        <span className={`status-dot ${snapshot.connection.status}`} />
        <span>{connecting ? '连接中…' : snapshot.connection.status === 'disconnected' && snapshot.connection.reason ? '已断开' : '未连接'}</span>
      </div>
    </section>
  )
}

function BookmarkRow({ bookmark, onConnect, onDelete }: { bookmark: Bookmark; onConnect: () => void; onDelete: () => void }) {
  return (
    <div className="bookmark-row">
      <button className="bookmark-main" onDoubleClick={onConnect} title="双击连接">
        <span className="bookmark-icon">★</span>
        <span className="bookmark-text"><strong>{bookmark.nickname}</strong><small>{bookmark.address}</small></span>
        <span className="bookmark-arrow">›</span>
      </button>
      <button className="bookmark-delete" aria-label={`删除书签 ${bookmark.nickname}`} title="删除书签" onClick={onDelete}>×</button>
    </div>
  )
}

export default function App() {
  const [snapshot, setSnapshot] = useState<AppSnapshot>(emptySnapshot)
  const [started, setStarted] = useState(false)
  const [runtimeMissing, setRuntimeMissing] = useState(false)
  const [settingsOpen, setSettingsOpen] = useState(false)
  const [notice, setNotice] = useState('')
  const [pwdPrompt, setPwdPrompt] = useState<{ id: number; name: string } | null>(null)
  const [pwdError, setPwdError] = useState('')
  const [pwdBusy, setPwdBusy] = useState(false)
  const promptRef = useRef(pwdPrompt)
  promptRef.current = pwdPrompt

  useEffect(() => {
    let active = true
    const offs: Array<() => void> = []
    const start = async () => {
      try {
        offs.push(await subscribeRuntimeMissing(() => setRuntimeMissing(true)))
        offs.push(await subscribeSnapshot((next) => active && setSnapshot(next)))
        offs.push(await subscribeUserError((message) => {
          if (!active) return
          if (promptRef.current) setPwdError(message)
          else { setNotice(message); setTimeout(() => setNotice(''), 4000) }
        }))
        const initial = await tauriInvoke<AppSnapshot>('get_app_snapshot')
        if (active) { setSnapshot(initial); setStarted(true) }
      } catch {
        if (active) { setSnapshot(fallbackSnapshot); setStarted(true) }
      }
    }
    void start()
    return () => { active = false; offs.forEach((off) => off()) }
  }, [])

  const connected = snapshot.connection.status === 'connected'
  const groupedChannels = useMemo(() => snapshot.channels, [snapshot.channels])
  const connect = async (address: string, nickname: string, password: string) => {
    await tauriInvoke('connect', { address, nickname, password: password || null, bookmarkId: null })
  }
  const connectBookmark = async (bookmark: Bookmark) => {
    // 书本密码由 Rust 侧从配置解析，不经过前端状态
    await tauriInvoke('connect', { address: bookmark.address, nickname: bookmark.nickname, password: null, bookmarkId: bookmark.id })
  }
  const save = async (address: string, nickname: string, password: string) => {
    await tauriInvoke('save_bookmark', { address, nickname, password: password || null })
    setNotice('书签已保存'); setTimeout(() => setNotice(''), 1800)
  }
  const removeBookmark = async (id: string) => {
    try {
      await tauriInvoke('delete_bookmark', { id })
      setNotice('书签已删除'); setTimeout(() => setNotice(''), 1800)
    } catch (err) { setNotice(String(err).replace(/^Error:\s*/, '')) }
  }
  const disconnect = async () => { await tauriInvoke('disconnect') }
  // M2a：按住说话。pointer capture 保证按住后拖出按钮也能在松开时结束发送。
  const [transmitting, setTransmitting] = useState(false)
  const transmitRef = useRef(false)
  const setTransmit = (enabled: boolean) => {
    if (transmitRef.current === enabled) return
    transmitRef.current = enabled
    setTransmitting(enabled)
    void tauriInvoke('set_transmit_enabled', { enabled }).catch(() => { transmitRef.current = !enabled; setTransmitting(!enabled) })
  }
  const selectChannel = async (channel: ChannelNode) => {
    if (channel.password) { setPwdError(''); setPwdPrompt({ id: channel.id, name: channel.name }); return }
    try { await tauriInvoke('select_channel', { channelId: channel.id, password: null }) }
    catch (err) { setNotice(String(err).replace(/^Error:\s*/, '')) }
  }
  const submitChannelPassword = async (password: string) => {
    if (!pwdPrompt) return
    setPwdBusy(true); setPwdError('')
    try {
      await tauriInvoke('select_channel', { channelId: pwdPrompt.id, password: password || null })
      setPwdPrompt(null)
    } catch (err) { setPwdError(String(err).replace(/^Error:\s*/, '')) }
    finally { setPwdBusy(false) }
  }

  if (runtimeMissing || (started && !snapshot.runtime_available)) return <RuntimeMissing />
  if (!started) return <main className="loading-page"><div className="loader" /><span>正在启动 MicaSpeak…</span></main>
  const noticeEl = notice ? <div className="notice">{notice}</div> : null
  if (!connected) return <>{noticeEl}<ConnectForm snapshot={snapshot} onConnect={connect} onConnectBookmark={(b) => void connectBookmark(b)} onSave={save} onDelete={(id) => void removeBookmark(id)} /></>
  return (
    <main className="app-shell">
      <header className="topbar"><div className={`status-dot ${snapshot.connection.status}`} /><div className="server-title"><strong>{snapshot.connection.server_name || 'TS3 服务器'}</strong><span>{snapshot.connection.server_address}</span></div><span className="status-text">{statusText(snapshot)}</span><button className="icon-button" onClick={() => setSettingsOpen(true)} aria-label="打开设置">⚙</button><button className="icon-button" onClick={() => void disconnect()} aria-label="断开连接">×</button></header>
      <div className="content-grid"><section className="panel channel-panel"><div className="panel-heading"><span>频道</span><span className="muted">{snapshot.channels.length}</span></div><ChannelTree channels={groupedChannels} onSelect={selectChannel} /></section><section className="panel chat-panel"><div className="tabs"><button className="tab active">当前频道</button><button className="tab">聊天</button></div><div className="chat-empty">频道聊天将在 M2 接入</div></section></div>
      <footer className="statusbar"><button
        className={`ptt-chip hold ${transmitting ? 'active' : ''}`}
        disabled={!connected}
        aria-label="按住说话"
        onPointerDown={(e) => { e.currentTarget.setPointerCapture(e.pointerId); setTransmit(true) }}
        onPointerUp={() => setTransmit(false)}
        onPointerCancel={() => setTransmit(false)}
        onLostPointerCapture={() => setTransmit(false)}
      >{transmitting ? '说话中…' : '按住说话'}</button><span className="latency">连接稳定</span></footer>
      {settingsOpen && <div className="settings-popover"><div className="panel-heading"><strong>设置</strong><button className="icon-button" onClick={() => setSettingsOpen(false)} aria-label="关闭设置">×</button></div><p>设置窗口占位入口，音频与悬浮窗将在后续任务接入。</p></div>}
      {pwdPrompt && (
        <ChannelPasswordModal
          name={pwdPrompt.name}
          error={pwdError}
          busy={pwdBusy}
          onSubmit={(password) => void submitChannelPassword(password)}
          onCancel={() => { setPwdPrompt(null); setPwdError('') }}
        />
      )}
      {noticeEl}
    </main>
  )
}
