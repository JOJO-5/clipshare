import { useState, useEffect } from 'react'
import { Form, Radio, Input, InputNumber, Button, Space, message } from 'antd'
import { invoke } from '@tauri-apps/api/core'
import { StatusIndicator } from './StatusIndicator'

interface Config {
  role: string
  port: number
  target_ip: string
  target_port: number
  max_file_size: number
}

export function ConfigPanel() {
  const [form] = Form.useForm<Config>()
  const [loading, setLoading] = useState(false)
  const [status, setStatus] = useState('disconnected')

  useEffect(() => {
    loadConfig()
    const interval = setInterval(() => {
      invoke<string>('get_connection_status').then(setStatus).catch(() => {})
    }, 2000)
    return () => clearInterval(interval)
  }, [])

  const loadConfig = async () => {
    try {
      const config = await invoke<Config>('get_config')
      form.setFieldsValue({
        role: config.role,
        port: config.port,
        target_ip: config.target_ip,
        target_port: config.target_port,
        max_file_size: config.max_file_size,
      })
    } catch (e) {
      console.error('Failed to load config:', e)
    }
  }

  const handleSave = async () => {
    try {
      const values = form.getFieldsValue()
      await invoke('save_config', {
        config: {
          role: values.role,
          port: values.port,
          target_ip: values.target_ip || '',
          target_port: values.target_port,
          max_file_size: 104857600,
          language: 'zh-CN',
        }
      })
      message.success('配置已保存')
    } catch (e) {
      message.error('保存失败: ' + e)
    }
  }

  const handleConnect = async () => {
    setLoading(true)
    try {
      const values = form.getFieldsValue()
      if (values.role === 'server') {
        await invoke('start_server', { port: values.port })
        message.success('服务端已启动')
      } else {
        await invoke('connect_to_server', { ip: values.target_ip, port: values.target_port })
        message.success('已连接')
      }
    } catch (e) {
      message.error('操作失败: ' + e)
    } finally {
      setLoading(false)
    }
  }

  const handleDisconnect = async () => {
    await invoke('disconnect')
    message.info('已断开连接')
  }

  return (
    <div style={{ padding: '16px' }}>
      <Form form={form} layout="vertical" initialValues={{ role: 'server', port: 9527, target_port: 9527, max_file_size: 104857600 }}>
        <Form.Item name="role" label="运行模式">
          <Radio.Group>
            <Radio value="server">服务端（接收剪贴板）</Radio>
            <Radio value="client">客户端（发送剪贴板）</Radio>
          </Radio.Group>
        </Form.Item>

        <Form.Item name="port" label="监听端口">
          <InputNumber min={1} max={65535} style={{ width: 200 }} />
        </Form.Item>

        <Form.Item noStyle shouldUpdate={(prev, curr) => prev.role !== curr.role}>
          {({ getFieldValue }) =>
            getFieldValue('role') === 'client' && (
              <>
                <Form.Item name="target_ip" label="目标 IP" rules={[{ required: true }]}>
                  <Input placeholder="例如: 192.168.1.100" style={{ width: 200 }} />
                </Form.Item>
                <Form.Item name="target_port" label="目标端口">
                  <InputNumber min={1} max={65535} style={{ width: 200 }} />
                </Form.Item>
              </>
            )
          }
        </Form.Item>

        <Form.Item name="max_file_size" label="最大文件大小">
          <InputNumber value={100} disabled suffix="MB" style={{ width: 200 }} />
        </Form.Item>

        <Space style={{ marginTop: 16 }}>
          <Button type="primary" onClick={handleConnect} loading={loading}>
            {form.getFieldValue('role') === 'server' ? '启动监听' : '连接'}
          </Button>
          <Button onClick={handleDisconnect}>断开</Button>
          <Button onClick={handleSave}>保存配置</Button>
        </Space>

        <div style={{ marginTop: 16 }}>
          <StatusIndicator status={status} />
        </div>
      </Form>
    </div>
  )
}
