import React, { useState, useEffect, useRef } from 'react';
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

// Types
type Role = 'user' | 'agent' | 'assistant' | 'system' | 'tool';

interface Message {
  id: string;
  role: Role;
  content?: string;
  thought?: string;
  images?: string[];
  toolCall?: { name: string; args: any };
  toolResult?: { result: string };
  interactive?: { id: string; components: any[] };
}

interface CronJobInfo {
  id: string;
  schedule_type: string;
  expr: string;
  message: string;
}

type Tab = 'chat' | 'dashboard' | 'automation' | 'skills' | 'research' | 'database' | 'system' | 'settings';

export interface HealthStats {
  failure_rate?: number;
  last_latency?: string;
  is_healthy?: boolean;
  uptime?: number;
  connected_clients?: number;
  memory_usage?: number;
  total_tokens?: number;
  total_cost?: number;
  [key: string]: any;
}

function App() {
  const [activeTab, setActiveTab] = useState<Tab>('chat');
  const [socket, setSocket] = useState<WebSocket | null>(null);
  const [connected, setConnected] = useState(false);
  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState('');
  const [cronJobs, setCronJobs] = useState<CronJobInfo[]>([]);
  const [insights, setInsights] = useState<string[]>([]);
  const [healthStats, setHealthStats] = useState<HealthStats | null>(null);
  const [activeSwarms, setActiveSwarms] = useState<{id: string, role: string, status: string}[]>([]);
  const [availableModels, setAvailableModels] = useState<string[]>([]);
  const [currentModel, setCurrentModel] = useState<string>('');
  const [sessions, setSessions] = useState<string[]>([]);
  const [currentSession, setCurrentSession] = useState<string>('gateway');
  const [mcpStats, setMcpStats] = useState<any[]>([]);
  const [tools, setTools] = useState<any[]>([]);
  const [usageHistory, setUsageHistory] = useState<any[]>([]);
  const [settings, setSettings] = useState<any>(null);
  const [notebook, setNotebook] = useState<any>(null);
  const [graphData, setGraphData] = useState<string[]>([]);

  const messagesEndRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const handleServerEventRef = useRef<((event: any) => void) | null>(null);

  // Initialize WebSocket
  useEffect(() => {
    const ws = new WebSocket('ws://localhost:19999/ws');

    ws.onopen = () => {
      setConnected(true);
      ws.send(JSON.stringify({ type: "GetCronJobs" }));
      ws.send(JSON.stringify({ type: "GetModels" }));
      ws.send(JSON.stringify({ type: "GetOrchestration" }));
      ws.send(JSON.stringify({ type: "GetMcpStats" }));
      ws.send(JSON.stringify({ type: "GetGatewayStatus" }));
      ws.send(JSON.stringify({ type: "GetSessions" }));
      ws.send(JSON.stringify({ type: "GetTools" }));
      ws.send(JSON.stringify({ type: "GetUsageHistory" }));
      ws.send(JSON.stringify({ type: "GetSettings" }));
      ws.send(JSON.stringify({ type: "GetResearchNotebook" }));
      ws.send(JSON.stringify({ type: "GetGraphMemory", data: { query: "" } }));
    };

    ws.onclose = () => setConnected(false);

    ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data);
        if (handleServerEventRef.current) {
          handleServerEventRef.current(data);
        }
      } catch (e) {
        console.error("Error parsing WS message:", e);
      }
    };

    setSocket(ws);
    return () => ws.close();
  }, []);

  const parseContent = (content: any): { text: string; images: string[] } => {
    if (typeof content === 'string') return { text: content, images: [] };
    if (Array.isArray(content)) {
      let text = '';
      const images: string[] = [];
      content.forEach(part => {
        if (part.type === 'text') text += part.text;
        if (part.type === 'image_url') images.push(part.image_url.url);
      });
      return { text, images };
    }
    return { text: '', images: [] };
  };

  useEffect(() => {
    if (activeTab === 'chat') {
      messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    }
  }, [messages, activeTab]);

  handleServerEventRef.current = (event: any) => {
    const { type, data: payload } = event;

    switch (type) {
      case 'AgentThought':
        setMessages(prev => {
          const { text } = parseContent(payload.content);
          const lastMsg = prev[prev.length - 1];
          if (lastMsg && lastMsg.role === 'agent' && !lastMsg.content) {
            return [...prev.slice(0, -1), { ...lastMsg, thought: (lastMsg.thought || '') + text }];
          }
          return [...prev, { id: Math.random().toString(), role: 'agent', thought: text }];
        });
        break;

      case 'AgentResponse':
        setMessages(prev => {
          const { text, images } = parseContent(payload.content);
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
        setMessages(prev => [...prev, { id: Math.random().toString(), role: 'agent', toolCall: { name: payload.name, args: payload.args } }]);
        break;

      case 'ToolResult':
        setMessages(prev => [...prev, { id: Math.random().toString(), role: 'tool', toolResult: { result: payload.result } }]);
        break;

      case 'CronJobList': setCronJobs(payload.jobs); break;
      case 'TokenUsageUpdate':
        setHealthStats(prev => ({ ...prev, total_tokens: payload.total_tokens, total_cost: payload.total_cost }));
        break;
      case 'GatewayStatus':
        setHealthStats(prev => ({ ...prev, ...payload }));
        break;
      case 'SystemLog':
        setInsights(prev => [...prev.slice(-20), `[${payload.level}] ${payload.message}`]);
        break;
      case 'McpStats': setMcpStats(payload.stats); break;
      case 'OrchestrationState': setActiveSwarms(payload.sub_agents); break;
      case 'AgentInsight': setInsights(prev => [...prev.slice(-10), payload.insight]); break;
      case 'ModelList': setAvailableModels(payload.models); break;
      case 'ModelSwitched': setCurrentModel(payload.model_id); break;
      case 'SessionList': setSessions(payload.sessions); break;
      case 'ToolList': setTools(payload.tools); break;
      case 'UsageHistory': setUsageHistory(payload.history); break;
      case 'SettingsUpdate': setSettings(payload.settings); break;
      case 'ResearchNotebookUpdate': setNotebook(payload.notebook); break;
      case 'GraphUpdate': setGraphData(payload.relations); break;
      case 'ForensicLog':
        setInsights(prev => [...prev.slice(-30), `[FORENSIC] ${payload.action}: ${payload.hypothesis} ${payload.observation ? `-> ${payload.observation}` : ''}`]);
        break;
      case 'HistoryList':
        setMessages(payload.messages.map((m: any) => ({
          id: Math.random().toString(),
          role: m.role as Role,
          content: m.content?.Text || m.content || "",
          thought: m.thought
        })));
        break;
      case 'Error':
        setMessages(prev => [...prev, { id: Math.random().toString(), role: 'system', content: `Error: ${payload.message}` }]);
        break;
    }
  };

  const sendMessage = (e?: React.FormEvent) => {
    if (e) e.preventDefault();
    if (!input.trim() || !socket || !connected) return;
    setMessages(prev => [...prev, { id: Math.random().toString(), role: 'user', content: input }]);
    socket.send(JSON.stringify({ type: "SendMessage", data: { message: input } }));
    setInput('');
  };

  const switchSession = (id: string) => {
    setCurrentSession(id);
    socket?.send(JSON.stringify({ type: "SwitchSession", data: { id } }));
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
            <SoulConfigurator socket={socket} />
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
            style={{ flex: 1, display: 'flex', flexDirection: 'column' }}
          >
            {activeTab === 'chat' && (
              <ChatView
                messages={messages} input={input} setInput={setInput}
                sendMessage={sendMessage} handleKeyDown={(e) => e.key === 'Enter' && sendMessage()}
                handleInput={(e) => setInput(e.target.value)} connected={connected}
                clearMessages={() => setMessages([])} currentModel={currentModel}
                availableModels={availableModels} switchModel={(id) => socket?.send(JSON.stringify({type: "SwitchModel", data: {model_id: id}}))}
                textareaRef={textareaRef} messagesEndRef={messagesEndRef}
              />
            )}
            {activeTab === 'dashboard' && <DashboardView stats={healthStats} mcpStats={mcpStats} usageHistory={usageHistory} />}
            {activeTab === 'automation' && <AutomationView cronJobs={cronJobs} onCancel={(id) => socket?.send(JSON.stringify({type: "CancelCronJob", data: {id}}))} onRefresh={() => socket?.send(JSON.stringify({type: "GetCronJobs"}))} />}
            {activeTab === 'skills' && <SkillsView tools={tools} />}
            {activeTab === 'research' && <ResearchView notebook={notebook} />}
            {activeTab === 'database' && <DatabaseView relations={graphData} onSearch={(q) => socket?.send(JSON.stringify({type: "GetGraphMemory", data: {query: q}}))} />}
            {activeTab === 'system' && <SystemView insights={insights} />}
            {activeTab === 'settings' && (
              <SettingsView
                settings={settings}
                onUpdate={(s) => socket?.send(JSON.stringify({type: "UpdateSettings", data: {settings: s}}))}
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
