import { BLUE, Card, StatusDot, Win, type Theme } from '../lib'
import { Mic, X } from '../icons'

const bookmarks = [
  { name: '开黑联盟', addr: 'voice.kaihei.gg:9987' },
  { name: '深夜电台', addr: 'ts.midnight-radio.net' },
  { name: '设计小组', addr: '10.0.4.21:9987' },
]

function Field({
  t,
  label,
  value,
  mono,
}: {
  t: Theme
  label: string
  value: string
  mono?: boolean
}) {
  return (
    <label style={{ display: 'block' }}>
      <div style={{ fontSize: 12, color: t.subtext, marginBottom: 5 }}>{label}</div>
      <div
        style={{
          height: 34,
          display: 'flex',
          alignItems: 'center',
          padding: '0 10px',
          borderRadius: 5,
          background: t.inputBg,
          border: `1px solid ${t.inputBorder}`,
          borderBottom: `2px solid ${t.dark ? 'rgba(255,255,255,0.35)' : 'rgba(0,0,0,0.4)'}`,
          fontSize: 13,
          letterSpacing: mono ? 0.2 : 0,
        }}
      >
        {value}
      </div>
    </label>
  )
}

export function ConnectFrame({
  t,
  error,
  onConnect,
}: {
  t: Theme
  error?: string
  onConnect?: () => void
}) {
  return (
    <Win t={t} width={420}>
      <div
        style={{
          flex: 1,
          minHeight: 0,
          display: 'flex',
          flexDirection: 'column',
          padding: 12,
          paddingTop: 4,
          gap: 12,
        }}
      >
        {/* Identity + connect card */}
        <Card t={t} style={{ padding: 20 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 11, marginBottom: 18 }}>
            <div
              style={{
                width: 40,
                height: 40,
                borderRadius: 10,
                background: `linear-gradient(150deg, ${BLUE}, #2FA1F0)`,
                display: 'grid',
                placeItems: 'center',
                color: '#fff',
                boxShadow: '0 4px 12px -2px rgba(0,120,212,0.5)',
              }}
            >
              <Mic size={20} />
            </div>
            <div>
              <div style={{ fontSize: 19, fontWeight: 600, letterSpacing: -0.2 }}>
                MicaSpeak
              </div>
              <div style={{ fontSize: 12, color: t.subtext, marginTop: 1 }}>
                轻量 TeamSpeak 客户端
              </div>
            </div>
          </div>

          <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
            <Field t={t} label="服务器地址" value="voice.kaihei.gg:9987" mono />
            <Field t={t} label="昵称" value="李四" />
          </div>

          <button
            onClick={onConnect}
            style={{
              marginTop: 16,
              width: '100%',
              height: 38,
              borderRadius: 5,
              border: 'none',
              background: BLUE,
              color: '#fff',
              fontSize: 14,
              fontWeight: 600,
              cursor: 'pointer',
              boxShadow: '0 2px 8px -2px rgba(0,120,212,0.55)',
            }}
          >
            连接
          </button>

          {error && (
            <div style={{ marginTop: 10, fontSize: 12, color: '#E5484D', lineHeight: 1.4 }}>
              {error}
            </div>
          )}
        </Card>

        {/* Bookmarks */}
        <Card t={t} style={{ padding: '12px 6px 8px', flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column' }}>
          <div
            style={{
              fontSize: 11,
              fontWeight: 600,
              letterSpacing: 0.4,
              color: t.faint,
              padding: '0 12px 6px',
            }}
          >
            书签
          </div>
          <div style={{ flex: 1, minHeight: 0, overflow: 'auto' }}>
            {bookmarks.map((b) => (
              <div key={b.addr} className="ms-row" style={{ borderRadius: 5 }}>
                <div
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: 10,
                    padding: '8px 8px 8px 12px',
                  }}
                >
                  <div style={{ flex: 1, minWidth: 0 }}>
                    <div style={{ fontSize: 13, fontWeight: 500 }}>{b.name}</div>
                    <div
                      style={{
                        fontSize: 11.5,
                        color: t.subtext,
                        marginTop: 1,
                        overflow: 'hidden',
                        textOverflow: 'ellipsis',
                        whiteSpace: 'nowrap',
                      }}
                    >
                      {b.addr}
                    </div>
                  </div>
                  <button
                    title="删除书签"
                    style={{
                      width: 26,
                      height: 26,
                      borderRadius: 5,
                      border: 'none',
                      background: 'transparent',
                      color: t.faint,
                      display: 'grid',
                      placeItems: 'center',
                      cursor: 'pointer',
                    }}
                  >
                    <X size={14} />
                  </button>
                </div>
              </div>
            ))}
          </div>
          <button
            style={{
              alignSelf: 'flex-start',
              margin: '4px 8px 4px',
              padding: '4px 4px',
              background: 'transparent',
              border: 'none',
              color: BLUE,
              fontSize: 12.5,
              cursor: 'pointer',
            }}
          >
            ＋ 存为书签
          </button>
        </Card>

        {/* Status line */}
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: 7,
            fontSize: 12,
            color: t.subtext,
            paddingLeft: 2,
          }}
        >
          <StatusDot color="gray" />
          未连接
        </div>
      </div>
    </Win>
  )
}
