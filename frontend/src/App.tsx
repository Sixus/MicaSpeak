import { useEffect, useMemo, useRef, useState, type ReactElement, type ReactNode } from 'react'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import {
  subscribeChatMessage,
  subscribeLevel,
  subscribeRuntimeMissing,
  subscribeSnapshot,
  subscribeTalking,
  subscribeUserError,
  tauriInvoke,
  type LevelPayload,
  type TalkingPayload,
} from './api'
import { emptySnapshot, type AppSnapshot, type Bookmark, type ChannelNode, type ChatMessageEvent, type ChatTabView, type ClientNode } from './types'

const fallbackSnapshot: AppSnapshot = {
  ...emptySnapshot,
  connection: { ...emptySnapshot.connection, reason: '请在 Tauri 窗口中运行 MicaSpeak' },
}

// ---- M3 A4：PTT 按键捕获（Windows 虚拟键码） ----

function jsEventToVk(e: KeyboardEvent): number | null {
  const loc = e.location // 1=左 2=右 3=小键盘
  if (e.keyCode === 16) return loc === 2 ? 0xa1 : 0xa0
  if (e.keyCode === 17) return loc === 2 ? 0xa3 : 0xa2
  if (e.keyCode === 18) return loc === 2 ? 0xa5 : 0xa4
  return e.keyCode > 0 ? e.keyCode : null
}

const VK_NAMES: Record<number, string> = {
  0x08: 'Backspace', 0x09: 'Tab', 0x0d: 'Enter', 0x13: 'Pause', 0x14: 'CapsLock', 0x1b: 'Esc',
  0x20: '空格', 0x21: 'PageUp', 0x22: 'PageDown', 0x23: 'End', 0x24: 'Home',
  0x25: '←', 0x26: '↑', 0x27: '→', 0x28: '↓', 0x2d: 'Insert', 0x2e: 'Delete',
  0x5b: '左Win', 0x5c: '右Win', 0x60: '小键盘0', 0x61: '小键盘1', 0x62: '小键盘2', 0x63: '小键盘3',
  0x64: '小键盘4', 0x65: '小键盘5', 0x66: '小键盘6', 0x67: '小键盘7', 0x68: '小键盘8', 0x69: '小键盘9',
  0x6a: '小键盘*', 0x6b: '小键盘+', 0x6d: '小键盘-', 0x6e: '小键盘.', 0x6f: '小键盘/',
  0xa0: '左Shift', 0xa1: '右Shift', 0xa2: '左Ctrl', 0xa3: '右Ctrl', 0xa4: '左Alt', 0xa5: '右Alt',
  0xba: ';', 0xbb: '=', 0xbc: ',', 0xbd: '-', 0xbe: '.', 0xbf: '/', 0xc0: '`',
  0xdb: '[', 0xdc: '\\', 0xdd: ']', 0xde: "'",
}

function vkName(vk: number): string {
  if (VK_NAMES[vk]) return VK_NAMES[vk]
  if (vk >= 0x70 && vk <= 0x87) return `F${vk - 0x6f}`
  if (vk >= 0x41 && vk <= 0x5a) return String.fromCharCode(vk)
  if (vk >= 0x30 && vk <= 0x39) return String(vk - 0x30)
  return `VK 0x${vk.toString(16).toUpperCase()}`
}

function SettingsPage({ snapshot, started, levels }: { snapshot: AppSnapshot; started: boolean; levels: LevelPayload }) {
  const [capturing, setCapturing] = useState(false)
  const [notice, setNotice] = useState('')
  const [devices, setDevices] = useState<{ inputs: string[]; outputs: string[] }>({ inputs: [], outputs: [] })
  const noticeTimer = useRef<number | undefined>(undefined)
  const showNotice = (message: string) => {
    setNotice(message)
    window.clearTimeout(noticeTimer.current)
    noticeTimer.current = window.setTimeout(() => setNotice(''), 3500)
  }
  const voice = snapshot.voice

  // 捕获模式：拦截下一个物理键（Esc 取消）。纯修饰键也允许（钩子路径的红利）。
  useEffect(() => {
    if (!capturing) return
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault()
      e.stopPropagation()
      if (e.key === 'Escape') {
        setCapturing(false)
        showNotice('已取消改绑')
        return
      }
      const vk = jsEventToVk(e)
      if (vk == null || vk === 0x1b) return
      setCapturing(false)
      tauriInvoke('set_ptt_key', { vk })
        .then(() => showNotice(`PTT 按键已改绑为 ${vkName(vk)}，旧按键已失效`))
        .catch((err) => showNotice(String(err).replace(/^Error:\s*/, '')))
    }
    window.addEventListener('keydown', onKey, true)
    return () => window.removeEventListener('keydown', onKey, true)
  }, [capturing])

  // 设备列表（挂载时拉一次；错误时静默保留空列表 + 提示）。
  useEffect(() => {
    tauriInvoke<{ inputs: string[]; outputs: string[] }>('list_audio_devices')
      .then(setDevices)
      .catch(() => {})
  }, [])

  const setMode = (mode: 'ptt' | 'vad') => {
    tauriInvoke('set_voice_mode', { mode, denoise: null })
      .then(() => showNotice(mode === 'vad' ? '已切换到自动语音检测（VAD）' : '已切换到按住说话（PTT）'))
      .catch((err) => showNotice(String(err).replace(/^Error:\s*/, '')))
  }
  const setDenoise = (denoise: boolean) => {
    tauriInvoke('set_voice_mode', { mode: null, denoise })
      .then(() => showNotice(denoise ? '软件降噪已开启' : '软件降噪已关闭'))
      .catch((err) => showNotice(String(err).replace(/^Error:\s*/, '')))
  }
  const setThreshold = (value: number) => {
    tauriInvoke('set_vad_threshold', { value })
      .catch((err) => showNotice(String(err).replace(/^Error:\s*/, '')))
  }
  const setDevice = (input: string | null, output: string | null) => {
    tauriInvoke('set_audio_devices', { input, output })
      .then(() => showNotice('音频设备已切换'))
      .catch((err) => showNotice(String(err).replace(/^Error:\s*/, '')))
  }

  if (!started) {
    return (
      <main className="loading-page"><div className="loader" /><span>正在加载设置…</span></main>
    )
  }
  const probPct = Math.min(100, Math.round((levels.prob || 0) * 100))
  return (
    <main className="settings-page">
      <header className="settings-header"><strong>设置</strong><span>语音与按键</span></header>

      <section className="settings-card">
        <div className="section-label">按住说话（PTT）</div>
        <div className="setting-row">
          <span className="setting-label">全局按键</span>
          {capturing ? (
            <button className="secondary-button capturing" type="button">请按下任意按键（Esc 取消）…</button>
          ) : (
            <button className="secondary-button" type="button" onClick={() => setCapturing(true)}>
              {vkName(voice.ptt_key_vk)}　更改…
            </button>
          )}
        </div>
        <div className={`setting-hint ${voice.hotkey_installed ? '' : 'setting-hint-bad'}`}>
          {voice.hotkey_installed
            ? `全局热键已生效：任意窗口按住 ${vkName(voice.ptt_key_vk)} 即可发送（含其他程序聚焦时）。`
            : '全局热键当前不可用（键盘钩子未安装）。仍可在主窗口按住按钮说话。'}
        </div>
        <div className="setting-hint">
          提示：当前台程序以更高权限运行（如管理员权限的程序/游戏）时，Windows 安全边界（UIPI）
          会阻止本应用接收键盘事件，全局 PTT 在这些窗口聚焦时不生效；这是系统限制，无法绕过。
        </div>
        <div className="setting-hint">本应用只读取 PTT 按键的按下与抬起，不记录任何按键内容。</div>
      </section>

      <section className="settings-card">
        <div className="section-label">语音模式</div>
        <div className="setting-row">
          <button
            className={`mode-chip ${voice.mode === 'ptt' ? 'active' : ''}`}
            type="button"
            onClick={() => setMode('ptt')}
          >按住说话（PTT）</button>
          <button
            className={`mode-chip ${voice.mode === 'vad' ? 'active' : ''}`}
            type="button"
            onClick={() => setMode('vad')}
          >自动语音检测（VAD）</button>
        </div>
        <div className="setting-hint">
          PTT：按住按键或界面按钮时发送。VAD：检测到语音自动发送，停止后约 0.25 秒补静音收尾。
        </div>
        {voice.mode === 'vad' && (
          <>
            <label className="setting-label slider-row">
              <span>触发阈值</span>
              <input
                type="range"
                min={0.1}
                max={0.9}
                step={0.05}
                value={voice.vad_threshold}
                onChange={(e) => setThreshold(Number(e.target.value))}
              />
              <span className="slider-value">{voice.vad_threshold.toFixed(2)}</span>
            </label>
            <div className="setting-row">
              <span className="setting-label">实时语音概率</span>
              <span className="level-meter prob-meter" aria-label="语音概率"><span className="level-fill" style={{ width: `${probPct}%` }} /></span>
            </div>
            <div className="setting-hint">概率高于阈值时自动发送。阈值越低越灵敏，也越容易误触发。</div>
          </>
        )}
        <label className="setting-row check-row">
          <input
            type="checkbox"
            checked={voice.denoise}
            onChange={(e) => setDenoise(e.target.checked)}
          />
          <span>软件降噪（nnnoiseless）</span>
        </label>
        <div className="setting-hint">
          降噪始终参与语音检测（VAD 概率来自降噪模型）；关闭降噪只影响发送的声音，不影响 VAD。
        </div>
      </section>

      <section className="settings-card">
        <div className="section-label">音频设备</div>
        <label className="setting-label select-row">
          <span>输入设备</span>
          <select
            value={voice.input_device ?? ''}
            onChange={(e) => setDevice(e.target.value || null, voice.output_device)}
          >
            <option value="">系统默认</option>
            {devices.inputs.map((d) => <option key={d} value={d}>{d}</option>)}
          </select>
        </label>
        <label className="setting-label select-row">
          <span>输出设备</span>
          <select
            value={voice.output_device ?? ''}
            onChange={(e) => setDevice(voice.input_device, e.target.value || null)}
          >
            <option value="">系统默认</option>
            {devices.outputs.map((d) => <option key={d} value={d}>{d}</option>)}
          </select>
        </label>
        <div className="setting-hint">切换只重建音频流，不会断开服务器连接。</div>
      </section>

      <section className="settings-card">
        <div className="section-label">系统降噪</div>
        <div className="setting-hint">不可用（本版本未启用系统级降噪探测，软件降噪不受影响）。</div>
      </section>

      {notice && <div className="notice">{notice}</div>}
    </main>
  )
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

export type ViewingTarget = { kind: ChatTabView['kind']; id: number }

function ChannelTree({ channels, talkingIds, onSelect, onClientContextMenu }: {
  channels: ChannelNode[]
  talkingIds: Set<number>
  onSelect: (channel: ChannelNode) => void
  onClientContextMenu: (client: ClientNode, x: number, y: number) => void
}) {
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
        <div
          className={`client-row ${client.is_self ? 'self' : ''} ${talkingIds.has(client.id) ? 'talking' : ''}`}
          key={`client-${client.id}`}
          style={{ paddingLeft: 26 + depth * 16 }}
          title={client.is_self ? undefined : '右键打开菜单'}
          onContextMenu={(e) => {
            if (client.is_self) return
            e.preventDefault()
            // 阻止冒泡，避免同一事件落到 window 的"点任意处关闭菜单"监听器
            e.stopPropagation()
            onClientContextMenu(client, e.clientX, e.clientY)
          }}
        >
          <span className="avatar">{client.name.slice(0, 1)}</span>
          <span>{client.name}{client.is_self ? '（我）' : ''}</span>
          {talkingIds.has(client.id) && <span className="speaking-dot" aria-label="正在说话" />}
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

// ---- M4a：频道聊天（事实状态在 Rust，这里只渲染快照并追加事件） ----

const URL_RE = /(https?:\/\/[^\s<>"'\]]+)/g

function renderMessageBody(text: string, onNotice: (message: string) => void): ReactNode[] {
  const parts: ReactNode[] = []
  let last = 0
  // BBCode（如 [b]、[url=…]）原样作为文本显示，仅 http/https 裸链接可点击
  for (const match of text.matchAll(URL_RE)) {
    const idx = match.index ?? 0
    if (idx > last) parts.push(text.slice(last, idx))
    const url = match[0]
    parts.push(
      <a
        key={`url-${idx}`}
        className="chat-link"
        href={url}
        title={url}
        onClick={(e) => {
          e.preventDefault()
          tauriInvoke('open_url', { url }).catch((err) => onNotice(String(err).replace(/^Error:\s*/, '')))
        }}
      >{url}</a>
    )
    last = idx + url.length
  }
  if (last < text.length) parts.push(text.slice(last))
  return parts
}

function formatTime(ms: number): string {
  const d = new Date(ms)
  return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
}

function ChatPanel({ tabs, ownChannelId, channels, viewing, setViewing, onNotice, onRemoveTab }: {
  tabs: ChatTabView[]
  ownChannelId: number
  channels: ChannelNode[]
  viewing: ViewingTarget | null
  setViewing: (target: ViewingTarget | null) => void
  onNotice: (message: string) => void
  onRemoveTab: (target: ViewingTarget) => void
}) {
  const activeTarget: ViewingTarget = viewing ?? { kind: 'channel', id: ownChannelId }
  const activeTab = tabs.find((t) => t.kind === activeTarget.kind && t.target_id === activeTarget.id) ?? null
  const isPrivateView = activeTarget.kind === 'private'
  const viewingOtherChannel = activeTarget.kind === 'channel' && activeTarget.id !== ownChannelId
  const clientName = (id: number) => channels.flatMap((c) => c.clients).find((c) => c.id === id)?.name
  const channelName = (id: number) => channels.find((c) => c.id === id)?.name
  const activeTitle = activeTab
    ? (activeTarget.kind === 'private'
        ? (clientName(activeTarget.id) ?? (activeTab.title || `用户 ${activeTarget.id}`))
        : (channelName(activeTarget.id) ?? (activeTab.title || '频道')))
    : (channelName(ownChannelId) ?? '频道')

  // 滚动：贴底时自动滚底；上翻暂停，回底恢复
  const listRef = useRef<HTMLDivElement>(null)
  const atBottomRef = useRef(true)
  const [detached, setDetached] = useState(false)
  const messages = activeTab?.messages ?? []
  useEffect(() => {
    if (atBottomRef.current && listRef.current) {
      const el = listRef.current
      requestAnimationFrame(() => { el.scrollTop = el.scrollHeight })
    }
  }, [messages.length, activeTarget.kind, activeTarget.id])

  // 切换频道后回到跟随当前频道（私聊查看不受频道切换影响）
  useEffect(() => {
    if (viewing?.kind === 'channel') setViewing(null)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ownChannelId])

  const onScroll = () => {
    const el = listRef.current
    if (!el) return
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 8
    atBottomRef.current = atBottom
    setDetached(!atBottom)
  }
  const scrollToBottom = () => {
    const el = listRef.current
    if (!el) return
    el.scrollTop = el.scrollHeight
    atBottomRef.current = true
    setDetached(false)
  }

  const [draft, setDraft] = useState('')
  const [sending, setSending] = useState(false)
  const send = async () => {
    const text = draft.trim()
    if (!text || sending || viewingOtherChannel) return
    const canSend = !isPrivateView || clientName(activeTarget.id)
    if (!canSend) { onNotice('该用户已不在服务器上，无法发送'); return }
    setSending(true)
    try {
      if (isPrivateView) {
        await tauriInvoke('send_private_message', { clientId: activeTarget.id, text })
      } else {
        await tauriInvoke('send_channel_message', { text })
      }
      setDraft('')
      atBottomRef.current = true
    } catch (err) {
      onNotice(String(err).replace(/^Error:\s*/, ''))
    } finally {
      setSending(false)
    }
  }

  const selectTab = (tab: ChatTabView) => {
    if (tab.kind === 'private') {
      // 打开即告知 Rust（标记正在查看、清未读）
      tauriInvoke('open_private_chat', { clientId: tab.target_id })
        .catch((err) => onNotice(String(err).replace(/^Error:\s*/, '')))
    }
    setViewing({ kind: tab.kind, id: tab.target_id })
  }
  const closeTab = (tab: ChatTabView) => {
    tauriInvoke('close_chat_tab', { kind: tab.kind, targetId: tab.target_id })
      .catch((err) => onNotice(String(err).replace(/^Error:\s*/, '')))
    onRemoveTab({ kind: tab.kind, id: tab.target_id })
    if (activeTarget.kind === tab.kind && activeTarget.id === tab.target_id) setViewing(null)
  }

  const channelTabs = tabs.filter((t) => t.kind === 'channel')
  const privateTabs = tabs.filter((t) => t.kind === 'private')
  return (
    <section className="panel chat-panel">
      <div className="tabs" role="tablist">
        {channelTabs.map((tab) => {
          const active = tab.kind === activeTarget.kind && tab.target_id === activeTarget.id
          const title = channelName(tab.target_id) ?? (tab.title || `频道 ${tab.target_id}`)
          return (
            <button
              key={`tab-${tab.kind}-${tab.target_id}`}
              className={`tab ${active ? 'active' : ''}`}
              role="tab"
              aria-selected={active}
              onClick={() => selectTab(tab)}
            >
              {title}
              {tab.unread > 0 && <span className="tab-unread" aria-label={`${tab.unread} 条未读`} />}
            </button>
          )
        })}
        {privateTabs.map((tab) => {
          const active = tab.kind === activeTarget.kind && tab.target_id === activeTarget.id
          const title = clientName(tab.target_id) ?? (tab.title || `用户 ${tab.target_id}`)
          return (
            <span key={`tab-${tab.kind}-${tab.target_id}`} className={`tab tab-private ${active ? 'active' : ''}`} role="tab" aria-selected={active}>
              <button className="tab-main" onClick={() => selectTab(tab)}>
                {title}
                {tab.unread > 0 && <span className="tab-unread" aria-label={`${tab.unread} 条未读`} />}
              </button>
              <button className="tab-close" aria-label={`关闭与 ${title} 的私聊`} onClick={() => closeTab(tab)}>×</button>
            </span>
          )
        })}
        {!channelTabs.length && !privateTabs.length && <button className="tab active">频道</button>}
      </div>
      <div className="chat-messages" ref={listRef} onScroll={onScroll}>
        {messages.map((m) => (
          <div className="chat-row" key={m.id}>
            <span className="chat-time">{formatTime(m.time_ms)}</span>
            <span className={`chat-nick ${m.is_self ? 'self' : ''}`}>{m.from_name}{m.is_self ? '（我）' : ''}</span>
            <span className="chat-text">{renderMessageBody(m.text, onNotice)}</span>
          </div>
        ))}
        {!messages.length && <div className="chat-empty">{isPrivateView ? `与 ${activeTitle} 的私聊，发送第一条消息吧` : '暂无消息，回车即可发送第一条'}</div>}
      </div>
      {detached && (
        <button className="chat-back-bottom" onClick={scrollToBottom}>↓ 回到底部</button>
      )}
      <div className="chat-composer">
        <input
          className="chat-input"
          value={draft}
          disabled={viewingOtherChannel}
          placeholder={viewingOtherChannel
            ? '正在查看其他频道，点击当前频道标签后可发送'
            : isPrivateView
              ? `发送私聊到 ${activeTitle}…`
              : `发送消息到 ${activeTitle}…`}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && !e.nativeEvent.isComposing) {
              e.preventDefault()
              void send()
            }
          }}
        />
        <button
          className="chat-send"
          aria-label="发送消息"
          disabled={!draft.trim() || sending}
          onClick={() => void send()}
        >➤</button>
      </div>
    </section>
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
        <button
          className="settings-link"
          onClick={() => void tauriInvoke('open_settings').catch((err) => { setError(String(err).replace(/^Error:\s*/, '')) })}
        >⚙ 设置</button>
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
  const [notice, setNotice] = useState('')
  const [pwdPrompt, setPwdPrompt] = useState<{ id: number; name: string } | null>(null)
  const [pwdError, setPwdError] = useState('')
  const [pwdBusy, setPwdBusy] = useState(false)
  // M2c：说话人状态与电平（只有 id/名字/电平标量，不放原始音频数据）
  const [talking, setTalking] = useState<Map<number, string>>(new Map())
  const [levels, setLevels] = useState<LevelPayload>({ mic: 0, out: 0, prob: 0 })
  // M4a：聊天标签（快照整体校正 + chat://message 增量按消息 id 去重追加）
  const [chatTabs, setChatTabs] = useState<ChatTabView[]>([])
  // M4b：正在查看的聊天标签（null=跟随当前频道）与频道树右键菜单
  const [viewing, setViewing] = useState<ViewingTarget | null>(null)
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number; client: ClientNode } | null>(null)
  const promptRef = useRef(pwdPrompt)
  promptRef.current = pwdPrompt
  // M3 A4：设置窗口复用同一 React 应用与后端状态，按窗口标签路由。
  const isSettingsWindow = useMemo(() => {
    try { return getCurrentWebviewWindow().label === 'settings' } catch { return false }
  }, [])

  useEffect(() => {
    let active = true
    const offs: Array<() => void> = []
    const start = async () => {
      try {
        offs.push(await subscribeRuntimeMissing(() => setRuntimeMissing(true)))
        offs.push(await subscribeSnapshot((next) => {
          if (!active) return
          setSnapshot(next)
          setChatTabs(next.chat)
          // 快照是权威状态：用快照里的说话列表整体校正（覆盖漏收的事件）
          setTalking(new Map(next.talking.map((t) => [t.client_id, t.name])))
        }))
        offs.push(await subscribeChatMessage((payload) => {
          if (!active) return
          setChatTabs((prev) => {
            const idx = prev.findIndex((t) => t.kind === payload.kind && t.target_id === payload.target_id)
            if (idx === -1) {
              return [...prev, {
                kind: payload.kind,
                target_id: payload.target_id,
                title: '',
                unread: payload.unread,
                messages: [payload.message],
              }]
            }
            const tab = prev[idx]
            if (tab.messages.some((m) => m.id === payload.message.id)) return prev
            const merged: ChatTabView = {
              ...tab,
              unread: payload.unread,
              messages: [...tab.messages, payload.message],
            }
            const next = prev.slice()
            next[idx] = merged
            return next
          })
        }))
        offs.push(await subscribeUserError((message) => {
          if (!active) return
          if (promptRef.current) setPwdError(message)
          else { setNotice(message); setTimeout(() => setNotice(''), 4000) }
        }))
        offs.push(await subscribeTalking((p) => {
          if (!active) return
          setTalking((prev) => {
            const next = new Map(prev)
            if (p.talking) next.set(p.client_id, p.name)
            else next.delete(p.client_id)
            return next
          })
        }))
        offs.push(await subscribeLevel((l) => { if (active) setLevels(l) }))
        const initial = await tauriInvoke<AppSnapshot>('get_app_snapshot')
        if (active) {
          setSnapshot(initial)
          setChatTabs(initial.chat)
          setTalking(new Map(initial.talking.map((t) => [t.client_id, t.name])))
          setStarted(true)
        }
      } catch {
        if (active) { setSnapshot(fallbackSnapshot); setStarted(true) }
      }
    }
    void start()
    return () => { active = false; offs.forEach((off) => off()) }
  }, [])

  const connected = snapshot.connection.status === 'connected'
  const groupedChannels = useMemo(() => snapshot.channels, [snapshot.channels])
  const talkingIds = useMemo(() => new Set(talking.keys()), [talking])
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
  // M4b：右键菜单的"私聊"入口——打开/创建私聊标签并切换查看。
  const openPrivateChat = async (clientId: number) => {
    try {
      await tauriInvoke('open_private_chat', { clientId })
      setViewing({ kind: 'private', id: clientId })
    } catch (err) {
      setNotice(String(err).replace(/^Error:\s*/, ''))
      setTimeout(() => setNotice(''), 3000)
    }
  }
  // 右键菜单：点击任意处或 Escape 关闭
  useEffect(() => {
    if (!ctxMenu) return
    const close = () => setCtxMenu(null)
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') setCtxMenu(null) }
    window.addEventListener('click', close)
    window.addEventListener('contextmenu', close)
    window.addEventListener('keydown', onKey)
    return () => {
      window.removeEventListener('click', close)
      window.removeEventListener('contextmenu', close)
      window.removeEventListener('keydown', onKey)
    }
  }, [ctxMenu])
  const removeChatTab = (target: ViewingTarget) => {
    setChatTabs((prev) => prev.filter((t) => !(t.kind === target.kind && t.target_id === target.id)))
  }

  if (runtimeMissing || (started && !snapshot.runtime_available)) return <RuntimeMissing />
  if (isSettingsWindow) return <SettingsPage snapshot={snapshot} started={started} levels={levels} />
  if (!started) return <main className="loading-page"><div className="loader" /><span>正在启动 MicaSpeak…</span></main>
  const noticeEl = notice ? <div className="notice">{notice}</div> : null
  if (!connected) return <>{noticeEl}<ConnectForm snapshot={snapshot} onConnect={connect} onConnectBookmark={(b) => void connectBookmark(b)} onSave={save} onDelete={(id) => void removeBookmark(id)} /></>

  return (
    <main className="app-shell">
      <header className="topbar"><div className={`status-dot ${snapshot.connection.status}`} /><div className="server-title"><strong>{snapshot.connection.server_name || 'TS3 服务器'}</strong><span>{snapshot.connection.server_address}</span></div><span className="status-text">{statusText(snapshot)}</span><button className="icon-button" onClick={() => void tauriInvoke('open_settings').catch((err) => { setNotice(String(err).replace(/^Error:\s*/, '')); setTimeout(() => setNotice(''), 3000) })} aria-label="打开设置">⚙</button><button className="icon-button" onClick={() => void disconnect()} aria-label="断开连接">×</button></header>
      <div className="content-grid"><section className="panel channel-panel"><div className="panel-heading"><span>频道</span><span className="muted">{snapshot.channels.length}</span></div><ChannelTree channels={groupedChannels} talkingIds={talkingIds} onSelect={selectChannel} onClientContextMenu={(client, x, y) => setCtxMenu({ x, y, client })} /></section><ChatPanel tabs={chatTabs} ownChannelId={snapshot.own_channel_id} channels={groupedChannels} viewing={viewing} setViewing={setViewing} onNotice={(message) => { setNotice(message); setTimeout(() => setNotice(''), 3000) }} onRemoveTab={removeChatTab} /></div>
      {ctxMenu && (
        <div className="ctx-menu" style={{ left: ctxMenu.x, top: ctxMenu.y }} role="menu">
          <button role="menuitem" onClick={() => { const id = ctxMenu.client.id; setCtxMenu(null); void openPrivateChat(id) }}>
            💬 私聊（{ctxMenu.client.name}）
          </button>
        </div>
      )}
      <footer className="statusbar"><button
        className={`ptt-chip hold ${transmitting ? 'active' : ''}`}
        disabled={!connected}
        aria-label="按住说话"
        onPointerDown={(e) => { e.currentTarget.setPointerCapture(e.pointerId); setTransmit(true) }}
        onPointerUp={() => setTransmit(false)}
        onPointerCancel={() => setTransmit(false)}
        onLostPointerCapture={() => setTransmit(false)}
      >{transmitting ? '说话中…' : '按住说话'}</button><span className="level-meter" aria-label="麦克风电平"><span className="level-fill" style={{ width: `${Math.min(100, Math.round(levels.mic * 300))}%` }} /></span><span className="talkers">{talking.size ? [...talking.values()].join('、') : '无人说话'}</span></footer>
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
