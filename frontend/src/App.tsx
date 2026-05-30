import React, { useEffect, useEffectEvent, useRef, useState } from 'react';
import {
  Clock, Zap, Layout, Terminal,
  BarChart3, Settings, Package, MessageSquare, BookOpen
} from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';
import './App.css';
import HealthMonitor from './components/HealthMonitor';
import SoulConfigurator from './components/SoulConfigurator';

// Views
import ChatView from './views/ChatView';
import DashboardView from './views/DashboardView';
import AutomationView from './views/AutomationView';
import SkillsView from './views/SkillsView';
import SystemView from './views/SystemView';
import SettingsView from './views/SettingsView';
import ResearchView from './views/ResearchView';
import DatabaseView from './views/DatabaseView';
import type {
  AppSettings,
  CronJobInfo,
  HealthStats,
  McpStatEntry,
  Message,
  ResearchNotebook,
  ServerEvent,
  SwarmStatus,
  ToolInfo,
  UsageEntry,
} from './types';

type Tab = 'chat' | 'dashboard' | 'automation' | 'skills' | 'research' | 'database' | 'system' | 'settings';

interface ContentPart {
  type: 'text' | 'image_url';
  text?: string;
  image_url?: {
    url: string;
  };
}

interface HistoryMessage {
  role: string;
  content?: string | { Text?: string };
  thought?: string;
}

interface EventPayloadMap {
  AgentThought: { content: string | ContentPart[] };
  AgentResponse: { content: string | ContentPart[] };
  ToolCall: { name: string; args: Record<string, unknown> };
  ToolResult: { result: string };
  CronJobList: { jobs: CronJobInfo[] };
  TokenUsageUpdate: { total_tokens: number; total_cost: number };
  GatewayStatus: HealthStats;
  SystemLog: { level: string; message: string };
  McpStats: { stats: McpStatEntry[] };
  OrchestrationState: { sub_agents: SwarmStatus[] };
  AgentInsight: { insight: string };
  ModelList: { models: string[] };
  ModelSwitched: { model_id: string };
  SessionList: { sessions: string[] };
  ToolList: { tools: ToolInfo[] };
  UsageHistory: { history: UsageEntry[] };
  SettingsUpdate: { settings: AppSettings };
  ResearchNotebookUpdate: { notebook: ResearchNotebook };
  GraphUpdate: { relations: string[] };
  ForensicLog: { action: string; hypothesis: string; observation?: string };
  HistoryList: { messages: HistoryMessage[] };
  Error: { message: string };
}

type EventType = keyof EventPayloadMap;
type TypedServerEvent<K extends EventType> = {
  type: K;
  data: EventPayloadMap[K];
};

function App() {
  const [activeTab, setActiveTab] = useState<Tab>('chat');
  const [connected, setConnected] = useState(false);
  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState('');
  const [cronJobs, setCronJobs] = useState<CronJobInfo[]>([]);
  const [insights, setInsights] = useState<string[]>([]);
  const [healthStats, setHealthStats] = useState<HealthStats | null>(null);
  const [activeSwarms, setActiveSwarms] = useState<SwarmStatus[]>([]);
  const [availableModels, setAvailableModels] = useState<string[]>([]);
  const [currentModel, setCurrentModel] = useState<string>('');
  const [sessions, setSessions] = useState<string[]>([]);
  const [currentSession, setCurrentSession] = useState<string>('gateway');
  const [mcpStats, setMcpStats] = useState<McpStatEntry[]>([]);
  const [tools, setTools] = useState<ToolInfo[]>([]);
  const [usageHistory, setUsageHistory] = useState<UsageEntry[]>([]);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [notebook, setNotebook] = useState<ResearchNotebook | null>(null);
  const [graphData, setGraphData] = useState<string[]>([]);

  const socketRef = useRef<WebSocket | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const sendWs = (payload: object) => {
    socketRef.current?.send(JSON.stringify(payload));
  };

  const parseContent = (content: string | ContentPart[] | undefined): { text: string; images: string[] } => {
    if (typeof content === 'string') return { text: content, images: [] };
    if (Array.isArray(content)) {
      let text = '';
      const images: string[] = [];
      content.forEach((part) => {
        if (part.type === 'text' && part.text) text += part.text;
        if (part.type === 'image_url' && part.image_url?.url) images.push(part.image_url.url);
      });
      return { text, images };
    }
    return { text: '', images: [] };
  };

  const handleServerEvent = useEffectEvent((event: ServerEvent) => {
    const type = event.type as EventType;
    const payload = event.data as EventPayloadMap[EventType];

    switch (type) {
      case 'AgentThought':
        setMessages(prev => {
          const { text } = parseContent((payload as TypedServerEvent<'AgentThought'>['data']).content);
          const lastMsg = prev[prev.length - 1];
          if (lastMsg && lastMsg.role === 'agent' && !lastMsg.content) {
            return [...prev.slice(0, -1), { ...lastMsg, thought: (lastMsg.thought || '') + text }];
          }
          return [...prev, { id: Math.random().toString(), role: 'agent', thought: text }];
        });
        break;

      case 'AgentResponse':
        setMessages(prev => {
          const { text, images } = parseContent((payload as TypedServerEvent<'AgentResponse'>['data']).content);
          const lastMsg = prev[prev.length - 1];
          if (lastMsg && lastMsg.role === 'agent') {
            return [...prev.slice(0, -1), {
              ...lastMsg,
              content: text,
              images: [...(lastMsg.images || []), ...images]
            }];
          }
          return [...prev, { id: Math.random().toString(), role: 'agent', content: text, images }];
        });
        break;

      case 'ToolCall':
        setMessages(prev => [...prev, {
          id: Math.random().toString(),
          role: 'agent',
          toolCall: {
            name: (payload as TypedServerEvent<'ToolCall'>['data']).name,
            args: (payload as TypedServerEvent<'ToolCall'>['data']).args,
          },
        }]);
        break;

      case 'ToolResult':
        setMessages(prev => [...prev, {
          id: Math.random().toString(),
          role: 'tool',
          toolResult: { result: (payload as TypedServerEvent<'ToolResult'>['data']).result },
        }]);
        break;

      case 'CronJobList': setCronJobs((payload as TypedServerEvent<'CronJobList'>['data']).jobs); break;
      case 'TokenUsageUpdate':
        setHealthStats(prev => ({
          ...(prev ?? {}),
          total_tokens: (payload as TypedServerEvent<'TokenUsageUpdate'>['data']).total_tokens,
          total_cost: (payload as TypedServerEvent<'TokenUsageUpdate'>['data']).total_cost,
        }));
        break;
      case 'GatewayStatus': setHealthStats(prev => ({ ...(prev ?? {}), ...(payload as HealthStats) })); break;
      case 'SystemLog':
        setInsights(prev => [...prev.slice(-20), `[${(payload as TypedServerEvent<'SystemLog'>['data']).level}] ${(payload as TypedServerEvent<'SystemLog'>['data']).message}`]);
        break;
      case 'McpStats': setMcpStats((payload as TypedServerEvent<'McpStats'>['data']).stats); break;
      case 'OrchestrationState': setActiveSwarms((payload as TypedServerEvent<'OrchestrationState'>['data']).sub_agents); break;
      case 'AgentInsight': setInsights(prev => [...prev.slice(-10), (payload as TypedServerEvent<'AgentInsight'>['data']).insight]); break;
      case 'ModelList': setAvailableModels((payload as TypedServerEvent<'ModelList'>['data']).models); break;
      case 'ModelSwitched': setCurrentModel((payload as TypedServerEvent<'ModelSwitched'>['data']).model_id); break;
      case 'SessionList': setSessions((payload as TypedServerEvent<'SessionList'>['data']).sessions); break;
      case 'ToolList': setTools((payload as TypedServerEvent<'ToolList'>['data']).tools); break;
      case 'UsageHistory': setUsageHistory((payload as TypedServerEvent<'UsageHistory'>['data']).history); break;
      case 'SettingsUpdate': setSettings((payload as TypedServerEvent<'SettingsUpdate'>['data']).settings); break;
      case 'ResearchNotebookUpdate': setNotebook((payload as TypedServerEvent<'ResearchNotebookUpdate'>['data']).notebook); break;
      case 'GraphUpdate': setGraphData((payload as TypedServerEvent<'GraphUpdate'>['data']).relations); break;
      case 'ForensicLog':
        setInsights(prev => {
          const forensic = payload as TypedServerEvent<'ForensicLog'>['data'];
          return [...prev.slice(-30), `[FORENSIC] ${forensic.action}: ${forensic.hypothesis} ${forensic.observation ? `-> ${forensic.observation}` : ''}`];
        });
        break;
      case 'HistoryList':
        setMessages((payload as TypedServerEvent<'HistoryList'>['data']).messages.map((m) => ({
          id: Math.random().toString(),
          role: m.role as Message['role'],
          content: typeof m.content === 'string' ? m.content : m.content?.Text || '',
          thought: m.thought,
        })));
        break;
      case 'Error':
        setMessages(prev => [...prev, {
          id: Math.random().toString(),
          role: 'system',
          content: `Error: ${(payload as TypedServerEvent<'Error'>['data']).message}`,
        }]);
        break;
    }
  });

  useEffect(() => {
    const ws = new WebSocket('ws://localhost:19999/ws');
    socketRef.current = ws;

    ws.onopen = () => {
      setConnected(true);
      [
        { type: 'GetCronJobs' },
        { type: 'GetModels' },
        { type: 'GetOrchestration' },
        { type: 'GetMcpStats' },
        { type: 'GetGatewayStatus' },
        { type: 'GetSessions' },
        { type: 'GetTools' },
        { type: 'GetUsageHistory' },
        { type: 'GetSettings' },
        { type: 'GetResearchNotebook' },
        { type: 'GetGraphMemory', data: { query: '' } },
      ].forEach((request) => ws.send(JSON.stringify(request)));
    };

    ws.onclose = () => setConnected(false);
    ws.onmessage = (event) => {
      try {
        handleServerEvent(JSON.parse(event.data) as ServerEvent);
      } catch (error) {
        console.error('Error parsing WS message:', error);
      }
    };

    return () => {
      socketRef.current = null;
      ws.close();
    };
  }, []);

  const sendMessage = (e?: React.FormEvent) => {
    if (e) e.preventDefault();
    if (!input.trim() || !socketRef.current || !connected) return;
    setMessages(prev => [...prev, { id: Math.random().toString(), role: 'user', content: input }]);
    sendWs({ type: 'SendMessage', data: { message: input } });
    setInput('');
  };

  const switchSession = (id: string) => {
    setCurrentSession(id);
    sendWs({ type: 'SwitchSession', data: { id } });
  };

  return (
    <div className="app-container premium-theme">
      {/* Far-Left Navigation Sidebar */}
      <nav className="nav-sidebar">
        <div className="nav-logo">Φ</div>
        <div
          className={`nav-item ${activeTab === 'chat' ? 'active' : ''}`}
          onClick={() => setActiveTab('chat')}
          title="Chat"
        >
          <MessageSquare size={20} />
        </div>
        <div
          className={`nav-item ${activeTab === 'dashboard' ? 'active' : ''}`}
          onClick={() => setActiveTab('dashboard')}
          title="Statistics"
        >
          <BarChart3 size={20} />
        </div>
        <div
          className={`nav-item ${activeTab === 'automation' ? 'active' : ''}`}
          onClick={() => setActiveTab('automation')}
          title="Automation"
        >
          <Clock size={20} />
        </div>
        <div
          className={`nav-item ${activeTab === 'skills' ? 'active' : ''}`}
          onClick={() => setActiveTab('skills')}
          title="Skills"
        >
          <Package size={20} />
        </div>
        <div
          className={`nav-item ${activeTab === 'research' ? 'active' : ''}`}
          onClick={() => setActiveTab('research')}
          title="Deep Research"
        >
          <BookOpen size={20} />
        </div>
        <div
          className={`nav-item ${activeTab === 'database' ? 'active' : ''}`}
          onClick={() => setActiveTab('database')}
          title="Knowledge Graph"
        >
          <Zap size={20} />
        </div>
        <div className="nav-spacer" style={{ flex: 1 }} />
        <div
          className={`nav-item ${activeTab === 'system' ? 'active' : ''}`}
          onClick={() => setActiveTab('system')}
          title="System Logs"
        >
          <Terminal size={20} />
        </div>
        <div
          className={`nav-item ${activeTab === 'settings' ? 'active' : ''}`}
          onClick={() => setActiveTab('settings')}
          title="Settings"
        >
          <Settings size={20} />
        </div>
      </nav>

      {/* Middle Sidebar: View-specific context */}
      <aside className="sidebar left-sidebar glass-panel">
        {activeTab === 'chat' ? (
          <>
            <div className="sidebar-section">
              <div className="section-header">
                <Layout size={18} /> <span>SESSIONS</span>
              </div>
              <div className="session-list">
                {sessions.map(s => (
                  <div key={s} className={`session-item ${s === currentSession ? 'active' : ''}`} onClick={() => switchSession(s)}>
                    <Clock size={14} /> <span>{s}</span>
                  </div>
                ))}
              </div>
            </div>
            <HealthMonitor stats={healthStats} />
            <SoulConfigurator
              onSync={(traits, systemPrompt) => sendWs({
                type: 'UpdateSoul',
                payload: {
                  traits,
                  system_prompt: systemPrompt,
                },
              })}
            />
          </>
        ) : (
          <div className="sidebar-section">
            <div className="section-header">QUICK STATS</div>
            <div className="glass-panel" style={{ padding: '12px', fontSize: '0.8rem' }}>
              <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: '8px' }}>
                <span>Status</span> <span style={{ color: '#4ade80' }}>ONLINE</span>
              </div>
              <div style={{ display: 'flex', justifyContent: 'space-between' }}>
                <span>Clients</span> <span>1</span>
              </div>
            </div>
          </div>
        )}
      </aside>

      {/* Main View Area */}
      <main className="main-content">
        <AnimatePresence mode="wait">
          <motion.div
            key={activeTab}
            initial={{ opacity: 0, x: 10 }}
            animate={{ opacity: 1, x: 0 }}
            exit={{ opacity: 0, x: -10 }}
            transition={{ duration: 0.2 }}
            style={{ flex: 1, display: 'flex', flexDirection: 'column', height: '100%' }}
          >
            {activeTab === 'chat' && (
              <ChatView
                messages={messages} input={input} setInput={setInput}
                sendMessage={sendMessage} handleKeyDown={(e) => e.key === 'Enter' && sendMessage()}
                handleInput={(e) => setInput(e.target.value)} connected={connected}
                clearMessages={() => setMessages([])} currentModel={currentModel}
                availableModels={availableModels} switchModel={(id) => sendWs({ type: 'SwitchModel', data: { model_id: id } })}
                textareaRef={textareaRef} messagesEndRef={messagesEndRef}
                onInteractiveResponse={(elementId, action) => sendWs({ type: 'InteractiveResponse', data: { element_id: elementId, action, value: null } })}
              />
            )}
            {activeTab === 'dashboard' && <DashboardView stats={healthStats} mcpStats={mcpStats} usageHistory={usageHistory} />}
            {activeTab === 'automation' && <AutomationView cronJobs={cronJobs} onCancel={(id) => sendWs({ type: 'CancelCronJob', data: { id } })} onRefresh={() => sendWs({ type: 'GetCronJobs' })} />}
            {activeTab === 'skills' && <SkillsView tools={tools} />}
            {activeTab === 'research' && <ResearchView notebook={notebook} />}
            {activeTab === 'database' && <DatabaseView relations={graphData} onSearch={(q) => sendWs({ type: 'GetGraphMemory', data: { query: q } })} />}
            {activeTab === 'system' && <SystemView insights={insights} />}
            {activeTab === 'settings' && (
              <SettingsView
                settings={settings}
                onUpdate={(s) => sendWs({ type: 'UpdateSettings', data: { settings: s } })}
              />
            )}
          </motion.div>
        </AnimatePresence>
      </main>

      {/* Right Sidebar: Contextual Inspector */}
      {activeTab === 'chat' && (
        <aside className="sidebar right-sidebar glass-panel">
          <div className="sidebar-section">
            <div className="section-header"><Zap size={18} /> <span>SWARM STATUS</span></div>
            <div className="swarm-list">
              {activeSwarms.map((s, i) => (
                <div key={i} className="swarm-card">
                  <span className="role-tag">{s.role}</span>
                  <span className="agent-name">{s.id}</span>
                  <p className="agent-status-text">{s.status}</p>
                </div>
              ))}
            </div>
          </div>
        </aside>
      )}
    </div>
  );
}

export default App;
