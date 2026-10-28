import { invoke as tauriInvoke } from '@tauri-apps/api/core'
import { listen as tauriListen, type UnlistenFn } from '@tauri-apps/api/event'

export const isTauri = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
export const invoke = <T,>(cmd: string, args?: Record<string, unknown>) => tauriInvoke<T>(cmd, args)
export async function listen<T>(event: string, cb: (payload: T) => void): Promise<UnlistenFn> {
  if (!isTauri()) return () => {}
  return tauriListen<T>(event, (e) => cb(e.payload))
}
export function windowLabel(): string {
  if (isTauri()) {
    const w = (window as unknown as { __TAURI_INTERNALS__?: { metadata?: { currentWindow?: { label?: string } } } }).__TAURI_INTERNALS__
    return w?.metadata?.currentWindow?.label ?? 'settings'
  }
  return location.hash.replace('#', '') || 'whiteout-preview'
}
