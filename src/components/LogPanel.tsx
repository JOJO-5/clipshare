import { List, Tag, Button } from 'antd'

export interface LogEntry {
  time: string
  type: string
  dataType: string
  content: string
  size: number
}

const typeColors: Record<string, string> = {
  send: 'blue',
  recv: 'green',
  error: 'red',
}

const dataTypeIcons: Record<string, string> = {
  text: '📄',
  image: '🖼️',
  file: '📁',
}

interface LogPanelProps {
  logs: LogEntry[]
  onClear: () => void | Promise<void>
}

export function LogPanel({ logs, onClear }: LogPanelProps) {

  const formatTime = (iso: string) => {
    const d = new Date(iso)
    return d.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit', second: '2-digit' })
  }

  const formatSize = (bytes: number) => {
    if (bytes < 1024) return bytes + ' B'
    if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KB'
    return (bytes / 1024 / 1024).toFixed(2) + ' MB'
  }

  return (
    <div style={{ padding: '16px', height: '100%', display: 'flex', flexDirection: 'column' }}>
      <List
        style={{ flex: 1, overflow: 'auto' }}
        dataSource={logs}
        renderItem={(item) => (
          <List.Item>
            <List.Item.Meta
              avatar={<span style={{ fontSize: 16 }}>{dataTypeIcons[item.dataType] || '❓'}</span>}
              title={
                <span>
                  <Tag color={typeColors[item.type]}>
                    {item.type === 'send' ? '发送' : item.type === 'recv' ? '接收' : '错误'}
                  </Tag>
                  <span style={{ marginLeft: 8, color: '#666' }}>{formatTime(item.time)}</span>
                </span>
              }
              description={
                <span>
                  {item.content}
                  {item.size > 0 && <span style={{ color: '#999', marginLeft: 8 }}>{formatSize(item.size)}</span>}
                </span>
              }
            />
          </List.Item>
        )}
        locale={{ emptyText: '暂无日志' }}
      />
      <Button onClick={onClear} style={{ marginTop: 8 }}>清空日志</Button>
    </div>
  )
}
