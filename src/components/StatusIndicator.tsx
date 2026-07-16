import { Badge } from 'antd'

interface Props {
  status: string
}

const statusMap: Record<string, { text: string; color: string }> = {
  connected: { text: '在线', color: 'success' },
  connecting: { text: '连接中', color: 'processing' },
  disconnected: { text: '离线', color: 'default' },
}

export function StatusIndicator({ status }: Props) {
  const info = statusMap[status] || statusMap.disconnected
  return <Badge status={info.color as any} text={`连接状态: ${info.text}`} />
}
