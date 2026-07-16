import { useState } from 'react'
import { ConfigPanel } from './components/ConfigPanel'
import { LogPanel } from './components/LogPanel'
import { Tabs, Typography } from 'antd'

const { Title } = Typography

function App() {
  const [activeTab, setActiveTab] = useState('config')

  return (
    <div style={{ width: '100vw', height: '100vh', display: 'flex', flexDirection: 'column', background: '#f0f2f5' }}>
      <div style={{ background: 'linear-gradient(135deg, #667eea 0%, #764ba2 100%)', padding: '16px 20px' }}>
        <Title level={4} style={{ color: 'white', margin: 0, textAlign: 'center' }}>
          ClipShare 剪贴板共享
        </Title>
      </div>
      <div style={{ flex: 1, overflow: 'hidden' }}>
        <Tabs
          activeKey={activeTab}
          onChange={setActiveTab}
          style={{ height: '100%' }}
          tabBarStyle={{ paddingLeft: 16, marginBottom: 0, background: 'white' }}
          items={[
            { key: 'config', label: '🔧 配置', children: <ConfigPanel /> },
            { key: 'log', label: '📋 日志', children: <LogPanel /> },
          ]}
        />
      </div>
    </div>
  )
}

export default App
