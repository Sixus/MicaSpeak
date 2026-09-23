import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { AppSnapshot } from './types'

export const tauriInvoke = <T>(command: string, args?: Record<string, unknown>) =>
  invoke<T>(command, args)

export async function subscribeSnapshot(onSnapshot: (snapshot: AppSnapshot) => void): Promise<UnlistenFn> {
  return listen<AppSnapshot>('app://snapshot', (event) => onSnapshot(event.payload))
}

export async function subscribeRuntimeMissing(onMissing: () => void): Promise<UnlistenFn> {
  return listen('runtime://webview2-missing', onMissing)
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
