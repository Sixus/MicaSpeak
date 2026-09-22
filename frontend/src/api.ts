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
