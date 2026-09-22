import { useEffect, useMemo, useState, type ReactElement } from 'react'
import { tauriInvoke, subscribeRuntimeMissing, subscribeSnapshot } from './api'
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

function ConnectForm({ snapshot, onConnect, onSave }: { snapshot: AppSnapshot; onConnect: (address: string, nickname: string, password: string) => Promise<void>; onSave: (address: string, nickname: string, password: string) => Promise<void> }) {
  const [address, setAddress] = useState('saintbb1234.ts3.uno:9987')
  const [nickname, setNickname] = useState('MicaSpeak')
  const [password, setPassword] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
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
        {error && <div className="error-line">{error}</div>}
        <button className="primary-button" disabled={busy || !address || !nickname} onClick={() => void submit(false)}>{busy ? '正在连接…' : '连接服务器'}</button>
        <button className="link-button" disabled={busy} onClick={() => void submit(true)}>保存为书签</button>
      </div>
      <div className="bookmarks">
        <div className="section-label">书签</div>
        {snapshot.bookmarks.length === 0 ? <div className="empty-state compact">还没有保存的服务器</div> : snapshot.bookmarks.map((bookmark) => <BookmarkRow key={bookmark.id} bookmark={bookmark} onConnect={() => { setAddress(bookmark.address); setNickname(bookmark.nickname); void onConnect(bookmark.address, bookmark.nickname, '') }} />)}
      </div>
    </section>
  )
}

function BookmarkRow({ bookmark, onConnect }: { bookmark: Bookmark; onConnect: () => void }) {
  return <button className="bookmark-row" onDoubleClick={onConnect}><span className="bookmark-icon">★</span><span><strong>{bookmark.nickname}</strong><small>{bookmark.address}</small></span><span className="bookmark-arrow">›</span></button>
}

export default function App() {
  const [snapshot, setSnapshot] = useState<AppSnapshot>(emptySnapshot)
  const [started, setStarted] = useState(false)
  const [runtimeMissing, setRuntimeMissing] = useState(false)
  const [settingsOpen, setSettingsOpen] = useState(false)
  const [notice, setNotice] = useState('')

  useEffect(() => {
    let unlisten: (() => void) | undefined
    let active = true
    const start = async () => {
      try {
        const offRuntime = await subscribeRuntimeMissing(() => setRuntimeMissing(true))
        unlisten = () => offRuntime()
        const offSnapshot = await subscribeSnapshot((next) => active && setSnapshot(next))
        unlisten = () => { offSnapshot(); offRuntime() }
        const initial = await tauriInvoke<AppSnapshot>('get_app_snapshot')
        if (active) { setSnapshot(initial); setStarted(true) }
      } catch {
        if (active) { setSnapshot(fallbackSnapshot); setStarted(true) }
      }
    }
    void start()
    return () => { active = false; unlisten?.() }
  }, [])

  const connected = snapshot.connection.status === 'connected'
  const groupedChannels = useMemo(() => snapshot.channels, [snapshot.channels])
  const connect = async (address: string, nickname: string, password: string) => {
    await tauriInvoke('connect', { address, nickname, password: password || null })
  }
  const save = async (address: string, nickname: string, password: string) => {
    await tauriInvoke('save_bookmark', { address, nickname, password: password || null })
    setNotice('书签已保存'); setTimeout(() => setNotice(''), 1800)
  }
  const disconnect = async () => { await tauriInvoke('disconnect') }
  const selectChannel = async (channel: ChannelNode) => {
    try { await tauriInvoke('select_channel', { channelId: channel.id, password: null }) }
    catch (err) { setNotice(String(err).replace(/^Error:\s*/, '')) }
  }

  if (runtimeMissing || (started && !snapshot.runtime_available)) return <RuntimeMissing />
  if (!started) return <main className="loading-page"><div className="loader" /><span>正在启动 MicaSpeak…</span></main>
  if (!connected) return <ConnectForm snapshot={snapshot} onConnect={connect} onSave={save} />
  return (
    <main className="app-shell">
      <header className="topbar"><div className={`status-dot ${snapshot.connection.status}`} /><div className="server-title"><strong>{snapshot.connection.server_name || 'TS3 服务器'}</strong><span>{snapshot.connection.server_address}</span></div><span className="status-text">{statusText(snapshot)}</span><button className="icon-button" onClick={() => setSettingsOpen(true)} aria-label="打开设置">⚙</button><button className="icon-button" onClick={() => void disconnect()} aria-label="断开连接">×</button></header>
      <div className="content-grid"><section className="panel channel-panel"><div className="panel-heading"><span>频道</span><span className="muted">{snapshot.channels.length}</span></div><ChannelTree channels={groupedChannels} onSelect={selectChannel} /></section><section className="panel chat-panel"><div className="tabs"><button className="tab active">当前频道</button><button className="tab">聊天</button></div><div className="chat-empty">频道聊天将在 M2 接入</div></section></div>
      <footer className="statusbar"><span className="ptt-chip">麦克风待命</span><span className="latency">连接稳定</span></footer>
      {settingsOpen && <div className="settings-popover"><div className="panel-heading"><strong>设置</strong><button className="icon-button" onClick={() => setSettingsOpen(false)} aria-label="关闭设置">×</button></div><p>设置窗口占位入口，音频与悬浮窗将在后续任务接入。</p></div>}
      {notice && <div className="notice">{notice}</div>}
    </main>
  )
}
