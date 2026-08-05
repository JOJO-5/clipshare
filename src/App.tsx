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
const WECHAT_NOTIFICATION_DEDUP_WINDOW_MS = 30_000
const WECHAT_NATIVE_NOTIFICATION_COOLDOWN_MS = 30_000

function truncatePreview(content: string, limit = 40) {
  return content.length <= limit ? content : `${Array.from(content).slice(0, limit).join('')}…`
}

function wechatNotificationKey(message: WeChatMessage) {
  if (message.id.startsWith('wechat-session-')) {
    return `${message.sender}\u0000${message.content}\u0000${message.unread_count}`
  }
  return message.id
}

function wechatNotificationGroup(message: WeChatMessage) {
  const sender = message.sender.trim().toLowerCase()
  return `wechat-sender-${sender || 'unknown'}`
}

function shouldSendNativeWechatNotification(lastSentAt: number | undefined, now: number) {
  return lastSentAt === undefined || now - lastSentAt >= WECHAT_NATIVE_NOTIFICATION_COOLDOWN_MS
}

function combineWechatMessages(messages: WeChatMessage[]) {
  const latest = messages[messages.length - 1]
  if (!latest) throw new Error('Cannot combine an empty WeChat message group')
  if (messages.length === 1) return latest
  return {
    ...latest,
    content: messages
      .map((message, index) => `[${index + 1}] ${message.content}`)
      .join('\n\n'),
  }
}

function App() {
  const [activeTab, setActiveTab] = useState('config')
  const [logs, setLogs] = useState<LogEntry[]>([])
  const [selectedWeChat, setSelectedWeChat] = useState<WeChatMessage | null>(null)
  const [notificationApi, notificationContextHolder] = notification.useNotification()
  const wechatMessages = useRef(new Map<string, WeChatMessage>())
  const wechatNotificationKeys = useRef(new Map<string, number>())
  const wechatNotificationGroups = useRef(new Map<string, WeChatMessage[]>())
  const wechatNativeNotificationTimes = useRef(new Map<string, number>())

  useEffect(() => {
    let disposed = false
    let stopChanged: (() => void) | undefined
    let stopReceived: (() => void) | undefined
    let stopWeChat: (() => void) | undefined
    let stopWeChatMonitorLog: (() => void) | undefined
    let stopClipboardMonitorLog: (() => void) | undefined
    let stopNetworkStatusLog: (() => void) | undefined
    let stopNotificationAction: (() => void) | undefined
    const appendLog = (entry: LogEntry) => setLogs(prev => [...prev.slice(-499), entry])

    const subscribeToLogs = async () => {
      let permissionGranted = false
      try {
        permissionGranted = await isPermissionGranted()
        if (!permissionGranted) permissionGranted = (await requestPermission()) === 'granted'
      } catch {
        permissionGranted = false
      }
      appendLog({
        time: new Date().toISOString(),
        type: 'info',
        dataType: 'wechat',
        content: `wechat-notification permission=${permissionGranted ? 'granted' : 'denied'}`,
        size: 0,
      })

      try {
        const notificationActionListener = await onAction(action => {
          const messageId = action.extra?.messageId
          if (typeof messageId === 'string') {
            const message = wechatMessages.current.get(messageId)
            if (message) {
              const groupKey = wechatNotificationGroup(message)
              const groupedMessages = wechatNotificationGroups.current.get(groupKey) ?? [message]
              wechatNotificationGroups.current.delete(groupKey)
              wechatNativeNotificationTimes.current.delete(groupKey)
              notificationApi.destroy(groupKey)
              setSelectedWeChat(combineWechatMessages(groupedMessages))
            }
          }
        })
        stopNotificationAction = () => { void notificationActionListener.unregister() }
      } catch {
        stopNotificationAction = undefined
      }

      const [changed, received, wechat, wechatMonitorLog, clipboardMonitorLog, networkStatusLog] = await Promise.all([
        listen<LogEntry>('clipboard-changed', event => appendLog(event.payload)),
        listen<LogEntry>('clipboard-received', event => {
          if (event.payload.dataType !== 'wechat') appendLog(event.payload)
        }),
        listen<WeChatMessage>('wechat-received', event => {
          wechatMessages.current.set(event.payload.id, event.payload)
          const notificationKey = wechatNotificationKey(event.payload)
          const now = Date.now()
          for (const [key, timestamp] of wechatNotificationKeys.current) {
            if (now - timestamp >= WECHAT_NOTIFICATION_DEDUP_WINDOW_MS) {
              wechatNotificationKeys.current.delete(key)
            }
          }
          const previousNotification = wechatNotificationKeys.current.get(notificationKey)
          const isDuplicate = previousNotification !== undefined
            && now - previousNotification < WECHAT_NOTIFICATION_DEDUP_WINDOW_MS
          wechatNotificationKeys.current.set(notificationKey, now)
          if (isDuplicate) return

          const preview = truncatePreview(event.payload.preview || event.payload.content)
          const groupKey = wechatNotificationGroup(event.payload)
          const shouldSendNativeNotification = shouldSendNativeWechatNotification(
            wechatNativeNotificationTimes.current.get(groupKey),
            now,
          )
          const groupedMessages = [
            ...(wechatNotificationGroups.current.get(groupKey) ?? []),
            event.payload,
          ].slice(-50)
          const groupedCount = groupedMessages.length
          wechatNotificationGroups.current.set(groupKey, groupedMessages)
          const groupedDescription = groupedCount > 1
            ? `${groupedCount} 条新消息\n${preview}`
            : preview
          appendLog({
            time: new Date().toISOString(),
            type: 'recv',
            dataType: 'wechat',
            content: `${event.payload.sender}: ${preview}`,
            size: event.payload.content.length,
          })
          notificationApi.open({
            key: groupKey,
            message: `微信消息 · ${event.payload.sender}`,
            description: groupedDescription,
            placement: 'bottomRight',
            duration: 0,
            onClick: () => {
              const messages = wechatNotificationGroups.current.get(groupKey) ?? [event.payload]
              wechatNotificationGroups.current.delete(groupKey)
              wechatNativeNotificationTimes.current.delete(groupKey)
              notificationApi.destroy(groupKey)
              setSelectedWeChat(combineWechatMessages(messages))
            },
          })
          if (permissionGranted && shouldSendNativeNotification) {
            wechatNativeNotificationTimes.current.set(groupKey, now)
            try {
              sendNotification({
              id: Math.abs(hashMessageId(groupKey)),
              title: `微信消息 · ${event.payload.sender}`,
              body: groupedDescription,
              extra: { messageId: event.payload.id },
              autoCancel: true,
              })
              appendLog({
                time: new Date().toISOString(),
                type: 'info',
                dataType: 'wechat',
                content: `wechat-notification status=sent sender=${event.payload.sender}`,
                size: 0,
              })
            } catch (error: unknown) {
              appendLog({
                time: new Date().toISOString(),
                type: 'error',
                dataType: 'wechat',
                content: `wechat-notification status=failed error=${String(error)}`,
                size: 0,
              })
            }
          }
        }),
        listen<LogEntry>('wechat-monitor-log', event => appendLog(event.payload)),
        listen<LogEntry>('clipboard-monitor-log', event => appendLog(event.payload)),
        listen<LogEntry>('network-status-log', event => appendLog(event.payload)),
      ])
      if (disposed) {
        changed()
        received()
        wechat()
        wechatMonitorLog()
        clipboardMonitorLog()
        networkStatusLog()
        return
      }
      stopChanged = changed
      stopReceived = received
      stopWeChat = wechat
      stopWeChatMonitorLog = wechatMonitorLog
      stopClipboardMonitorLog = clipboardMonitorLog
      stopNetworkStatusLog = networkStatusLog

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
    subscribeToLogs()
      .then(() => {
        if (disposed) return
        void invoke('start_clipboard_monitor').catch(console.error)
        void invoke('start_wechat_monitor').catch(console.error)
      })
      .catch(console.error)
    return () => {
      disposed = true
      stopChanged?.()
      stopReceived?.()
      stopWeChat?.()
      stopWeChatMonitorLog?.()
      stopClipboardMonitorLog?.()
      stopNetworkStatusLog?.()
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
