import { useState } from 'react'
import { Button, Popconfirm } from 'antd'
import { invoke } from '@tauri-apps/api/core'
import { errorText } from '../connection'

export interface LogEntry {
  time: string
  type: string
  dataType: string
  content: string
  size: number
}

const timeFormatter = new Intl.DateTimeFormat('zh-CN', {
  hour: '2-digit', minute: '2-digit', second: '2-digit', hour12: false,
})

export function formatLogTime(iso: string) {
  const date = new Date(iso)
  return Number.isNaN(date.valueOf()) ? iso : timeFormatter.format(date)
}

const kinds: Record<string, string> = { text: '文本', image: '图片', file: '文件', wechat: '微信消息' }
const actions: Record<string, string> = { send: '已发送', recv: '已接收', error: '失败', info: '诊断' }

function formatSize(bytes: number) {
  if (bytes < 1024) return bytes + ' B'
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KB'
  return (bytes / 1024 / 1024).toFixed(1) + ' MB'
}

export function LogPanel({ logs, onClear }: { logs: LogEntry[]; onClear: () => Promise<void> }) {
  const [showDiagnostics, setShowDiagnostics] = useState(false)
  const [notice, setNotice] = useState('')
  const [error, setError] = useState('')
  const entries = (showDiagnostics ? [...logs] : logs.filter(entry => entry.type !== 'info')).reverse()

  const copyDiagnostics = async () => {
    setError('')
    setNotice('')
    try {
      const text = logs.map(entry => JSON.stringify(entry)).join('\n')
      await invoke('copy_local_text', { text })
      setNotice('诊断信息已复制')
    } catch (cause) {
      setError('复制失败。' + errorText(cause))
    }
  }

  const clear = async () => {
    setError('')
    setNotice('')
    try {
      await onClear()
      setNotice('已清空本次记录')
    } catch (cause) {
      setError('清空失败。' + errorText(cause))
    }
  }

  return (
    <section className="log-panel" aria-label="传输记录">
      <div className="log-toolbar">
        <Button
          size="small"
          aria-pressed={showDiagnostics}
          onClick={() => setShowDiagnostics(value => !value)}
        >
          {showDiagnostics ? '隐藏诊断' : '显示诊断'}
        </Button>
        <Button size="small" disabled={!logs.length} onClick={() => void copyDiagnostics()}>
          复制诊断信息
        </Button>
      </div>
      {error && <p role="alert" className="action-error">{error}</p>}
      {notice && <p role="status" className="hint">{notice}</p>}
      {entries.length ? (
        <ul className="log-list">
          {entries.map((entry, index) => (
            <li className="log-entry" key={entry.time + entry.type + ':' + (entries.length - index)}>
              <div className="log-entry-heading">
                <span className="log-kind" data-kind={entry.type}>
                  {actions[entry.type] ?? '记录'}{kinds[entry.dataType] ? ' · ' + kinds[entry.dataType] : ''}
                </span>
                <time dateTime={entry.time}>{formatLogTime(entry.time)}</time>
              </div>
              <p>{entry.content}</p>
              {entry.size > 0 && <span className="log-size">{formatSize(entry.size)}</span>}
            </li>
          ))}
        </ul>
      ) : (
        <div className="log-empty">
          <p>{showDiagnostics ? '暂无记录' : '暂无传输记录'}</p>
          <p className="hint">连接后，复制文本、图片或文件即可同步。</p>
        </div>
      )}
      <div className="log-footer">
        <Popconfirm
          title="清空本次记录？"
          description="此操作会清空当前列表，磁盘诊断日志仍保留。"
          okText="清空"
          cancelText="取消"
          onConfirm={clear}
          disabled={!logs.length}
        >
          <Button size="small" disabled={!logs.length}>清空本次记录</Button>
        </Popconfirm>
      </div>
    </section>
  )
}
