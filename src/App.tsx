import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { Badge, Tabs, Typography } from 'antd'
import { ConfigPanel } from './components/ConfigPanel'
import { LogPanel } from './components/LogPanel'
import type { LogEntry } from './components/LogPanel'

const { Title } = Typography

function App() {
  const [activeTab, setActiveTab] = useState('config')
  const [logs, setLogs] = useState<LogEntry[]>([])

  useEffect(() => {
    invoke('start_clipboard_monitor').catch(console.error)

    let disposed = false
    let stopChanged: (() => void) | undefined
    let stopReceived: (() => void) | undefined
    const appendLog = (entry: LogEntry) => setLogs(prev => [...prev.slice(-499), entry])

    const subscribeToLogs = async () => {
      const [changed, received] = await Promise.all([
        listen<LogEntry>('clipboard-changed', event => appendLog(event.payload)),
        listen<LogEntry>('clipboard-received', event => appendLog(event.payload)),
      ])
      if (disposed) {
        changed()
        received()
        return
      }
      stopChanged = changed
      stopReceived = received

      try {
        const initialLogs = await invoke<LogEntry[]>('get_logs')
        setLogs(prev => {
          const existing = new Set(prev.map(entry => `${entry.time}:${entry.type}:${entry.content}`))
          const missing = initialLogs.filter(entry => !existing.has(`${entry.time}:${entry.type}:${entry.content}`))
          return [...missing, ...prev].slice(-500)
        })
      } catch (error) {
        console.error(error)
      }
    }
    subscribeToLogs().catch(console.error)
    return () => {
      disposed = true
      stopChanged?.()
      stopReceived?.()
    }
  }, [])

  const clearLogs = async () => {
    await invoke('clear_logs')
    setLogs([])
  }

  return (
    <div className="app-shell">
      <header className="app-header">
        <div>
          <div className="eyebrow">LOCAL NETWORK · CLIPBOARD SYNC</div>
          <Title level={3} className="app-title">ClipShare</Title>
        </div>
        <Badge status="processing" text={<span className="header-status">服务已就绪</span>} />
      </header>
      <div className="app-content">
        <Tabs
          className="main-tabs"
          activeKey={activeTab}
          onChange={setActiveTab}
          items={[
            { key: 'config', label: '连接配置', children: <ConfigPanel /> },
            { key: 'log', label: '传输日志', children: <LogPanel logs={logs} onClear={clearLogs} /> },
          ]}
        />
      </div>
    </div>
  )
}

export default App
