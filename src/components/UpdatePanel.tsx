import { useState } from 'react'
import { Button, Modal, Progress } from 'antd'
import { invoke } from '@tauri-apps/api/core'
import { errorText } from '../connection'
import type { useUpdates } from '../updates'

export function UpdatePanel({ updates }: { updates: ReturnType<typeof useUpdates> }) {
  const { info, status, check, install } = updates
  const [modal, context] = Modal.useModal()
  const [linkError, setLinkError] = useState('')
  const installing = status.state === 'downloading' || status.state === 'verifying' || status.state === 'installing'

  const openReleases = async () => {
    setLinkError('')
    try {
      await invoke('open_release_downloads')
    } catch (error) {
      setLinkError('打开发布页失败。' + errorText(error))
    }
  }

  const confirmInstall = async (version: string) => {
    if (await modal.confirm({
      title: '更新到 ' + version,
      content: '下载并校验完成后，ClipShare 会退出并启动安装程序。同步会暂时中断。',
      okText: '下载并更新',
      cancelText: '取消',
    })) await install(version)
  }

  return (
    <section className="update-panel" aria-labelledby="updates-heading">
      {context}
      <div className="update-heading">
        <h2 id="updates-heading">软件更新</h2>
        {info && <span className="muted">当前版本 {info.version}</span>}
      </div>
      {info && !info.supported && <p className="hint">请从 GitHub 下载适用于当前系统的安装包。</p>}
      {status.state === 'current' && <p className="hint" role="status">已是最新正式版。</p>}
      {status.state === 'no_manifest' && <p className="hint" role="status">目前没有可用的签名更新包。</p>}
      {status.state === 'error' && <p className="action-error" role="alert">{status.message}</p>}
      {linkError && <p className="action-error" role="alert">{linkError}</p>}
      {status.state === 'available' && (
        <div className="update-release">
          <strong role="status">发现新版 {status.version}</strong>
          {status.notes && <p className="update-notes">{status.notes}</p>}
        </div>
      )}
      {status.state === 'downloading' && (
        <div role="status" className="update-progress">
          <span className="hint">已下载 {(status.downloaded / (1024 * 1024)).toFixed(1)} MB</span>
          {status.total && <Progress percent={Math.min(100, Math.floor(status.downloaded / status.total * 100))} size="small" />}
        </div>
      )}
      {status.state === 'verifying' && <p className="hint" role="status">正在校验安装包签名…</p>}
      {status.state === 'installing' && <p className="hint" role="status">正在启动安装程序…</p>}
      <div className="update-actions">
        {info?.supported && (
          <Button size="small" onClick={() => void check()} disabled={installing} loading={status.state === 'checking'}>
            检查更新
          </Button>
        )}
        {status.state === 'available' && (
          <Button size="small" type="primary" onClick={() => void confirmInstall(status.version)}>
            下载并更新
          </Button>
        )}
        <Button size="small" type="link" onClick={() => void openReleases()}>查看 GitHub 发布</Button>
      </div>
    </section>
  )
}
