import { useEffect, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { isPermissionGranted, onAction, requestPermission, sendNotification } from '@tauri-apps/plugin-notification'
import { Badge, Modal, Tabs, Typography, notification } from 'antd'
import { ConfigPanel } from './components/ConfigPanel'
import { LogPanel } from './components/LogPanel'
import type { LogEntry } from './components/LogPanel'

interface WeChatMessage {
  id: string
  sender: string
  preview: string
  content: string
  timestamp: number
  unread_count: number
}

const { Title } = Typography

function truncatePreview(content: string, limit = 40) {
  return content.length <= limit ? content : `${Array.from(content).slice(0, limit).join('')}…`
}

function App() {
  const [activeTab, setActiveTab] = useState('config')
  const [logs, setLogs] = useState<LogEntry[]>([])
  const [selectedWeChat, setSelectedWeChat] = useState<WeChatMessage | null>(null)
  const [notificationApi, notificationContextHolder] = notification.useNotification()
  const wechatMessages = useRef(new Map<string, WeChatMessage>())

  useEffect(() => {
    invoke('start_clipboard_monitor').catch(console.error)
    invoke('start_wechat_monitor').catch(() => undefined)

    let disposed = false
    let stopChanged: (() => void) | undefined
    let stopReceived: (() => void) | undefined
    let stopWeChat: (() => void) | undefined
    let stopNotificationAction: (() => void) | undefined
    const appendLog = (entry: LogEntry) => setLogs(prev => [...prev.slice(-499), entry])

    const subscribeToLogs = async () => {
      let permissionGranted = await isPermissionGranted()
      if (!permissionGranted) permissionGranted = (await requestPermission()) === 'granted'

      const notificationActionListener = await onAction(action => {
        const messageId = action.extra?.messageId
        if (typeof messageId === 'string') {
          const message = wechatMessages.current.get(messageId)
          if (message) setSelectedWeChat(message)
        }
      })
      stopNotificationAction = () => { void notificationActionListener.unregister() }

      const [changed, received, wechat] = await Promise.all([
        listen<LogEntry>('clipboard-changed', event => appendLog(event.payload)),
        listen<LogEntry>('clipboard-received', event => {
          if (event.payload.dataType !== 'wechat') appendLog(event.payload)
        }),
        listen<WeChatMessage>('wechat-received', event => {
          wechatMessages.current.set(event.payload.id, event.payload)
          const preview = truncatePreview(event.payload.preview || event.payload.content)
          appendLog({
            time: new Date().toISOString(),
            type: 'recv',
            dataType: 'wechat',
            content: `${event.payload.sender}: ${preview}`,
            size: event.payload.content.length,
          })
          notificationApi.open({
            key: event.payload.id,
            message: `微信消息 · ${event.payload.sender}`,
            description: preview,
            placement: 'bottomRight',
            duration: 0,
            onClick: () => setSelectedWeChat(event.payload),
          })
          if (permissionGranted) {
            sendNotification({
              id: Math.abs(hashMessageId(event.payload.id)),
              title: `微信消息 · ${event.payload.sender}`,
              body: preview,
              extra: { messageId: event.payload.id },
              autoCancel: true,
            })
          }
        }),
      ])
      if (disposed) {
        changed()
        received()
        wechat()
        return
      }
      stopChanged = changed
      stopReceived = received
      stopWeChat = wechat

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
      stopWeChat?.()
      stopNotificationAction?.()
    }
  }, [notificationApi])

  const clearLogs = async () => {
    await invoke('clear_logs')
    setLogs([])
  }

  return (
    <div className="app-shell">
      {notificationContextHolder}
      <Modal
        open={selectedWeChat !== null}
        title={selectedWeChat ? `微信消息 · ${selectedWeChat.sender}` : '微信消息'}
        footer={null}
        onCancel={() => setSelectedWeChat(null)}
      >
        <p style={{ whiteSpace: 'pre-wrap', marginBottom: 0 }}>{selectedWeChat?.content}</p>
      </Modal>
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

function hashMessageId(value: string) {
  let hash = 0
  for (const character of value) hash = (hash * 31 + character.charCodeAt(0)) | 0
  return hash || 1
}

export default App
