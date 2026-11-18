import { useState } from 'react'
import { Button, Form, Input, InputNumber, Switch } from 'antd'
import { invoke } from '@tauri-apps/api/core'
import { errorText, type Config } from '../connection'

export function SettingsPanel({ config, onChange }: { config: Config; onChange: (config: Config) => void }) {
  const [form] = Form.useForm<Config>()
  const [busy, setBusy] = useState(false)
  const [notice, setNotice] = useState('')
  const [error, setError] = useState('')

  const toggleMonitor = async (enabled: boolean) => {
    setBusy(true)
    setNotice('')
    setError('')
    try {
      await invoke('set_wechat_monitor_enabled', { enabled })
      onChange({ ...config, wechat_enabled: enabled })
      setNotice(enabled ? '微信监听已开启' : '微信监听已关闭')
    } catch (cause) {
      form.setFieldValue('wechat_enabled', config.wechat_enabled)
      setError('微信监听切换失败。' + errorText(cause))
    } finally {
      setBusy(false)
    }
  }

  const save = async (values: Partial<Config>) => {
    setBusy(true)
    setNotice('')
    setError('')
    try {
      const updated = await invoke<Config>('save_preferences', { config: { ...config, ...values } })
      onChange(updated)
      setNotice('设置已保存')
    } catch (cause) {
      setError('保存失败。' + errorText(cause))
    } finally {
      setBusy(false)
    }
  }

  return (
    <Form
      className="settings-panel"
      form={form}
      layout="vertical"
      initialValues={config}
      disabled={busy}
      onFinish={values => void save(values)}
      onValuesChange={() => { setNotice(''); setError('') }}
    >
      <h2>微信消息</h2>
      <Form.Item name="wechat_enabled" label="监听本机微信" valuePropName="checked" extra="在运行微信的设备上开启，切换立即生效。">
        <Switch onChange={enabled => void toggleMonitor(enabled)} />
      </Form.Item>
      <Form.Item
        name="wechat_session_filter"
        label="监听指定个人或群"
        extra="每行填一个名称关键词。留空监听全部。"
      >
        <Input.TextArea rows={2} placeholder="例如 项目工作群" />
      </Form.Item>
      <Form.Item
        name="wechat_preview_limit"
        label="本机通知预览字数上限"
        rules={[{ required: true, type: 'integer', min: 1, max: 200, message: '请输入 1–200 之间的整数。' }]}
      >
        <InputNumber min={1} max={200} precision={0} />
      </Form.Item>
      <Form.Item name="wechat_show_content" label="点击本机通知时显示完整正文" valuePropName="checked">
        <Switch />
      </Form.Item>
      <h2>应用行为</h2>
      <Form.Item name="minimize_to_tray" label="关闭或最小化时隐藏到托盘" valuePropName="checked">
        <Switch />
      </Form.Item>
      <Form.Item name="autostart" label="登录 Windows 后自动启动到托盘" valuePropName="checked">
        <Switch />
      </Form.Item>
      <p className="hint">单条传输上限为 100 MB。</p>
      <div className="settings-actions">
        <Button htmlType="submit" type="primary" loading={busy}>保存设置</Button>
        {error && <span role="alert" className="action-error">{error}</span>}
        {notice && <span role="status" className="muted">{notice}</span>}
      </div>
    </Form>
  )
}
