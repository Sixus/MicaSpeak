import { Mic } from '../icons'

const GAME_BG =
  'https://images.unsplash.com/photo-1731937817165-1fed94fc03b2?w=1280&h=800&fit=crop&auto=format'

/* The 260x80 speaking pill. */
export function OverlayPill({
  empty = false,
  floating = false,
}: {
  empty?: boolean
  floating?: boolean
}) {
  return (
    <div
      style={{
        width: 260,
        minHeight: 80,
        borderRadius: 12,
        background: 'rgba(0,0,0,0.70)',
        backdropFilter: 'blur(12px)',
        WebkitBackdropFilter: 'blur(12px)',
        boxShadow: floating ? '0 10px 30px -8px rgba(0,0,0,0.6)' : 'none',
        display: 'flex',
        overflow: 'hidden',
        color: '#fff',
      }}
    >
      {/* Drag handle */}
      <div style={{ width: 14, display: 'grid', placeItems: 'center', flex: '0 0 auto' }}>
        <span style={{ width: 6, height: 40, borderRadius: 999, background: 'rgba(255,255,255,0.35)' }} />
      </div>

      <div style={{ flex: 1, display: 'flex', flexDirection: 'column', justifyContent: 'center', gap: 8, padding: '12px 14px 12px 2px' }}>
        {empty ? (
          <div style={{ fontSize: 12.5, color: 'rgba(255,255,255,0.55)' }}>等待说话…</div>
        ) : (
          <>
            <Speaker name="张三" />
            <Speaker name="李四 (我)" />
          </>
        )}
      </div>
    </div>
  )
}

function Speaker({ name }: { name: string }) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
      <span
        style={{
          width: 20,
          height: 20,
          borderRadius: 999,
          background: 'rgba(0,120,212,0.9)',
          display: 'grid',
          placeItems: 'center',
          flex: '0 0 auto',
        }}
      >
        <Mic size={12} />
      </span>
      <span style={{ fontSize: 13.5, fontWeight: 500 }}>{name}</span>
    </div>
  )
}

export function OverlayFrame() {
  return (
    <div
      style={{
        width: 640,
        height: 400,
        borderRadius: 8,
        overflow: 'hidden',
        position: 'relative',
        boxShadow: '0 20px 60px -14px rgba(0,0,0,0.4)',
      }}
    >
      {/* Blurred game screenshot backdrop (usage context) */}
      <img
        src={GAME_BG}
        alt="游戏画面背景，展示悬浮窗使用场景"
        style={{
          position: 'absolute',
          inset: 0,
          width: '100%',
          height: '100%',
          objectFit: 'cover',
          filter: 'blur(3px) saturate(1.1)',
          transform: 'scale(1.06)',
        }}
      />
      <div style={{ position: 'absolute', inset: 0, background: 'rgba(0,0,0,0.12)' }} />

      {/* Active pill, top-right */}
      <div style={{ position: 'absolute', top: 16, right: 16 }}>
        <OverlayPill floating />
      </div>

      {/* Empty-state pill beside/below it */}
      <div style={{ position: 'absolute', top: 112, right: 16 }}>
        <OverlayPill empty floating />
      </div>
    </div>
  )
}
