import type { CSSProperties, ReactNode } from 'react'
import { Minus, Square, X } from './icons'

export const BLUE = '#0078D4'
export const BLUE_HOVER = '#1A86D9'

export type Theme = {
  dark: boolean
  windowBg: string
  card: string
  cardSolid: string
  text: string
  subtext: string
  faint: string
  border: string
  divider: string
  inputBg: string
  inputBorder: string
  hover: string
  speaking: string
  speakingBorder: string
  chip: string
}

export function theme(dark: boolean): Theme {
  return dark
    ? {
        dark,
        windowBg:
          'radial-gradient(120% 90% at 15% 0%, #2b2f3a 0%, #23252c 42%, #1c1c1f 100%)',
        card: 'rgba(43,43,43,0.86)',
        cardSolid: '#2b2b2b',
        text: '#ffffff',
        subtext: 'rgba(255,255,255,0.62)',
        faint: 'rgba(255,255,255,0.40)',
        border: 'rgba(255,255,255,0.09)',
        divider: 'rgba(255,255,255,0.07)',
        inputBg: 'rgba(255,255,255,0.06)',
        inputBorder: 'rgba(255,255,255,0.12)',
        hover: 'rgba(255,255,255,0.06)',
        speaking: 'rgba(0,120,212,0.28)',
        speakingBorder: 'rgba(64,164,255,0.55)',
        chip: 'rgba(255,255,255,0.10)',
      }
    : {
        dark,
        windowBg:
          'radial-gradient(120% 90% at 15% 0%, #eef2f8 0%, #eaecef 45%, #f3f3f3 100%)',
        card: 'rgba(255,255,255,0.86)',
        cardSolid: '#ffffff',
        text: '#1a1a1a',
        subtext: 'rgba(0,0,0,0.56)',
        faint: 'rgba(0,0,0,0.38)',
        border: 'rgba(0,0,0,0.07)',
        divider: 'rgba(0,0,0,0.06)',
        inputBg: 'rgba(255,255,255,0.72)',
        inputBorder: 'rgba(0,0,0,0.12)',
        hover: 'rgba(0,0,0,0.04)',
        speaking: 'rgba(0,120,212,0.13)',
        speakingBorder: 'rgba(0,120,212,0.45)',
        chip: 'rgba(0,0,0,0.05)',
      }
}

/* A native-feeling Windows 11 window: 8px corners, mica background,
   slim caption bar with min / max / close buttons. */
export function Win({
  t,
  width,
  height = 640,
  children,
  style,
}: {
  t: Theme
  width: number
  height?: number
  children: ReactNode
  style?: CSSProperties
}) {
  return (
    <div
      style={{
        width,
        height,
        borderRadius: 8,
        background: t.windowBg,
        boxShadow: t.dark
          ? '0 20px 60px -12px rgba(0,0,0,0.6), 0 0 0 1px rgba(255,255,255,0.06)'
          : '0 20px 60px -14px rgba(0,0,0,0.28), 0 0 0 1px rgba(0,0,0,0.05)',
        overflow: 'hidden',
        display: 'flex',
        flexDirection: 'column',
        color: t.text,
        ...style,
      }}
    >
      <CaptionBar t={t} />
      <div style={{ flex: 1, minHeight: 0, display: 'flex', flexDirection: 'column' }}>
        {children}
      </div>
    </div>
  )
}

function CaptionBar({ t }: { t: Theme }) {
  const btn: CSSProperties = {
    width: 40,
    height: 30,
    display: 'grid',
    placeItems: 'center',
    color: t.subtext,
    cursor: 'default',
  }
  return (
    <div
      style={{
        height: 30,
        display: 'flex',
        justifyContent: 'flex-end',
        flex: '0 0 auto',
      }}
    >
      <div style={btn}>
        <Minus size={14} />
      </div>
      <div style={btn}>
        <Square size={12} />
      </div>
      <div style={{ ...btn }} className="ms-close">
        <X size={14} />
      </div>
    </div>
  )
}

/* Frosted content card that keeps 12px from window edges by default. */
export function Card({
  t,
  children,
  style,
}: {
  t: Theme
  children: ReactNode
  style?: CSSProperties
}) {
  return (
    <div
      style={{
        background: t.card,
        backdropFilter: 'blur(20px) saturate(140%)',
        WebkitBackdropFilter: 'blur(20px) saturate(140%)',
        border: `1px solid ${t.border}`,
        borderRadius: 8,
        ...style,
      }}
    >
      {children}
    </div>
  )
}

export function StatusDot({ color }: { color: 'green' | 'yellow' | 'red' | 'gray' }) {
  const c = {
    green: '#3FB950',
    yellow: '#E8B23A',
    red: '#E5484D',
    gray: '#8A8A8A',
  }[color]
  return (
    <span
      style={{
        width: 8,
        height: 8,
        borderRadius: 999,
        background: c,
        boxShadow: `0 0 0 3px ${c}22`,
        display: 'inline-block',
        flex: '0 0 auto',
      }}
    />
  )
}
