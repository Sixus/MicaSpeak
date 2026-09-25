import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { AppSnapshot, ChatMessageEvent } from './types'

export const tauriInvoke = <T>(command: string, args?: Record<string, unknown>) =>
  invoke<T>(command, args)

export async function subscribeSnapshot(onSnapshot: (snapshot: AppSnapshot) => void): Promise<UnlistenFn> {
  return listen<AppSnapshot>('app://snapshot', (event) => onSnapshot(event.payload))
}

export async function subscribeChatMessage(onMessage: (payload: ChatMessageEvent) => void): Promise<UnlistenFn> {
  return listen<ChatMessageEvent>('chat://message', (event) => onMessage(event.payload))
}

export async function subscribeRuntimeMissing(onMissing: () => void): Promise<UnlistenFn> {
  return listen('runtime://webview2-missing', onMissing)
}

// M5 D1：设备错误与回退提示（拔出设备时 Rust 回退系统默认并广播）。
export async function subscribeDeviceError(onError: (message: string) => void): Promise<UnlistenFn> {
  return listen<{ message: string }>('audio://device-error', (event) => onError(event.payload.message))
}

export async function subscribeUserError(onError: (message: string) => void): Promise<UnlistenFn> {
  return listen<{ message: string }>('error://user', (event) => onError(event.payload.message))
}

export type TalkingPayload = {
  client_id: number
  name: string
  talking: boolean
  is_self: boolean
  timestamp: number
}

export async function subscribeTalking(onTalking: (payload: TalkingPayload) => void): Promise<UnlistenFn> {
  return listen<TalkingPayload>('voice://talking', (event) => onTalking(event.payload))
}

export type LevelPayload = { mic: number; out: number; prob: number }

export async function subscribeLevel(onLevel: (payload: LevelPayload) => void): Promise<UnlistenFn> {
  return listen<LevelPayload>('voice://level', (event) => onLevel(event.payload))
}

export type OverlayStatePayload = { visible: boolean; editing: boolean; talkers: string[] }

export async function subscribeOverlayState(onState: (payload: OverlayStatePayload) => void): Promise<UnlistenFn> {
  return listen<OverlayStatePayload>('overlay://state', (event) => onState(event.payload))
}
