import { useEffect, useRef, useState } from 'react'
import { Button, Input, InputNumber, Modal, Radio, Select, type InputRef } from 'antd'
import { invoke } from '@tauri-apps/api/core'
import { DEFAULT_PORT, errorText, parseAddress, type Config, type ConnectionSnapshot } from '../connection'

interface Props {
  config: Config
  connection: ConnectionSnapshot | null
  lastTransfer: string | undefined
  onConnectionChange: () => Promise<void>
}

function connectionError(error: unknown) {
  const detail = errorText(error)
  if (/address already in use|only one usage|10048|地址已被使用|端口.*占用/i.test(detail)) {
    return '端口已被占用。请换一个监听端口，再重试。'
  }
  return '操作失败。' + detail
}

export function ConfigPanel({ config, connection, lastTransfer, onConnectionChange }: Props) {
  const [role, setRole] = useState(config.role === 'client' ? 'client' : 'server')
  const [address, setAddress] = useState(config.target_ip ? config.target_ip + ':' + config.target_port : '')
  const [port, setPort] = useState<number | null>(config.port)
  const [addresses, setAddresses] = useState<string[] | null>(null)
  const [selectedIp, setSelectedIp] = useState('')
  const [copiedAddress, setCopiedAddress] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const addressInput = useRef<InputRef>(null)
  const [modal, modalContext] = Modal.useModal()

  const loadAddresses = () => invoke<string[]>('get_local_addresses').then(setAddresses).catch(() => setAddresses([]))

  useEffect(() => {
    void loadAddresses()
  }, [])

  const active = connection !== null && connection.status !== 'disconnected'
  const endpoint = active ? connection.endpoint : null
  const parsedAddress = parseAddress(address)
  const target = parsedAddress ? parsedAddress.ip + ':' + parsedAddress.port : null
  const sameEndpoint = endpoint !== null && (
    endpoint.role === 'server' ? role === 'server' && endpoint.port === port : role === 'client' && endpoint.address === target
  )
  const ip = addresses?.includes(selectedIp) ? selectedIp : addresses?.[0]
  const addressPort = endpoint?.role === 'server' ? endpoint.port : port
  const localAddress = ip && addressPort ? ip + ':' + addressPort : ''
  const runtimeError = !error && role === 'server' && connection?.status === 'disconnected'
    && connection.reason === 'server bind failed'

  const copyAddress = async () => {
    try {
      await invoke('copy_local_text', { text: localAddress })
      setCopiedAddress(localAddress)
      setError('')
    } catch (cause) {
      setError('复制失败。' + errorText(cause))
    }
  }

  const stop = async () => {
    setBusy(true)
    setError('')
    try {
      await invoke('disconnect')
    } catch (cause) {
      setError(connectionError(cause))
    } finally {
      await onConnectionChange()
      setBusy(false)
    }
  }

  const connect = async () => {
    setError('')
    if (role === 'client' && !parsedAddress) {
      setError('请输入有效的地址，例如 192.168.1.20:9527。')
      addressInput.current?.focus()
      return
    }
    if (role === 'server' && (port === null || !Number.isInteger(port) || port < 1 || port > 65535)) {
      setError('请输入 1–65535 之间的监听端口。')
      return
    }

    setBusy(true)
    try {
      if (connection?.status === 'connected') {
        const confirmed = await modal.confirm({
          title: '切换连接',
          content: '当前同步将中断，随后使用新的连接设置。',
          okText: '切换连接',
          cancelText: '取消',
        })
        if (!confirmed) return
      }
      if (role === 'server') {
        await invoke('start_server', { port })
      } else if (parsedAddress) {
        await invoke('connect_to_server', parsedAddress)
      }
    } catch (cause) {
      setError(connectionError(cause))
    } finally {
      await onConnectionChange()
      setBusy(false)
    }
  }

  const stopLabel = connection?.status === 'connected'
    ? '断开连接'
    : endpoint?.role === 'server' ? '停止等待' : '停止连接'
  const connectLabel = role === 'server'
    ? endpoint?.role === 'server' ? '更改监听端口' : active ? '切换到等待' : '开始等待'
    : active ? '切换并连接' : '连接设备'

  return (
    <section className="connection-panel">
      {modalContext}
      <fieldset className="role-field">
        <legend>连接方式</legend>
        <Radio.Group
          aria-label="连接方式"
          className="role-switch"
          value={role}
          disabled={busy}
          onChange={event => { setRole(event.target.value); setError('') }}
        >
          <Radio.Button value="server">等待另一台连接</Radio.Button>
          <Radio.Button value="client">连接另一台</Radio.Button>
        </Radio.Group>
      </fieldset>

      {role === 'server' ? (
        <div className="address-card">
          <div className="address-heading">本机连接地址</div>
          <div className="address-row">
            {addresses && addresses.length > 1 ? (
              <Select
                aria-label="本机连接地址"
                className="address-select"
                value={ip}
                onChange={setSelectedIp}
                options={addresses.map(value => ({ value, label: addressPort ? value + ':' + addressPort : value }))}
                getPopupContainer={node => node.parentElement!}
              />
            ) : (
              <span className={localAddress ? 'address-value' : 'muted'}>
                {localAddress || (addresses === null ? '正在读取地址…' : '暂无可用地址')}
              </span>
            )}
            {localAddress ? (
              <Button onClick={() => void copyAddress()} aria-live="polite">
                {copiedAddress === localAddress ? '已复制' : '复制地址'}
              </Button>
            ) : addresses !== null && (
              <Button onClick={() => void loadAddresses()}>刷新地址</Button>
            )}
          </div>
          <p className="hint">
            {endpoint?.role === 'server'
              ? '在另一台设备上选择「连接另一台」，然后粘贴此地址。'
              : '开始等待后，在另一台设备上粘贴此地址。'}
          </p>
          <details className="port-details">
            <summary>监听端口</summary>
            <label htmlFor="listen-port">监听端口</label>
            <InputNumber
              id="listen-port"
              name="port"
              min={1}
              max={65535}
              step={1}
              precision={0}
              value={port}
              disabled={busy}
              onChange={value => { setPort(value); setError('') }}
            />
          </details>
        </div>
      ) : (
        <div className="address-card">
          <label className="address-heading" htmlFor="target-address">另一台设备的地址</label>
          <Input
            ref={addressInput}
            id="target-address"
            name="address"
            value={address}
            placeholder="例如 192.168.1.20:9527"
            autoComplete="off"
            spellCheck={false}
            disabled={busy}
            aria-describedby="address-hint"
            onChange={event => { setAddress(event.target.value); setError('') }}
            onPressEnter={() => { if (!busy && !sameEndpoint && connection) void connect() }}
          />
          <p id="address-hint" className="hint">粘贴对方的连接地址。只输入 IP 时使用端口 {DEFAULT_PORT}。</p>
        </div>
      )}

      {(error || runtimeError) && (
        <p role="alert" className="action-error">{error || '监听端口暂时无法使用。请检查端口后重试。'}</p>
      )}

      <div className="connection-actions">
        <div className="connection-facts">
          {connection?.status === 'connected' && (connection.peer || endpoint?.role === 'client') && (
            <span>对端 <strong>{connection.peer ?? (endpoint?.role === 'client' && endpoint.address)}</strong></span>
          )}
          {connection?.status === 'connecting' && endpoint?.role === 'client' && <span>目标 {endpoint.address}</span>}
          <span aria-live="polite" className="last-transfer">{lastTransfer ?? '连接后，复制即可同步'}</span>
        </div>
        <div className="connection-buttons">
          {active && <Button onClick={() => void stop()} disabled={busy}>{stopLabel}</Button>}
          {!sameEndpoint && (
            <Button type="primary" disabled={!connection} loading={busy} onClick={() => void connect()}>
              {connectLabel}
            </Button>
          )}
        </div>
      </div>
    </section>
  )
}
