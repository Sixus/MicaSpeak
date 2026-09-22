import { BLUE, Card, Win, type Theme } from '../lib'

const categories = ['发送', '音频', '悬浮窗', '身份', '关于']

function SectionTitle({ t, children }: { t: Theme; children: string }) {
  return (
    <div
      style={{
        fontSize: 11,
        fontWeight: 600,
        letterSpacing: 0.5,
        color: t.faint,
        margin: '4px 0 10px',
      }}
    >
      {children}
    </div>
  )
}

function Radio({ t, checked, label }: { t: Theme; checked: boolean; label: string }) {
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 10, padding: '7px 0' }}>
      <span
        style={{
          width: 18,
          height: 18,
          borderRadius: 999,
          border: `1.5px solid ${checked ? BLUE : t.inputBorder}`,
          display: 'grid',
          placeItems: 'center',
          flex: '0 0 auto',
        }}
      >
        {checked && <span style={{ width: 8, height: 8, borderRadius: 999, background: BLUE }} />}
      </span>
      <span style={{ fontSize: 13, color: t.text }}>{label}</span>
    </div>
  )
}

function Toggle({ t, on }: { t: Theme; on: boolean }) {
  return (
    <span
      style={{
        width: 40,
        height: 22,
        borderRadius: 999,
        background: on ? BLUE : t.chip,
        border: `1px solid ${on ? BLUE : t.inputBorder}`,
        display: 'inline-flex',
        alignItems: 'center',
        padding: 2,
        justifyContent: on ? 'flex-end' : 'flex-start',
        flex: '0 0 auto',
      }}
    >
      <span style={{ width: 16, height: 16, borderRadius: 999, background: on ? '#fff' : t.subtext }} />
    </span>
  )
}

function Dropdown({ t, label, value }: { t: Theme; label: string; value: string }) {
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
          fontSize: 13,
        }}
      >
        <span style={{ flex: 1 }}>{value}</span>
        <span style={{ color: t.faint }}>▾</span>
      </div>
    </label>
  )
}

export function SettingsFrame({ t }: { t: Theme }) {
  return (
    <Win t={t} width={520}>
      <div style={{ flex: 1, minHeight: 0, display: 'flex', padding: 12, paddingTop: 4, gap: 8 }}>
        {/* Left rail */}
        <Card t={t} style={{ width: 128, padding: 6, flex: '0 0 auto' }}>
          <div style={{ fontSize: 15, fontWeight: 600, padding: '6px 8px 10px' }}>设置</div>
          {categories.map((c, i) => (
            <div
              key={c}
              className="ms-row"
              style={{
                position: 'relative',
                padding: '9px 10px',
                borderRadius: 5,
                fontSize: 13,
                fontWeight: i === 0 ? 600 : 400,
                color: i === 0 ? t.text : t.subtext,
                background: i === 0 ? t.hover : 'transparent',
                margin: '1px 0',
              }}
            >
              {i === 0 && (
                <span
                  style={{
                    position: 'absolute',
                    left: 2,
                    top: '50%',
                    transform: 'translateY(-50%)',
                    width: 3,
                    height: 16,
                    borderRadius: 999,
                    background: BLUE,
                  }}
                />
              )}
              {c}
            </div>
          ))}
        </Card>

        {/* Right panel: 发送 */}
        <Card t={t} style={{ flex: 1, minWidth: 0, padding: 18, overflow: 'auto' }}>
          <div style={{ fontSize: 16, fontWeight: 600, marginBottom: 4 }}>发送</div>
          <div style={{ fontSize: 12, color: t.subtext, marginBottom: 18 }}>
            控制语音的传输方式与降噪。
          </div>

          <SectionTitle t={t}>传输模式</SectionTitle>
          <Radio t={t} checked label="按键说话 (PTT)" />
          <Radio t={t} checked={false} label="语音激活 (VAD)" />

          {/* Hotkey row */}
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 10,
              margin: '10px 0 6px',
              paddingLeft: 28,
            }}
          >
            <span style={{ fontSize: 12.5, color: t.subtext }}>快捷键</span>
            <span
              style={{
                fontSize: 12.5,
                padding: '3px 10px',
                borderRadius: 5,
                background: t.chip,
                border: `1px solid ${t.inputBorder}`,
                fontWeight: 500,
              }}
            >
              左Ctrl
            </span>
            <button
              style={{
                fontSize: 12,
                height: 26,
                padding: '0 10px',
                borderRadius: 5,
                border: `1px solid ${t.inputBorder}`,
                background: t.inputBg,
                color: t.text,
                cursor: 'pointer',
              }}
            >
              修改
            </button>
          </div>

          <div style={{ height: 1, background: t.divider, margin: '16px 0' }} />

          {/* VAD block */}
          <SectionTitle t={t}>语音激活阈值</SectionTitle>
          <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
            <span style={{ fontSize: 11, color: t.faint }}>0.1</span>
            <div style={{ flex: 1, position: 'relative', height: 20, display: 'flex', alignItems: 'center' }}>
              <div style={{ height: 4, width: '100%', borderRadius: 999, background: t.chip }}>
                <div style={{ height: '100%', width: '62%', borderRadius: 999, background: BLUE }} />
              </div>
              <span
                style={{
                  position: 'absolute',
                  left: '62%',
                  transform: 'translateX(-50%)',
                  width: 16,
                  height: 16,
                  borderRadius: 999,
                  background: '#fff',
                  border: `1px solid ${t.inputBorder}`,
                  boxShadow: '0 1px 4px rgba(0,0,0,0.25)',
                }}
              />
            </div>
            <span style={{ fontSize: 11, color: t.faint }}>0.9</span>
            <span style={{ fontSize: 12.5, fontWeight: 600, width: 30, textAlign: 'right', fontVariantNumeric: 'tabular-nums' }}>
              0.6
            </span>
          </div>

          {/* Live voice-probability bar */}
          <div style={{ marginTop: 10, height: 6, borderRadius: 999, background: t.chip, overflow: 'hidden' }}>
            <div
              className="ms-vad"
              style={{ height: '100%', width: '48%', borderRadius: 999, background: `linear-gradient(90deg, ${BLUE}, #46C48B)` }}
            />
          </div>
          <div style={{ fontSize: 11.5, color: t.subtext, marginTop: 6 }}>
            实时语音概率 · 超过阈值即开始传输
          </div>

          {/* Noise suppression toggle */}
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 12,
              margin: '16px 0 4px',
            }}
          >
            <div style={{ flex: 1 }}>
              <div style={{ fontSize: 13 }}>噪声抑制</div>
              <div style={{ fontSize: 11.5, color: t.subtext, marginTop: 1 }}>AI 降噪 (RNNoise)</div>
            </div>
            <Toggle t={t} on />
          </div>

          <div style={{ height: 1, background: t.divider, margin: '16px 0' }} />

          {/* Audio devices */}
          <SectionTitle t={t}>音频设备</SectionTitle>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
            <Dropdown t={t} label="输入设备" value="系统默认" />
            <Dropdown t={t} label="输出设备" value="系统默认" />
          </div>

          <button
            style={{
              marginTop: 20,
              background: 'transparent',
              border: 'none',
              color: BLUE,
              fontSize: 12.5,
              cursor: 'pointer',
              padding: 0,
            }}
          >
            恢复默认设置
          </button>
        </Card>
      </div>
    </Win>
  )
}
