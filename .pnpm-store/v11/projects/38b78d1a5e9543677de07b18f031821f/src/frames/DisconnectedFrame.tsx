import { BLUE, Card, type Theme } from '../lib'
import { MainFrame } from './MainFrame'

/* Frame 7 composes the disconnected main window with a small
   form-error sample card annotating the connect-failure state. */
export function DisconnectedFrame({ t }: { t: Theme }) {
  return (
    <div style={{ display: 'flex', gap: 28, alignItems: 'flex-start' }}>
      <MainFrame t={t} dot="yellow" disconnected reconnecting />

      {/* Form-error sample */}
      <div style={{ width: 300, paddingTop: 40 }}>
        <div style={{ fontSize: 12, color: t.dark ? 'rgba(255,255,255,0.7)' : 'rgba(0,0,0,0.55)', marginBottom: 10 }}>
          表单错误示例 — 连接卡片下方
        </div>
        <Card t={t} style={{ padding: 18 }}>
          <div
            style={{
              height: 34,
              display: 'flex',
              alignItems: 'center',
              padding: '0 10px',
              borderRadius: 5,
              background: t.inputBg,
              border: `1px solid #E5484D`,
              borderBottom: '2px solid #E5484D',
              fontSize: 13,
              marginBottom: 12,
            }}
          >
            voice.wrong-host:9987
          </div>
          <button
            style={{
              width: '100%',
              height: 38,
              borderRadius: 5,
              border: 'none',
              background: BLUE,
              color: '#fff',
              fontSize: 14,
              fontWeight: 600,
              cursor: 'pointer',
            }}
          >
            连接
          </button>
          <div style={{ marginTop: 10, fontSize: 12, color: '#E5484D', lineHeight: 1.4 }}>
            连接失败：地址无法解析，请检查服务器地址
          </div>
        </Card>
      </div>
    </div>
  )
}
