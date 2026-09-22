import type { ReactNode } from 'react'
import { BLUE, Card, StatusDot, Win, type Theme } from '../lib'
import { ChevronDown, ChevronRight, Gear, Lock, Mic, Send, Spinner } from '../icons'

type UserRow = { name: string; me?: boolean }
type Group = { name: string; open: boolean; locked?: boolean; users: UserRow[] }

const groups: Group[] = [
  { name: '大厅', open: true, users: [{ name: '王五' }, { name: '赵六' }] },
  {
    name: '游戏频道',
    open: true,
    users: [{ name: '张三' }, { name: '李四', me: true }, { name: '孙七' }],
  },
  { name: '音乐频道', open: false, locked: true, users: [] },
]

type Msg = { time: string; nick: string; body: ReactNode; me?: boolean }

const messages: Msg[] = [
  { time: '14:28', nick: '王五', body: '大家晚上好，今晚开几把？' },
  { time: '14:30', nick: '张三', body: '房间信息在这' },
  {
    time: '14:31',
    nick: '张三',
    body: (
      <span style={{ color: BLUE, textDecoration: 'underline' }}>
        https://kaihei.gg/room/42
      </span>
    ),
  },
  { time: '14:32', nick: '李四', body: '收到，马上进语音 🎧', me: true },
  { time: '14:33', nick: '孙七', body: '等我五分钟，先热身' },
]

function Avatar({ name, t }: { name: string; t: Theme }) {
  return (
    <span
      style={{
        width: 20,
        height: 20,
        borderRadius: 999,
        background: t.chip,
        display: 'grid',
        placeItems: 'center',
        fontSize: 10,
        color: t.subtext,
        flex: '0 0 auto',
      }}
    >
      {name.slice(0, 1)}
    </span>
  )
}

export function MainFrame({
  t,
  speakers = [],
  pttActive = false,
  dot = 'green',
  disconnected = false,
  reconnecting = false,
  onGear,
  onPttDown,
  onPttUp,
}: {
  t: Theme
  speakers?: string[]
  pttActive?: boolean
  dot?: 'green' | 'yellow'
  disconnected?: boolean
  reconnecting?: boolean
  onGear?: () => void
  onPttDown?: () => void
  onPttUp?: () => void
}) {
  return (
    <Win t={t} width={420}>
      <div style={{ position: 'relative', flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column' }}>
        {/* Top bar */}
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: 9,
            padding: '2px 14px 10px',
          }}
        >
          <StatusDot color={dot} />
          <div style={{ fontSize: 13.5, fontWeight: 600 }}>开黑联盟</div>
          <div style={{ fontSize: 11.5, color: t.faint }}>voice.kaihei.gg</div>
          <div style={{ flex: 1 }} />
          <div style={{ fontSize: 12, color: t.subtext }}>李四</div>
          <button
            onClick={onGear}
            title="设置"
            style={{
              width: 28,
              height: 28,
              borderRadius: 5,
              border: 'none',
              background: 'transparent',
              color: t.subtext,
              display: 'grid',
              placeItems: 'center',
              cursor: 'pointer',
            }}
            className="ms-row"
          >
            <Gear size={17} />
          </button>
        </div>

        {/* Body: channel tree | chat */}
        <div style={{ flex: 1, minHeight: 0, display: 'flex', gap: 8, padding: '0 12px' }}>
          {/* Channel tree */}
          <Card
            t={t}
            style={{ width: '45%', minWidth: 0, padding: '8px 5px', overflow: 'auto' }}
          >
            {groups.map((g) => (
              <div key={g.name}>
                <div
                  className="ms-row"
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: 4,
                    padding: '6px 6px',
                    borderRadius: 5,
                    fontSize: 13,
                    fontWeight: 500,
                  }}
                >
                  <span style={{ color: t.faint, display: 'grid', placeItems: 'center' }}>
                    {g.open ? <ChevronDown size={15} /> : <ChevronRight size={15} />}
                  </span>
                  <span style={{ flex: 1, minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                    {g.name}
                  </span>
                  {g.locked && <Lock size={13} style={{ color: t.faint }} />}
                </div>
                {g.open &&
                  g.users.map((u) => {
                    const active = speakers.includes(u.name)
                    return (
                      <div
                        key={u.name}
                        style={{
                          display: 'flex',
                          alignItems: 'center',
                          gap: 8,
                          margin: '1px 4px',
                          padding: '5px 8px 5px 26px',
                          borderRadius: 5,
                          fontSize: 12.5,
                          background: active ? t.speaking : 'transparent',
                          boxShadow: active ? `inset 0 0 0 1px ${t.speakingBorder}` : 'none',
                        }}
                      >
                        <Avatar name={u.name} t={t} />
                        <span
                          style={{
                            flex: 1,
                            minWidth: 0,
                            overflow: 'hidden',
                            textOverflow: 'ellipsis',
                            whiteSpace: 'nowrap',
                            fontWeight: active ? 600 : 400,
                          }}
                        >
                          {u.name}
                          {u.me && <span style={{ color: t.faint, fontWeight: 400 }}> (我)</span>}
                        </span>
                        {active && (
                          <Mic size={13} style={{ color: t.dark ? '#5AB0FF' : BLUE }} />
                        )}
                      </div>
                    )
                  })}
              </div>
            ))}
          </Card>

          {/* Chat */}
          <Card t={t} style={{ flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column' }}>
            {/* Tabs */}
            <div style={{ display: 'flex', gap: 2, padding: '6px 8px 0', borderBottom: `1px solid ${t.divider}` }}>
              {[
                { label: '频道', active: true, unread: false },
                { label: '私聊·张三', active: false, unread: true },
              ].map((tab) => (
                <div
                  key={tab.label}
                  style={{
                    position: 'relative',
                    padding: '6px 10px 8px',
                    fontSize: 12.5,
                    fontWeight: tab.active ? 600 : 400,
                    color: tab.active ? t.text : t.subtext,
                    borderBottom: `2px solid ${tab.active ? BLUE : 'transparent'}`,
                    display: 'flex',
                    alignItems: 'center',
                    gap: 5,
                  }}
                >
                  {tab.label}
                  {tab.unread && (
                    <span style={{ width: 6, height: 6, borderRadius: 999, background: '#E5484D' }} />
                  )}
                </div>
              ))}
            </div>

            {/* Messages */}
            <div style={{ flex: 1, minHeight: 0, overflow: 'auto', padding: '8px 10px', display: 'flex', flexDirection: 'column', gap: 7 }}>
              {messages.map((m, i) => (
                <div key={i} style={{ fontSize: 12.5, lineHeight: 1.45 }}>
                  <span style={{ color: t.faint, marginRight: 6, fontVariantNumeric: 'tabular-nums' }}>
                    {m.time}
                  </span>
                  <span style={{ color: m.me ? (t.dark ? '#5AB0FF' : BLUE) : t.text, fontWeight: 600, marginRight: 6 }}>
                    {m.nick}
                    {m.me && ' (我)'}
                  </span>
                  <span style={{ color: t.text }}>{m.body}</span>
                </div>
              ))}
            </div>

            {/* Composer */}
            <div style={{ display: 'flex', gap: 6, padding: 8, borderTop: `1px solid ${t.divider}` }}>
              <div
                style={{
                  flex: 1,
                  height: 32,
                  display: 'flex',
                  alignItems: 'center',
                  padding: '0 10px',
                  borderRadius: 5,
                  background: t.inputBg,
                  border: `1px solid ${t.inputBorder}`,
                  fontSize: 12.5,
                  color: t.faint,
                }}
              >
                发送消息到 频道…
              </div>
              <button
                style={{
                  width: 32,
                  height: 32,
                  borderRadius: 5,
                  border: 'none',
                  background: BLUE,
                  color: '#fff',
                  display: 'grid',
                  placeItems: 'center',
                  cursor: 'pointer',
                }}
              >
                <Send size={15} />
              </button>
            </div>
          </Card>
        </div>

        {/* Bottom status bar */}
        <div style={{ padding: 12, paddingTop: 8 }}>
          {disconnected ? (
            <div
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: 10,
                fontSize: 12,
                color: t.subtext,
              }}
            >
              <span>已断开 · 10 秒后自动重连</span>
              <div style={{ flex: 1 }} />
              <button
                style={{
                  height: 26,
                  padding: '0 12px',
                  borderRadius: 5,
                  border: `1px solid ${t.inputBorder}`,
                  background: t.inputBg,
                  color: t.text,
                  fontSize: 12,
                  cursor: 'pointer',
                }}
              >
                立即重连
              </button>
            </div>
          ) : (
            <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
              <button
                onMouseDown={onPttDown}
                onMouseUp={onPttUp}
                onMouseLeave={onPttUp}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: 6,
                  height: 28,
                  padding: '0 10px',
                  borderRadius: 5,
                  border: `1px solid ${pttActive ? 'transparent' : t.inputBorder}`,
                  background: pttActive ? BLUE : t.inputBg,
                  color: pttActive ? '#fff' : t.subtext,
                  fontSize: 12,
                  cursor: 'pointer',
                  transition: 'background 0.08s, color 0.08s',
                  flex: '0 0 auto',
                }}
              >
                <Mic size={14} />
                左Ctrl 按住说话
              </button>

              {/* Volume meter */}
              <div
                style={{
                  flex: 1,
                  height: 4,
                  borderRadius: 999,
                  background: t.chip,
                  overflow: 'hidden',
                }}
              >
                <div
                  className={pttActive ? 'ms-vad' : undefined}
                  style={{
                    height: '100%',
                    width: pttActive ? '60%' : '0%',
                    borderRadius: 999,
                    background: `linear-gradient(90deg, ${BLUE}, #46C48B)`,
                  }}
                />
              </div>

              <span style={{ fontSize: 12, color: t.subtext, fontVariantNumeric: 'tabular-nums', flex: '0 0 auto' }}>
                延迟 32ms
              </span>
            </div>
          )}
        </div>

        {/* Reconnecting overlay */}
        {reconnecting && (
          <div
            style={{
              position: 'absolute',
              inset: 0,
              display: 'grid',
              placeItems: 'center',
              background: t.dark ? 'rgba(20,20,20,0.35)' : 'rgba(240,240,240,0.35)',
              backdropFilter: 'blur(2px)',
            }}
          >
            <Card t={t} style={{ padding: '18px 22px', display: 'flex', alignItems: 'center', gap: 12, boxShadow: '0 12px 40px -12px rgba(0,0,0,0.4)' }}>
              <span className="ms-spin" style={{ color: BLUE, display: 'grid', placeItems: 'center' }}>
                <Spinner size={20} />
              </span>
              <span style={{ fontSize: 13 }}>连接已断开，正在尝试恢复…</span>
            </Card>
          </div>
        )}
      </div>
    </Win>
  )
}
