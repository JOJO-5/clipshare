export const DEFAULT_PORT = 9527

export interface Config {
  role: string
  port: number
  target_ip: string
  target_port: number
  max_file_size: number
  language: string
  wechat_enabled: boolean
  wechat_preview_limit: number
  wechat_show_content: boolean
  wechat_session_filter: string
  minimize_to_tray: boolean
  autostart: boolean
}

export type ConnectionSnapshot = {
  status: 'disconnected' | 'connecting' | 'connected'
  endpoint: { role: 'server'; port: number } | { role: 'client'; address: string } | null
  peer: string | null
  reason: string
}

export function parseAddress(value: string) {
  const match = value.trim().match(/^(\[[^\]\s]+\]|[\w.-]+)(?::(\d{1,5}))?$/)
  if (!match) return null

  const [, ip, rawPort] = match
  if (/^[\d.]+$/.test(ip)) {
    const octets = ip.split('.')
    if (octets.length !== 4 || octets.some(part => !part || Number(part) > 255)) return null
  }

  const port = rawPort === undefined ? DEFAULT_PORT : Number(rawPort)
  if (port < 1 || port > 65535) return null
  return { ip, port }
}

export function errorText(error: unknown) {
  return error instanceof Error ? error.message : String(error)
}
