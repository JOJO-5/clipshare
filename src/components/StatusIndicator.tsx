import type { ConnectionSnapshot } from '../connection'

interface Props {
  connection: ConnectionSnapshot | null
  error: boolean
}

function statusText(connection: ConnectionSnapshot | null, error: boolean) {
  if (error) return '状态读取失败'
  if (!connection) return '读取连接状态…'
  if (connection.status === 'connected') return '已连接 · 双向同步中'
  if (connection.status === 'disconnected') return '已断开'
  if (connection.endpoint?.role === 'server') return '等待另一台设备连接'
  if (connection.reason.startsWith('connect failed')) return '连接失败 · 正在重试'
  if (connection.reason.includes('retrying')) return '连接中断 · 正在重试'
  return '正在连接…'
}

export function StatusIndicator({ connection, error }: Props) {
  return (
    <span className="connection-status" data-state={error ? 'error' : connection?.status} role="status">
      <span className="status-dot" aria-hidden="true" />
      {statusText(connection, error)}
    </span>
  )
}
