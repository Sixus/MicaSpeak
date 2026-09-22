import { useEffect, useState } from 'react'
import { BLUE, theme } from './lib'
import { Grid, Play } from './icons'
import { ConnectFrame } from './frames/ConnectFrame'
import { MainFrame } from './frames/MainFrame'
import { SettingsFrame } from './frames/SettingsFrame'
import { OverlayFrame, OverlayPill } from './frames/OverlayFrame'
import { DisconnectedFrame } from './frames/DisconnectedFrame'

const light = theme(false)
const dark = theme(true)

/* ---------- Canvas overview: all 7 frames as labelled cards ---------- */

function FrameLabel({ children }: { children: string }) {
  return (
    <div
      style={{
        fontSize: 12,
        fontFamily: 'ui-monospace, "Cascadia Code", "Segoe UI Mono", monospace',
        color: 'rgba(255,255,255,0.6)',
        marginBottom: 10,
        letterSpacing: 0.2,
      }}
    >
      {children}
    </div>
  )
}

function Frame({ name, children }: { name: string; children: React.ReactNode }) {
  return (
    <div>
      <FrameLabel>{name}</FrameLabel>
      {children}
    </div>
  )
}

function Canvas() {
  return (
    <div
      style={{
        display: 'flex',
        flexWrap: 'wrap',
        gap: 56,
        padding: '96px 64px 96px',
        alignItems: 'flex-start',
        justifyContent: 'center',
        maxWidth: 1600,
        margin: '0 auto',
      }}
    >
      <Frame name="01-连接页">
        <ConnectFrame t={light} />
      </Frame>
      <Frame name="02-主窗口-浅">
        <MainFrame t={light} speakers={['张三']} />
      </Frame>
      <Frame name="02-主窗口-深">
        <MainFrame t={dark} speakers={['张三']} />
      </Frame>
      <Frame name="02-主窗口-说话态">
        <MainFrame t={light} speakers={['张三', '李四']} pttActive />
      </Frame>
      <Frame name="03-设置页">
        <SettingsFrame t={light} />
      </Frame>
      <Frame name="04-悬浮窗">
        <OverlayFrame />
      </Frame>
      <Frame name="05-断线错误态">
        <DisconnectedFrame t={light} />
      </Frame>
    </div>
  )
}

/* ---------- Interactive prototype ---------- */

type View = 'connect' | 'main' | 'settings' | 'disconnected'

function Prototype() {
  const [view, setView] = useState<View>('connect')
  const [isDark, setDark] = useState(false)
  const [ptt, setPtt] = useState(false)
  const [overlay, setOverlay] = useState(false)
  const t = isDark ? dark : light

  // Left Ctrl / Space = push to talk, only while connected on the main view.
  useEffect(() => {
    if (view !== 'main') return
    const down = (e: KeyboardEvent) => {
      if (e.key === 'Control' || e.code === 'Space') {
        e.preventDefault()
        setPtt(true)
      }
    }
    const up = (e: KeyboardEvent) => {
      if (e.key === 'Control' || e.code === 'Space') setPtt(false)
    }
    window.addEventListener('keydown', down)
    window.addEventListener('keyup', up)
    return () => {
      window.removeEventListener('keydown', down)
      window.removeEventListener('keyup', up)
    }
  }, [view])

  const speakers = ptt ? ['张三', '李四'] : ['张三']

  const pill: React.CSSProperties = {
    height: 30,
    padding: '0 12px',
    borderRadius: 6,
    border: '1px solid rgba(255,255,255,0.18)',
    background: 'rgba(255,255,255,0.10)',
    color: '#fff',
    fontSize: 12.5,
    cursor: 'pointer',
    backdropFilter: 'blur(10px)',
  }

  return (
    <div
      style={{
        position: 'relative',
        minHeight: '100vh',
        display: 'grid',
        placeItems: 'center',
        background:
          'radial-gradient(120% 120% at 20% 10%, #3a5ba0 0%, #2a3f78 35%, #16203f 75%, #0d1226 100%)',
        overflow: 'hidden',
      }}
    >
      {/* Windows-11-style bloom */}
      <div
        style={{
          position: 'absolute',
          top: '-20%',
          left: '30%',
          width: 700,
          height: 700,
          borderRadius: '50%',
          background: 'radial-gradient(circle, rgba(120,180,255,0.45), transparent 60%)',
          filter: 'blur(40px)',
        }}
      />

      {/* Prototype controls */}
      <div style={{ position: 'absolute', top: 20, display: 'flex', gap: 8, zIndex: 20 }}>
        {view === 'settings' && (
          <button style={pill} onClick={() => setView('main')}>
            ← 返回主窗口
          </button>
        )}
        {(view === 'main' || view === 'settings' || view === 'disconnected') && (
          <>
            <button style={pill} onClick={() => setDark((d) => !d)}>
              {isDark ? '浅色主题' : '深色主题'}
            </button>
            <button style={pill} onClick={() => setOverlay((o) => !o)}>
              {overlay ? '关闭悬浮窗' : '悬浮窗'}
            </button>
            {view !== 'disconnected' ? (
              <button style={pill} onClick={() => setView('disconnected')}>
                模拟断线
              </button>
            ) : (
              <button style={pill} onClick={() => setView('main')}>
                立即重连
              </button>
            )}
            <button style={pill} onClick={() => setView('connect')}>
              断开连接
            </button>
          </>
        )}
      </div>

      {/* Current view */}
      <div style={{ zIndex: 10 }}>
        {view === 'connect' && <ConnectFrame t={t} onConnect={() => setView('main')} />}
        {view === 'main' && (
          <MainFrame
            t={t}
            speakers={speakers}
            pttActive={ptt}
            onGear={() => setView('settings')}
            onPttDown={() => setPtt(true)}
            onPttUp={() => setPtt(false)}
          />
        )}
        {view === 'settings' && <SettingsFrame t={t} />}
        {view === 'disconnected' && (
          <MainFrame t={t} dot="yellow" disconnected reconnecting />
        )}
      </div>

      {/* Floating overlay pill over the desktop corner */}
      {overlay && (
        <div style={{ position: 'absolute', top: 72, right: 40, zIndex: 30 }}>
          <OverlayPill floating empty={!ptt} />
        </div>
      )}

      {view === 'main' && (
        <div
          style={{
            position: 'absolute',
            bottom: 22,
            fontSize: 12,
            color: 'rgba(255,255,255,0.6)',
            zIndex: 20,
          }}
        >
          按住 <kbd style={{ color: '#fff' }}>Ctrl</kbd> 或 <kbd style={{ color: '#fff' }}>空格</kbd> 说话
        </div>
      )}
    </div>
  )
}

/* ---------- Shell with mode switch ---------- */

export default function App() {
  const [mode, setMode] = useState<'canvas' | 'proto'>('canvas')

  return (
    <div style={{ minHeight: '100vh', background: '#1b1b1f', color: '#fff' }}>
      {/* Mode switch */}
      <div
        style={{
          position: 'fixed',
          top: 16,
          left: '50%',
          transform: 'translateX(-50%)',
          zIndex: 100,
          display: 'flex',
          gap: 3,
          padding: 3,
          borderRadius: 999,
          background: 'rgba(40,40,46,0.8)',
          border: '1px solid rgba(255,255,255,0.1)',
          backdropFilter: 'blur(16px)',
          boxShadow: '0 8px 24px -8px rgba(0,0,0,0.5)',
        }}
      >
        {(
          [
            { id: 'canvas', label: '画布总览', icon: <Grid size={15} /> },
            { id: 'proto', label: '原型', icon: <Play size={15} /> },
          ] as const
        ).map((m) => (
          <button
            key={m.id}
            onClick={() => setMode(m.id)}
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 6,
              height: 32,
              padding: '0 16px',
              borderRadius: 999,
              border: 'none',
              cursor: 'pointer',
              fontSize: 13,
              fontWeight: 500,
              background: mode === m.id ? BLUE : 'transparent',
              color: mode === m.id ? '#fff' : 'rgba(255,255,255,0.7)',
            }}
          >
            {m.icon}
            {m.label}
          </button>
        ))}
      </div>

      {mode === 'canvas' ? <Canvas /> : <Prototype />}
    </div>
  )
}
