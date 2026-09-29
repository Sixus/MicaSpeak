import { useEffect, useMemo, useRef, useState, type ReactElement, type ReactNode } from 'react'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import {
  subscribeChatMessage,
  subscribeDeviceError,
  subscribeIdentityLevelRequired,
  subscribeIdentityLevelUp,
  subscribeLevel,
  subscribeOpenSettings,
  subscribeOverlayState,
  subscribeRuntimeMissing,
  subscribeSnapshot,
  subscribeTalking,
  subscribeUserError,
  tauriInvoke,
  type LevelPayload,
  type LevelRequiredPayload,
  type LevelUpPayload,
  type OverlayStatePayload,
  type TalkingPayload,
} from './api'
import { emptySnapshot, type AppSnapshot, type Bookmark, type ChannelNode, type ChatMessageEvent, type ChatTabView, type ClientNode, type IdentityView } from './types'

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

// ---- M6a 身份管理（设置页"身份"卡）：私钥只经 Rust 存取，这里只拿脱敏视图 ----

// 通用确认弹窗（删除身份/导出警告共用）。
function ConfirmModal({ title, body, confirmText, danger, busy, onConfirm, onCancel }: {
  title: string
  body: ReactNode
  confirmText: string
  danger?: boolean
  busy?: boolean
  onConfirm: () => void
  onCancel: () => void
}) {
  return (
    <div className="modal-overlay" onClick={onCancel}>
      <div className="modal-card" onClick={(e) => e.stopPropagation()}>
        <strong>{title}</strong>
        <div className="modal-body">{body}</div>
        <div className="modal-actions">
          <button className="secondary-button" onClick={onCancel} disabled={busy}>取消</button>
          <button className={danger ? 'danger-button' : 'primary-button'} onClick={onConfirm} disabled={busy}>{confirmText}</button>
        </div>
      </div>
    </div>
  )
}

// 导入弹窗：多行不回显输入（-webkit-text-security），提交后立即清空明文。
function ImportIdentityModal({ busy, error, onSubmit, onCancel }: {
  busy: boolean
  error: string
  onSubmit: (data: string) => void
  onCancel: () => void
}) {
  const [data, setData] = useState('')
  const inputRef = useRef<HTMLTextAreaElement>(null)
  useEffect(() => { inputRef.current?.focus() }, [])
  return (
    <div className="modal-overlay" onClick={onCancel}>
      <div className="modal-card modal-wide" onClick={(e) => e.stopPropagation()}>
        <strong>导入身份</strong>
        <div className="modal-body">
          <textarea
            ref={inputRef}
            className="secret-input"
            rows={4}
            value={data}
            placeholder="粘贴官方客户端“无密码”导出的身份字符串（内容不回显）"
            onChange={(e) => setData(e.target.value)}
            disabled={busy}
          />
          <div className="setting-hint">仅支持官方客户端导出时密码留空的格式；带密码导出的字符串无法导入。提交后明文立即从界面丢弃。</div>
          {error && <div className="error-line">{error}</div>}
        </div>
        <div className="modal-actions">
          <button className="secondary-button" onClick={onCancel} disabled={busy}>取消</button>
          <button className="primary-button" disabled={busy || !data.trim()} onClick={() => { const text = data; setData(''); onSubmit(text) }}>
            {busy ? '正在导入…' : '导入'}
          </button>
        </div>
      </div>
    </div>
  )
}

// 导出结果弹窗：含私钥，仅用户确认后出现；提供一键全选复制。
function ExportIdentityModal({ secret, onDone }: { secret: string; onDone: () => void }) {
  const areaRef = useRef<HTMLTextAreaElement>(null)
  const copy = () => {
    const area = areaRef.current
    if (!area) return
    area.focus(); area.select()
    if (document.execCommand('copy')) window.alert('已复制到剪贴板。注意：剪贴板内容含私钥，粘贴后请及时覆盖。')
  }
  return (
    <div className="modal-overlay" onClick={onDone}>
      <div className="modal-card modal-wide" onClick={(e) => e.stopPropagation()}>
        <strong>身份备份字符串</strong>
        <div className="modal-body">
          <div className="error-line">以下内容包含完整私钥：任何拿到它的人都能以你的身份登录。勿外传、勿截图、勿上传网盘。</div>
          <textarea ref={areaRef} rows={4} readOnly value={secret} onFocus={(e) => e.currentTarget.select()} />
        </div>
        <div className="modal-actions">
          <button className="secondary-button" onClick={copy}>复制</button>
          <button className="primary-button" onClick={onDone}>完成</button>
        </div>
      </div>
    </div>
  )
}

function IdentitySection({ snapshot, showNotice }: { snapshot: AppSnapshot; showNotice: (message: string) => void }) {
  const identities = snapshot.identities ?? []
  const [importing, setImporting] = useState(false)
  const [importBusy, setImportBusy] = useState(false)
  const [importError, setImportError] = useState('')
  const [exporting, setExporting] = useState<IdentityView | null>(null)
  const [exportSecret, setExportSecret] = useState<string | null>(null)
  const [deleting, setDeleting] = useState<IdentityView | null>(null)
  const [busy, setBusy] = useState(false)

  const run = async (action: () => Promise<string | void>) => {
    setBusy(true)
    try {
      const message = await action()
      if (typeof message === 'string') showNotice(message)
    } catch (err) {
      showNotice(String(err).replace(/^Error:\s*/, ''))
    } finally {
      setBusy(false)
    }
  }
  const submitImport = (data: string) => {
    setImportBusy(true); setImportError('')
    tauriInvoke<IdentityView>('import_identity', { data, label: null })
      .then((view) => {
        setImporting(false)
        showNotice(`身份「${view.label}」导入成功（等级 ${view.level}）`)
      })
      .catch((err) => setImportError(String(err).replace(/^Error:\s*/, '')))
      .finally(() => setImportBusy(false))
  }
  return (
    <section className="settings-card">
      <div className="section-label">身份</div>
      {!identities.length && <div className="empty-state compact">还没有身份：新建一个，或从官方客户端导入</div>}
      {identities.map((identity) => (
        <div className="identity-row" key={identity.id}>
          <div className="identity-info">
            <strong>{identity.label}{identity.active && <span className="identity-active">使用中</span>}</strong>
            <small>Unique ID {identity.uid_masked} · 安全等级 {identity.level}</small>
          </div>
          <div className="identity-actions">
            {!identity.active && (
              <button className="secondary-button" disabled={busy}
                onClick={() => void run(() => tauriInvoke<string>('set_active_identity', { id: identity.id }))}>
                设为当前
              </button>
            )}
            <button className="secondary-button" disabled={busy}
              onClick={() => { setExporting(identity) }}>导出…</button>
            <button className="secondary-button identity-delete" disabled={busy}
              onClick={() => { setDeleting(identity) }}>删除</button>
          </div>
        </div>
      ))}
      <div className="identity-toolbar">
        <button className="secondary-button" disabled={busy} onClick={() => { setImportError(''); setImporting(true) }}>导入身份…</button>
        <button className="secondary-button" disabled={busy}
          onClick={() => void run(() => tauriInvoke<IdentityView>('create_identity', { label: null }).then((view) => `已新建身份「${view.label}」`))}>新建身份</button>
      </div>
      <div className="setting-hint">切换身份后需重新连接服务器才会生效。身份文件（含私钥）只保存在本机 Data\identities\，不会上传。</div>

      {importing && (
        <ImportIdentityModal busy={importBusy} error={importError}
          onSubmit={submitImport}
          onCancel={() => { setImporting(false); setImportError('') }} />
      )}
      {exporting && exportSecret == null && (
        <ConfirmModal title={`导出「${exporting.label}」？`}
          body="导出将显示完整私钥字符串：任何拿到它的人都能以你的身份登录服务器。请确认周围无人、不录屏。"
          confirmText="显示私钥" danger busy={busy}
          onCancel={() => setExporting(null)}
          onConfirm={() => {
            void run(() => tauriInvoke<string>('export_identity', { id: exporting.id }).then((secret) => {
              setExportSecret(secret)
            }))
          }} />
      )}
      {exportSecret != null && (
        <ExportIdentityModal secret={exportSecret} onDone={() => { setExportSecret(null); setExporting(null) }} />
      )}
      {deleting && (
        <ConfirmModal title={`删除身份「${deleting.label}」？`}
          body={`将删除 ${deleting.label}（Unique ID ${deleting.uid_masked}）。此身份在服务器上的权限与等级绑定将不再可用。`}
          confirmText="删除" danger busy={busy}
          onCancel={() => setDeleting(null)}
          onConfirm={() => {
            void run(() => tauriInvoke('delete_identity', { id: deleting.id }).then(() => '身份已删除'))
            setDeleting(null)
          }} />
      )}
    </section>
  )
}

// ---- M6c 关于卡：非官方声明、版本、构建日期、许可入口、WebView2 前置 ----

type AboutInfo = {
  version: string
  build_date: string
  webview2_version: string | null
  webview2_available: boolean
  disclaimer: string
}

function AboutSection({ snapshot, showNotice }: { snapshot: AppSnapshot; showNotice: (message: string) => void }) {
  const [about, setAbout] = useState<AboutInfo | null>(null)
  useEffect(() => {
    let active = true
    tauriInvoke<AboutInfo>('get_about_info')
      .then((info) => { if (active) setAbout(info) })
      .catch(() => {})
    return () => { active = false }
  }, [])
  const webview2 = about?.webview2_version ?? snapshot.webview2_version ?? null
  return (
    <section className="settings-card">
      <div className="section-label">关于</div>
      <div className="about-row"><span className="setting-label">版本</span><span>{about ? `${about.version}（构建于 ${about.build_date}）` : '…'}</span></div>
      <div className="about-row"><span className="setting-label">WebView2</span><span>{webview2 ? `Evergreen ${webview2}` : '未检测到'}</span></div>
      <div className="setting-hint">
        本应用使用系统级 Evergreen WebView2 运行时渲染界面（不随应用捆绑）。缺失时会显示安装引导页；
        可在 <a className="chat-link" href="https://developer.microsoft.com/microsoft-edge/webview2/" target="_blank" rel="noreferrer">微软官网</a> 检查或安装。
      </div>
      <div className="setting-hint">{about?.disclaimer ?? 'MicaSpeak 是非官方的 TeamSpeak 3 第三方客户端，与 TeamSpeak Systems GmbH 无关联。'}</div>
      <div className="setting-hint">本应用仅支持 TeamSpeak 3 服务器（TS5/TS6 不在支持范围）。</div>
      <div className="identity-toolbar">
        <button className="secondary-button"
          onClick={() => tauriInvoke('open_licenses').catch((err) => showNotice(String(err).replace(/^Error:\s*/, '')))}>
          查看第三方许可（LICENSES）
        </button>
      </div>
    </section>
  )
}

// embedded：嵌入主窗口内容区（连接中，返回图标在顶栏原服务器信息位，
// 设置视图不显示服务器信息）；未连接时整页显示，backButton 为左上角常驻返回图标。
function SettingsPage({ snapshot, started, levels, backButton }: { snapshot: AppSnapshot; started: boolean; levels: LevelPayload; backButton?: ReactNode }) {
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
      {backButton}
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

      <section className="settings-card">
        <div className="section-label">悬浮窗</div>
        <label className="setting-row check-row">
          <input
            type="checkbox"
            checked={snapshot.overlay_enabled}
            onChange={(e) => {
              const enabled = e.target.checked
              tauriInvoke('set_overlay_enabled', { enabled })
                .then(() => showNotice(enabled ? '悬浮窗已开启' : '悬浮窗已关闭'))
                .catch((err) => showNotice(String(err).replace(/^Error:\s*/, '')))
            }}
          />
          <span>说话时显示悬浮窗</span>
        </label>
        <div className="setting-row">
          <span className="setting-label">位置</span>
          <button
            className="secondary-button"
            disabled={!snapshot.overlay_enabled}
            onClick={() => tauriInvoke('set_overlay_editing', { editing: true })
              .then(() => showNotice('已进入编辑模式：拖动悬浮窗把手调整位置，点悬浮窗上的"完成"保存'))
              .catch((err) => showNotice(String(err).replace(/^Error:\s*/, '')))
            }
          >编辑位置…</button>
        </div>
        <div className="setting-hint">
          有人说话时显示昵称，停止约 1 秒后隐藏；位置改动会自动保存。独占全屏的游戏画面中悬浮窗不可见（已知限制）。
        </div>
      </section>

      <IdentitySection snapshot={snapshot} showNotice={showNotice} />

      <AboutSection snapshot={snapshot} showNotice={showNotice} />

      <section className="settings-card">
        <div className="section-label">外观</div>
        <div className="setting-row">
          <span className="setting-label">窗口材质</span>
          <span className="material-value">{snapshot.material ?? '实体'}</span>
        </div>
        <div className="setting-hint">
          Windows 11 上使用 Mica 系统材质；不支持或启用失败时自动回退实体背景。启动参数 --force-fallback 可强制实体。
        </div>
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

// M6b：安全等级横幅——lib 自动升级（increasing/progress/increased）与
// 手动升级任务（manual-*）共用；计时在事件值基础上每秒本地续走。
function LevelUpBanner({ levelUp, levelRequired, onDismiss, onStartUpgrade, onCancelUpgrade }: {
  levelUp: LevelUpPayload | null
  levelRequired: LevelRequiredPayload | null
  onDismiss: () => void
  onStartUpgrade: () => void
  onCancelUpgrade: () => void
}) {
  const [, tick] = useState(0)
  const active = levelUp != null || levelRequired != null
  useEffect(() => {
    if (!active) return
    const timer = window.setInterval(() => tick((n) => n + 1), 1000)
    return () => window.clearInterval(timer)
  }, [active])
  if (levelUp) {
    const elapsedBase = levelUp.elapsed_ms ?? 0
    const seconds = Math.floor(elapsedBase / 1000)
    if (levelUp.phase === 'increasing' || levelUp.phase === 'progress') {
      return (
        <div className="levelup-banner" role="status">
          <strong>正在提升安全等级…</strong>
          <span>服务器要求 {levelUp.required} 级，身份等级不足，后台正在计算（已用时 {formatElapsed(seconds)}）。完成后会自动重连，期间界面可正常操作。</span>
        </div>
      )
    }
    if (levelUp.phase === 'increased') {
      return (
        <div className="levelup-banner" role="status">
          <strong>安全等级已达标</strong>
          <span>正在用升级后的身份重新连接服务器…</span>
        </div>
      )
    }
    if (levelUp.phase === 'manual-start' || levelUp.phase === 'manual-progress') {
      return (
        <div className="levelup-banner" role="status">
          <strong>正在提升安全等级（{levelUp.current}/{levelUp.target} 级）</strong>
          <span>后台计算中，已用时 {formatElapsed(seconds)}；等级越高耗时越长，可随时取消（已完成等级会保留）。</span>
          <div className="modal-actions">
            <button className="secondary-button" onClick={onCancelUpgrade}>取消升级</button>
          </div>
        </div>
      )
    }
    if (levelUp.phase === 'manual-done') {
      return (
        <div className="levelup-banner" role="status">
          <strong>安全等级已提升到 {levelUp.target} 级</strong>
          <span>用时 {formatElapsed(seconds)}，正在重新连接服务器并回到原频道…</span>
        </div>
      )
    }
    if (levelUp.phase === 'manual-failed') {
      return (
        <div className="levelup-banner levelup-bad" role="alert">
          <strong>安全等级提升失败</strong>
          <span>{levelUp.error ?? '未知错误'}</span>
          <div className="modal-actions"><button className="secondary-button" onClick={onDismiss}>关闭</button></div>
        </div>
      )
    }
    return null
  }
  if (levelRequired) {
    return (
      <div className="levelup-banner levelup-bad" role="alert">
        <strong>需要更高的安全等级</strong>
        <span>此服务器要求 {levelRequired.required} 级，当前身份 {levelRequired.have} 级。提升可能耗时较长（等级越高越久），确认后开始。</span>
        <div className="modal-actions">
          <button className="secondary-button" onClick={onDismiss}>暂不升级</button>
          <button className="primary-button" onClick={onStartUpgrade}>提升安全等级</button>
        </div>
      </div>
    )
  }
  return null
}

function formatElapsed(seconds: number): string {
  if (seconds < 60) return `${seconds} 秒`
  const minutes = Math.floor(seconds / 60)
  return `${minutes} 分 ${seconds % 60} 秒`
}

// 默认服务器地址：仅开发构建预填（不含端口，tsclientlib 缺省 9987）；
// 发行构建（pnpm build / 打包）自动清空，避免泄露测试服务器（第 10 项）。
const DEFAULT_ADDRESS = import.meta.env.DEV ? 'saintbb1234.ts3.uno' : ''

function ConnectForm({ snapshot, levelUp, levelRequired, onConnect, onConnectBookmark, onSave, onDelete, onDismissLevel, onStartUpgrade, onCancelUpgrade, onOpenSettings }: {
  snapshot: AppSnapshot
  levelUp: LevelUpPayload | null
  levelRequired: LevelRequiredPayload | null
  onConnect: (address: string, nickname: string, password: string) => Promise<void>
  onConnectBookmark: (bookmark: Bookmark) => void
  onSave: (address: string, nickname: string, password: string) => Promise<void>
  onDelete: (id: string) => void
  onDismissLevel: () => void
  onStartUpgrade: () => void
  onCancelUpgrade: () => void
  onOpenSettings: () => void
}) {
  const [address, setAddress] = useState(DEFAULT_ADDRESS)
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
  // M5d：统一回车行为——连接页任意输入框回车即连接（与聊天/密码弹窗一致）。
  const onEnterConnect = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !busy && !connecting && address && nickname) {
      e.preventDefault()
      void submit(false)
    }
  }
  return (
    <section className="connect-page">
      <div className="brand-lockup"><div className="brand-mark">M</div><div><strong>MicaSpeak</strong><span>TS3 语音客户端</span></div></div>
      <div className="form-card">
        <LevelUpBanner
          levelUp={levelUp}
          levelRequired={levelRequired}
          onDismiss={onDismissLevel}
          onStartUpgrade={onStartUpgrade}
          onCancelUpgrade={onCancelUpgrade}
        />
        <label>服务器地址<input value={address} onChange={(e) => setAddress(e.target.value)} onKeyDown={onEnterConnect} placeholder="服务器地址（端口可省略，默认 9987）" /></label>
        <label>昵称<input value={nickname} onChange={(e) => setNickname(e.target.value)} onKeyDown={onEnterConnect} placeholder="你的昵称" /></label>
        <label>服务器密码 <span className="optional">可选</span><input type="password" value={password} onChange={(e) => setPassword(e.target.value)} onKeyDown={onEnterConnect} /></label>
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
          onClick={onOpenSettings}
        >⚙ 设置</button>
      </div>
    </section>
  )
}

function BookmarkRow({ bookmark, onConnect, onDelete }: { bookmark: Bookmark; onConnect: () => void; onDelete: () => void }) {
  return (
    <div className="bookmark-row">
      <button className="bookmark-main" onClick={onConnect} title="单击连接">
        <span className="bookmark-icon">★</span>
        <span className="bookmark-text"><strong>{bookmark.nickname}</strong><small>{bookmark.address}</small></span>
        <span className="bookmark-arrow">›</span>
      </button>
      <button className="bookmark-delete" aria-label={`删除书签 ${bookmark.nickname}`} title="删除书签" onClick={onDelete}>×</button>
    </div>
  )
}

// M4c：按窗口标签路由。悬浮窗页面只订阅 overlay://state，
// 不挂主窗口的全套订阅（任务卡 C1）。
export default function App() {
  const isOverlay = useMemo(() => {
    try { return getCurrentWebviewWindow().label === 'overlay' } catch { return false }
  }, [])
  // UI 改版：屏蔽 WebView 默认右键菜单（用户列表右键私聊走自绘菜单不受影响）。
  // 输入框/文本域/下拉保留系统菜单，方便右键复制粘贴（Esc/Ctrl+V 均不受影响）。
  useEffect(() => {
    const onContextMenu = (e: MouseEvent) => {
      const target = e.target
      if (target instanceof Element && target.closest('input, textarea, select')) return
      e.preventDefault()
    }
    window.addEventListener('contextmenu', onContextMenu)
    return () => window.removeEventListener('contextmenu', onContextMenu)
  }, [])
  return isOverlay ? <OverlayPage /> : <AppShell />
}

// M4c：悬浮窗页面——透明药丸，把手拖动，编辑模式含"完成"。
function OverlayPage() {
  const [state, setState] = useState<OverlayStatePayload>({ visible: false, editing: false, talkers: [] })
  useEffect(() => {
    // 悬浮窗窗口本体透明（页面不再绘制底色）。
    document.documentElement.style.background = 'transparent'
    document.body.style.background = 'transparent'
    let active = true
    let off: (() => void) | undefined
    subscribeOverlayState((p) => { if (active) setState(p) })
      .then((un) => { if (active) off = un; else un() })
    return () => { active = false; off?.() }
  }, [])

  const onHandlePointerDown = (e: React.PointerEvent) => {
    e.preventDefault()
    void getCurrentWebviewWindow().startDragging()
  }
  const finishEditing = async () => {
    try {
      const pos = await getCurrentWebviewWindow().outerPosition()
      await tauriInvoke('save_overlay_position', { x: pos.x, y: pos.y })
      await tauriInvoke('set_overlay_editing', { editing: false })
    } catch { /* 位置读取失败时保留原位置 */ }
  }

  if (!state.editing && !state.visible) return null
  return (
    <div className={`overlay-pill ${state.editing ? 'editing' : ''}`} data-editing={state.editing}>
      {state.editing && (
        <div className="overlay-editbar">
          <span>拖动左侧把手调整位置</span>
          <button className="overlay-done" onClick={() => void finishEditing()}>完成</button>
        </div>
      )}
      <div className="overlay-body">
        <div className="overlay-handle" onPointerDown={onHandlePointerDown} title={state.editing ? '拖动调整位置' : undefined}>
          <span />
        </div>
        <div className="overlay-speakers">
          {state.talkers.length
            ? state.talkers.map((n) => (
              <div className="overlay-speaker" key={n}>
                <span className="overlay-mic" aria-hidden="true" />
                <span>{n}</span>
              </div>
            ))
            : <div className="overlay-empty">等待说话…</div>}
        </div>
      </div>
    </div>
  )
}

function AppShell() {
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
  // M6b：安全等级提升进度与结构化要求（连接页横幅）。
  const [levelUp, setLevelUp] = useState<LevelUpPayload | null>(null)
  const [levelRequired, setLevelRequired] = useState<LevelRequiredPayload | null>(null)
  const promptRef = useRef(pwdPrompt)
  promptRef.current = pwdPrompt
  // UI 改版：独立设置窗口已删除，主窗口内用 page 切换"主界面/设置"视图。
  const [page, setPage] = useState<'main' | 'settings'>('main')
  // 第 1 项：主界面改为整块频道框；消息卡片默认收起，底部箭头开合。
  const [chatOpen, setChatOpen] = useState(false)
  const hasUnread = useMemo(
    () => chatTabs.some((t) => t.unread > 0 && !(viewing && viewing.kind === t.kind && viewing.id === t.target_id)),
    [chatTabs, viewing],
  )

  // M5a：材质模式落成 data 属性，CSS 据此切换半透明（Mica）/不透明（实体）背景。
  useEffect(() => {
    document.documentElement.dataset.material = snapshot.material === 'Mica' ? 'mica' : 'solid'
  }, [snapshot.material])

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
          // M6b：连上/断开后清等级横幅（手动升级任务除外，由其自身结束事件收尾）
          if (next.connection.status === 'connected') {
            setLevelUp(null)
            setLevelRequired(null)
          }
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
        // M5 D1：设备错误/回退提示（事件由 Rust 音频事件任务广播）。
        offs.push(await subscribeDeviceError((message) => {
          if (!active) return
          setNotice(message)
          setTimeout(() => setNotice(''), 5000)
        }))
        // M6b：安全等级事件。
        offs.push(await subscribeIdentityLevelUp((payload) => {
          if (!active) return
          if (payload.phase === 'cancelled') { setLevelUp(null); return }
          if (payload.phase === 'manual-done') {
            // 达标后自动重连（connect/reconnect 会带上新身份回原频道）
            setTimeout(() => { void tauriInvoke('reconnect').catch(() => {}) }, 600)
          }
          setLevelUp(payload)
        }))
        offs.push(await subscribeIdentityLevelRequired((payload) => {
          if (!active) return
          setLevelRequired(payload)
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
        // UI 改版：托盘"打开设置"→ 主窗口内切换到设置视图。
        offs.push(await subscribeOpenSettings(() => { if (active) setPage('settings') }))
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
      setChatOpen(true) // 消息卡片收起时，从右键菜单发起私聊要把它弹出来
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
  // 消息卡片展开时，点击卡片外任意处收回（卡片内部点按不收，收回按钮自己处理）
  useEffect(() => {
    if (!chatOpen) return
    const onPointerDown = (e: PointerEvent) => {
      if (e.target instanceof Element && e.target.closest('.chat-card')) return
      setChatOpen(false)
    }
    window.addEventListener('pointerdown', onPointerDown, true)
    return () => window.removeEventListener('pointerdown', onPointerDown, true)
  }, [chatOpen])
  const removeChatTab = (target: ViewingTarget) => {
    setChatTabs((prev) => prev.filter((t) => !(t.kind === target.kind && t.target_id === target.id)))
  }

  if (runtimeMissing || (started && !snapshot.runtime_available)) return <RuntimeMissing />
  if (!started) return <main className="loading-page"><div className="loader" /><span>正在启动 MicaSpeak…</span></main>
  const noticeEl = notice ? <div className="notice">{notice}</div> : null

  const openSettings = () => setPage('settings')
  // 顶栏/状态栏在主界面与设置视图间共用。
  const topbarEl = (
    <header className="topbar">
      <div className={`status-dot ${snapshot.connection.status}`} />
      <div className="server-title"><strong>{snapshot.connection.server_name || 'TS3 服务器'}</strong><span>{snapshot.connection.server_address}</span></div>
      <span className="status-text">{statusText(snapshot)}</span>
      <button className="icon-button" onClick={openSettings} aria-label="打开设置">⚙</button>
      <button className="icon-button" onClick={() => void disconnect()} aria-label="断开连接">×</button>
    </header>
  )
  const statusbarEl = (
    <footer className="statusbar">
      {/* 第 7/8 项：仅 PTT 模式显示按住说话按钮，文案带上全局按键名 */}
      {snapshot.voice.mode === 'ptt' && (
        <button
          className={`ptt-chip hold ${transmitting ? 'active' : ''}`}
          disabled={!connected}
          aria-label="按住说话"
          onPointerDown={(e) => { e.currentTarget.setPointerCapture(e.pointerId); setTransmit(true) }}
          onPointerUp={() => setTransmit(false)}
          onPointerCancel={() => setTransmit(false)}
          onLostPointerCapture={() => setTransmit(false)}
        >{transmitting ? '说话中…' : `按住${vkName(snapshot.voice.ptt_key_vk)}说话`}</button>
      )}
      <span className="level-meter" aria-label="麦克风电平"><span className="level-fill" style={{ width: `${Math.min(100, Math.round(levels.mic * 300))}%` }} /></span>
      <span className="talkers">{talking.size ? [...talking.values()].join('、') : '无人说话'}</span>
    </footer>
  )

  // 第 3 项：设置视图。连接中嵌在顶栏/状态栏之间，返回图标放顶栏原服务器信息位
  //（风格与 ⚙/× 一致，此视图不显示服务器信息）；未连接时整页显示，左上角常驻返回图标。
  if (page === 'settings') {
    const settingsView = (
      <SettingsPage
        snapshot={snapshot}
        started={started}
        levels={levels}
        backButton={!connected ? (
          <button className="icon-button settings-back-icon" onClick={() => setPage('main')} aria-label="返回主界面" title="返回">←</button>
        ) : undefined}
      />
    )
    if (!connected) return <>{noticeEl}{settingsView}</>
    return (
      <main className="app-shell">
        <header className="topbar">
          <button className="icon-button" onClick={() => setPage('main')} aria-label="返回主界面" title="返回">←</button>
          <span className={`status-dot ${snapshot.connection.status}`} />
          <span className="status-text">{statusText(snapshot)}</span>
          <div className="flex-spacer" />
          <button className="icon-button" onClick={() => void disconnect()} aria-label="断开连接">×</button>
        </header>
        <div className="settings-embed">{settingsView}</div>
        {statusbarEl}
        {noticeEl}
      </main>
    )
  }

  if (!connected) return <>{noticeEl}<ConnectForm snapshot={snapshot} levelUp={levelUp} levelRequired={levelRequired} onConnect={connect} onConnectBookmark={(b) => void connectBookmark(b)} onSave={save} onDelete={(id) => void removeBookmark(id)} onDismissLevel={() => { setLevelUp(null); setLevelRequired(null) }} onOpenSettings={openSettings} onStartUpgrade={() => {
    const target = levelRequired?.required
    setLevelRequired(null)
    if (target) void tauriInvoke('start_security_upgrade', { target }).catch((err) => setNotice(String(err).replace(/^Error:\s*/, '')))
  }} onCancelUpgrade={() => void tauriInvoke('cancel_security_upgrade').catch(() => {})} /></>

  return (
    <main className="app-shell">
      {topbarEl}
      <div className="content-area">
        <section className="panel channel-panel">
          <div className="panel-heading"><span>频道</span><span className="muted">{snapshot.channels.length}</span></div>
          <ChannelTree channels={groupedChannels} talkingIds={talkingIds} onSelect={selectChannel} onClientContextMenu={(client, x, y) => setCtxMenu({ x, y, client })} />
        </section>
        {/* 消息卡片：默认收起滑出可视区（inert 防止焦点进入）；展开后贴住底边，
            收回按钮在卡片顶端，点击卡片外任意处也收回。布局内部沿用原消息框 */}
        <div className={`chat-card ${chatOpen ? 'open' : ''}`} inert={!chatOpen}>
          <button className="chat-collapse" onClick={() => setChatOpen(false)} aria-label="收起消息" title="收起消息">
            <span className="chat-toggle-arrow" aria-hidden="true">▾</span>
          </button>
          <ChatPanel tabs={chatTabs} ownChannelId={snapshot.own_channel_id} channels={groupedChannels} viewing={viewing} setViewing={setViewing} onNotice={(message) => { setNotice(message); setTimeout(() => setNotice(''), 3000) }} onRemoveTab={removeChatTab} />
        </div>
        {!chatOpen && (
          <button className="chat-toggle" onClick={() => setChatOpen(true)} aria-label="展开消息" title="展开消息">
            {hasUnread && <span className="chat-toggle-dot" aria-label="有未读消息" />}
            <span className="chat-toggle-arrow" aria-hidden="true">▴</span>
          </button>
        )}
      </div>
      {ctxMenu && (
        <div className="ctx-menu" style={{ left: ctxMenu.x, top: ctxMenu.y }} role="menu">
          <button role="menuitem" onClick={() => { const id = ctxMenu.client.id; setCtxMenu(null); void openPrivateChat(id) }}>
            💬 私聊（{ctxMenu.client.name}）
          </button>
        </div>
      )}
      {statusbarEl}
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
