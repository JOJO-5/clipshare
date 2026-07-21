import { useEffect, useState } from 'react'
import { Button, Form, Input, InputNumber, Radio, Space, Switch, message } from 'antd'
import { invoke } from '@tauri-apps/api/core'
import { StatusIndicator } from './StatusIndicator'

interface Config {
  role: string
  port: number
  target_ip: string
  target_port: number
  max_file_size: number
  wechat_enabled: boolean
  wechat_preview_limit: number
  wechat_show_content: boolean
  minimize_to_tray: boolean
  autostart: boolean
}

export function ConfigPanel() {
  const [form] = Form.useForm<Config>()
  const role = Form.useWatch('role', form)
  const [loading, setLoading] = useState(false)
  const [status, setStatus] = useState('disconnected')

  useEffect(() => {
    void loadConfig()
    const interval = setInterval(() => invoke<string>('get_connection_status').then(setStatus).catch(() => {}), 2000)
    return () => clearInterval(interval)
  }, [])

  const loadConfig = async () => {
    try {
      const config = await invoke<Config>('get_config')
      form.setFieldsValue(config)
    } catch (error) {
      console.error('Failed to load config:', error)
    }
  }

  const handleSave = async () => {
    try {
      // `port` is not mounted while editing client mode, but the Rust command
      // accepts a complete AppConfig. Include the preserved form store and
      // fill any missing legacy values before crossing the Tauri IPC boundary.
      const values = form.getFieldsValue(true)
      await invoke('save_config', {
        config: {
          role: values.role ?? 'server',
          port: values.port ?? 9527,
          target_ip: values.target_ip ?? '',
          target_port: values.target_port ?? 9527,
          max_file_size: values.max_file_size ?? 104857600,
          language: 'zh-CN',
          wechat_enabled: values.wechat_enabled ?? true,
          wechat_preview_limit: values.wechat_preview_limit ?? 40,
          wechat_show_content: values.wechat_show_content ?? true,
          minimize_to_tray: values.minimize_to_tray ?? true,
          autostart: values.autostart ?? false,
        },
      })
      message.success('配置已保存')
    } catch (error) {
      message.error(`保存失败：${error}`)
    }
  }

  const handleConnect = async () => {
    try {
      const values = await form.validateFields()
      setLoading(true)
      if (values.role === 'server') {
        await invoke('start_server', { port: values.port })
        message.success('已开始监听连接')
      } else {
        await invoke('connect_to_server', { ip: values.target_ip, port: values.target_port })
        message.success('已连接到目标设备')
      }
    } catch (error) {
      if (error instanceof Error) message.error(`操作失败：${error.message}`)
    } finally {
      setLoading(false)
    }
  }

  const handleDisconnect = async () => {
    await invoke('disconnect')
    setStatus('disconnected')
    message.info('连接已断开')
  }

  return (
    <div className="config-panel">
      <div className="config-scroll">
        <div className="section-kicker">连接设置</div>
        <Form form={form} layout="vertical" initialValues={{ role: 'server', port: 9527, target_port: 9527, max_file_size: 104857600, wechat_enabled: true, wechat_preview_limit: 40, wechat_show_content: true, minimize_to_tray: true, autostart: false }}>
          <Form.Item name="role" label="运行模式">
            <Radio.Group className="role-switch">
              <Radio.Button value="server">接收端</Radio.Button>
              <Radio.Button value="client">发送端</Radio.Button>
            </Radio.Group>
          </Form.Item>

          <Form.Item noStyle shouldUpdate={(previous, current) => previous.role !== current.role}>
            {({ getFieldValue }) => getFieldValue('role') === 'server' ? (
              <Form.Item name="port" label="监听端口" rules={[{ required: true, message: '请输入监听端口' }]}>
                <InputNumber min={1} max={65535} addonBefore="TCP" className="form-control" />
              </Form.Item>
            ) : (
              <div className="client-fields">
                <Form.Item name="target_ip" label="目标 IP" rules={[{ required: true, message: '请输入目标 IP' }]}>
                  <Input placeholder="例如：192.168.1.100" className="form-control" />
                </Form.Item>
                <Form.Item name="target_port" label="目标端口" rules={[{ required: true, message: '请输入目标端口' }]}>
                  <InputNumber min={1} max={65535} addonBefore="TCP" className="form-control" />
                </Form.Item>
              </div>
            )}
          </Form.Item>

          <Form.Item name="max_file_size" label="文件传输上限">
            <InputNumber disabled addonAfter="MB" value={100} className="form-control" />
          </Form.Item>
          <div className="section-kicker">微信消息提示</div>
          <Form.Item name="wechat_enabled" label="启用微信消息提示" valuePropName="checked">
            <Switch />
          </Form.Item>
          <Form.Item name="wechat_preview_limit" label="通知摘要长度">
            <InputNumber min={1} max={200} addonAfter="字" className="form-control" />
          </Form.Item>
          <Form.Item name="wechat_show_content" label="点击后显示完整正文" valuePropName="checked">
            <Switch />
          </Form.Item>
          <Form.Item name="minimize_to_tray" label="关闭或最小化时隐藏到托盘" valuePropName="checked">
            <Switch />
          </Form.Item>
          <Form.Item name="autostart" label="开机自启（启动后最小化）" valuePropName="checked">
            <Switch />
          </Form.Item>
        </Form>
      </div>
      <footer className="config-actions">
        <StatusIndicator status={status} />
        <Space wrap>
          <Button onClick={handleSave}>保存配置</Button>
          <Button onClick={handleDisconnect}>断开</Button>
          <Button type="primary" onClick={handleConnect} loading={loading}>
            {role === 'server' ? '开始监听' : '连接设备'}
          </Button>
        </Space>
      </footer>
    </div>
  )
}
