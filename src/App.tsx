import { useCallback, useEffect, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { isPermissionGranted, onAction, requestPermission, sendNotification } from '@tauri-apps/plugin-notification'
import { Button, Modal, Tabs, notification } from 'antd'
import { ConfigPanel } from './components/ConfigPanel'
import { formatLogTime, LogPanel } from './components/LogPanel'
import type { LogEntry } from './components/LogPanel'
import { SettingsPanel } from './components/SettingsPanel'
import { StatusIndicator } from './components/StatusIndicator'
import { errorText, type Config, type ConnectionSnapshot } from './connection'

interface WeChatMessage {
  id: string
  sender: string
  preview: string
  content: string
  timestamp: number
  unread_count: number
}

const WECHAT_NOTIFICATION_DEDUP_WINDOW_MS = 30_000
const WECHAT_NATIVE_NOTIFICATION_COOLDOWN_MS = 30_000

function truncatePreview(content: string, limit = 40) {
  if (content.length <= limit) return content
  let preview = ''
  let length = 0
  for (const character of content) {
    if (length === limit) return `${preview}…`
    preview += character
    length++
  }
  return content
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
    preview: messages.map(message => message.preview).join('\n\n'),
    content: messages
      .map((message, index) => `[${index + 1}] ${message.content}`)
      .join('\n\n'),
  }
}

function latestClipboardTransfer(logs: LogEntry[]) {
  const kinds: Record<string, string> = { text: '文本', image: '图片', file: '文件' }
  for (let index = logs.length - 1; index >= 0; index--) {
    const entry = logs[index]
    const kind = kinds[entry.dataType]
    if (kind && (entry.type === 'send' || entry.type === 'recv')) {
      return `最近${entry.type === 'send' ? '发送' : '接收'}${kind} · ${formatLogTime(entry.time)}`
    }
  }
}

function App() {
  const [config, setConfig] = useState<Config | null>(null)
  const [configError, setConfigError] = useState('')
  const [connection, setConnection] = useState<ConnectionSnapshot | null>(null)
  const [connectionError, setConnectionError] = useState(false)
  const [logs, setLogs] = useState<LogEntry[]>([])
  const [selectedWeChat, setSelectedWeChat] = useState<WeChatMessage | null>(null)
  const [notificationApi, notificationContextHolder] = notification.useNotification()
  const wechatMessages = useRef(new Map<string, WeChatMessage>())
  const wechatNotificationKeys = useRef(new Map<string, number>())
  const wechatNotificationGroups = useRef(new Map<string, WeChatMessage[]>())
  const wechatNativeNotificationTimes = useRef(new Map<string, number>())
  const currentConfig = useRef<Config | null>(null)
  const connectionRequest = useRef(0)

  const updateConfig = useCallback((value: Config) => {
    currentConfig.current = value
    setConfig(value)
  }, [])

  const loadConfig = useCallback(async () => {
    setConfigError('')
    try {
      updateConfig(await invoke<Config>('get_config'))
    } catch (error) {
      setConfigError('读取设置失败。' + errorText(error))
    }
  }, [updateConfig])

  const refreshConnection = useCallback(async () => {
    const request = ++connectionRequest.current
    try {
      const info = await invoke<ConnectionSnapshot>('get_connection_info')
      if (request !== connectionRequest.current) return
      setConnection(info)
      setConnectionError(false)
    } catch {
      if (request !== connectionRequest.current) return
      setConnection(null)
      setConnectionError(true)
    }
  }, [])

  useEffect(() => {
    void loadConfig()
    void refreshConnection()
    return () => { connectionRequest.current++ }
  }, [loadConfig, refreshConnection])

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
      if (disposed) return
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
      if (disposed) {
        stopNotificationAction?.()
        return
      }

      const [changed, received, wechat, wechatMonitorLog, clipboardMonitorLog, networkStatusLog] = await Promise.all([
        listen<LogEntry>('clipboard-changed', event => appendLog(event.payload)),
        listen<LogEntry>('clipboard-received', event => {
          if (event.payload.dataType !== 'wechat') appendLog(event.payload)
        }),
        listen<WeChatMessage>('wechat-received', event => {
          const preview = truncatePreview(
            event.payload.preview || event.payload.content,
            currentConfig.current?.wechat_preview_limit ?? 40,
          )
          const receivedMessage = { ...event.payload, preview }
          wechatMessages.current.set(receivedMessage.id, receivedMessage)
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

          const groupKey = wechatNotificationGroup(event.payload)
          const shouldSendNativeNotification = shouldSendNativeWechatNotification(
            wechatNativeNotificationTimes.current.get(groupKey),
            now,
          )
          const groupedMessages = [
            ...(wechatNotificationGroups.current.get(groupKey) ?? []),
            receivedMessage,
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
            size: 0,
          })
          const openMessage = () => {
            const messages = wechatNotificationGroups.current.get(groupKey) ?? [receivedMessage]
            wechatNotificationGroups.current.delete(groupKey)
            wechatNativeNotificationTimes.current.delete(groupKey)
            notificationApi.destroy(groupKey)
            setSelectedWeChat(combineWechatMessages(messages))
          }
          notificationApi.open({
            key: groupKey,
            message: `微信消息 · ${event.payload.sender}`,
            description: groupedDescription,
            placement: 'bottomRight',
            duration: 0,
            onClick: openMessage,
            actions: (
              <Button size="small" onClick={event => { event.stopPropagation(); openMessage() }}>
                查看消息
              </Button>
            ),
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
        listen<LogEntry>('network-status-log', event => {
          appendLog(event.payload)
          void refreshConnection()
        }),
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
      void refreshConnection()

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
  }, [notificationApi, refreshConnection])

  const clearLogs = async () => {
    const visible = new Set(logs)
    await invoke('clear_logs')
    setLogs(current => current.filter(entry => !visible.has(entry)))
  }

  const configPlaceholder = configError ? (
    <div className="loading-panel">
      <p role="alert">{configError}</p>
      <Button onClick={() => void loadConfig()}>重试</Button>
    </div>
  ) : <p role="status" className="loading-panel">正在读取设置…</p>

  return (
    <div className="app-shell">
      {notificationContextHolder}
      <Modal
        open={selectedWeChat !== null}
        title={selectedWeChat ? `微信消息 · ${selectedWeChat.sender}` : '微信消息'}
        footer={null}
        onCancel={() => setSelectedWeChat(null)}
      >
        <p className="wechat-detail">
          {config?.wechat_show_content ? selectedWeChat?.content : selectedWeChat?.preview}
        </p>
        {!config?.wechat_show_content && <p className="hint">当前仅显示通知预览，可在「设置」中更改。</p>}
      </Modal>
      <header className="app-header">
        <h1>ClipShare</h1>
        <div className="header-status">
          <StatusIndicator connection={connection} error={connectionError} />
          {connectionError && <Button size="small" onClick={() => void refreshConnection()}>重试</Button>}
        </div>
      </header>
      <main className="app-content">
        <Tabs
          className="main-tabs"
          items={[
            {
              key: 'config', label: '设备连接', children: config ? (
                <ConfigPanel config={config} connection={connection} lastTransfer={latestClipboardTransfer(logs)} onConnectionChange={refreshConnection} />
              ) : configPlaceholder,
            },
            { key: 'log', label: '传输记录', children: <LogPanel logs={logs} onClear={clearLogs} /> },
            { key: 'settings', label: '设置', children: config ? <SettingsPanel config={config} onChange={updateConfig} /> : configPlaceholder },
          ]}
        />
      </main>
    </div>
  )
}

function hashMessageId(value: string) {
  let hash = 0
  for (const character of value) hash = (hash * 31 + character.charCodeAt(0)) | 0
  return hash || 1
}

export default App
