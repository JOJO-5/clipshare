import { useCallback, useEffect, useRef, useState } from 'react'
import { Channel, invoke } from '@tauri-apps/api/core'
import { errorText } from './connection'

type UpdateInfo = { version: string; supported: boolean }
type Available = { state: 'available'; version: string; notes: string }
type CheckResult = { state: 'current' | 'no_manifest' } | Available
type Download = {
  state: 'downloading'
  downloaded: number
  total: number | null
}
type Progress = Download | { state: 'verifying' | 'installing' }
type UpdateState = CheckResult | Progress | { state: 'idle' | 'checking' } | { state: 'error'; message: string }

export function useUpdates() {
  const [info, setInfo] = useState<UpdateInfo | null>(null)
  const [status, setStatus] = useState<UpdateState>({ state: 'idle' })
  const busy = useRef(false)

  const check = useCallback(async () => {
    if (busy.current) return
    busy.current = true
    setStatus({ state: 'checking' })
    try {
      setStatus(await invoke<CheckResult>('check_for_update'))
    } catch (error) {
      setStatus({ state: 'error', message: '检查更新失败。' + errorText(error) })
    } finally {
      busy.current = false
    }
  }, [])

  const install = async (version: string) => {
    if (busy.current) return
    busy.current = true
    setStatus({ state: 'downloading', downloaded: 0, total: null })
    const progress = new Channel<Progress>()
    progress.onmessage = event => setStatus(event)
    try {
      await invoke('install_app_update', { version, progress })
    } catch (error) {
      setStatus({ state: 'error', message: '更新失败。' + errorText(error) })
    } finally {
      busy.current = false
    }
  }

  useEffect(() => {
    let disposed = false
    void invoke<UpdateInfo>('get_update_info').then(value => {
      if (!disposed) setInfo(value)
    }).catch(error => {
      if (!disposed) setStatus({ state: 'error', message: '读取更新状态失败。' + errorText(error) })
    })
    return () => { disposed = true }
  }, [])

  useEffect(() => {
    if (!info?.supported) return
    const startup = window.setTimeout(() => void check(), 2_000)
    const periodic = window.setInterval(() => void check(), 6 * 60 * 60 * 1_000)
    return () => {
      window.clearTimeout(startup)
      window.clearInterval(periodic)
    }
  }, [check, info?.supported])

  return { info, status, check, install }
}
