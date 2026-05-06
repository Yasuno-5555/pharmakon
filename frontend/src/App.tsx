import React, { useState, useEffect, useRef } from 'react';
import { Send, Clock, Trash2, Bot, Layout, Zap, Plus, Search } from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';
import './App.css';
import CanvasRenderer from './CanvasRenderer';
import type { CanvasPrimitive } from './CanvasRenderer';
import MessageItem from './components/MessageItem';
import HealthMonitor from './components/HealthMonitor';
import SoulConfigurator from './components/SoulConfigurator';
import SkillBrowser from './components/SkillBrowser';

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

function App() {
  const [socket, setSocket] = useState<WebSocket | null>(null);
  const [connected, setConnected] = useState(false);
  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState('');
  const [cronJobs, setCronJobs] = useState<CronJobInfo[]>([]);
  const [canvasPrimitives, setCanvasPrimitives] = useState<CanvasPrimitive[]>([]);
  const [insights, setInsights] = useState<string[]>([]);
  const [healthStats, setHealthStats] = useState<any>(null);
  const [activeSwarms, setActiveSwarms] = useState<{id: string, role: string, status: string}[]>([]);
  const [availableModels, setAvailableModels] = useState<string[]>([]);
  const [currentModel, setCurrentModel] = useState<string>('');
  const [sessions, setSessions] = useState<string[]>([]);
  const [currentSession, setCurrentSession] = useState<string>('gateway');
  const [searchQuery, setSearchQuery] = useState('');
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const handleServerEventRef = useRef<((event: any) => void) | null>(null);

  // Initialize WebSocket
  useEffect(() => {
    // Mock Swarms for UI demonstration
    setActiveSwarms([
      { id: '1', role: 'RESEARCHER', status: 'Analyzing trajectory for implicit goals...' }
    ]);

    const ws = new WebSocket('ws://localhost:19999/ws');

    ws.onopen = () => {
      setConnected(true);
      // Request initial state from gateway
      ws.send(JSON.stringify({ type: "GetCronJobs" }));
      ws.send(JSON.stringify({ type: "GetModels" }));
      ws.send(JSON.stringify({ type: "GetOrchestration" }));
      ws.send(JSON.stringify({ type: "GetMcpStats" }));
      ws.send(JSON.stringify({ type: "GetGatewayStatus" }));
      ws.send(JSON.stringify({ type: "GetSessions" }));
    };

    ws.onclose = () => setConnected(false);

    ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data);
        console.log("WS Received:", data.type, data);
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

  // Auto-scroll chat
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

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
              content: text, // Overwrite with final content
              images: [...(lastMsg.images || []), ...images]
            }];
          }
          return [...prev, { id: Math.random().toString(), role: 'agent', content: text, images }];
        });
        break;

      case 'AgentThoughtChunk':
        setMessages(prev => {
          const lastMsg = prev[prev.length - 1];
          if (lastMsg && lastMsg.role === 'agent' && !lastMsg.content) {
            return [...prev.slice(0, -1), { ...lastMsg, thought: (lastMsg.thought || '') + payload.chunk }];
          }
          return [...prev, { id: Math.random().toString(), role: 'agent', thought: payload.chunk }];
        });
        break;

      case 'AgentResponseChunk':
        setMessages(prev => {
          const lastMsg = prev[prev.length - 1];
          if (lastMsg && lastMsg.role === 'agent') {
            return [...prev.slice(0, -1), { ...lastMsg, content: (lastMsg.content || '') + payload.chunk }];
          }
          return [...prev, { id: Math.random().toString(), role: 'agent', content: payload.chunk }];
        });
        break;

      case 'ToolCall':
        setMessages(prev => [
          ...prev, 
          { id: Math.random().toString(), role: 'agent', toolCall: { name: payload.name, args: payload.args } }
        ]);
        break;

      case 'ToolResult':
        setMessages(prev => [
          ...prev,
          { id: Math.random().toString(), role: 'tool', toolResult: { result: payload.result } }
        ]);
        break;

      case 'CronJobList':
        setCronJobs(payload.jobs);
        break;

      case 'CanvasUpdate':
        setCanvasPrimitives(prev => [...prev, payload.primitive]);
        break;

      case 'CanvasClear':
        setCanvasPrimitives([]);
        break;
        
      case 'InteractiveElements':
        setMessages(prev => [
          ...prev,
          { id: Math.random().toString(), role: 'system', interactive: payload }
        ]);
        break;

      case 'HealthUpdate':
        setHealthStats(payload);
        break;
        
      case 'OrchestrationState':
        setActiveSwarms(payload.sub_agents.map((a: any) => ({
          id: a.name,
          role: a.role,
          status: a.status
        })));
        break;

      case 'AgentInsight':
        setInsights(prev => [...prev.slice(-4), payload.insight]);
        break;
        
      case 'ModelList':
        setAvailableModels(payload.models);
        break;

      case 'ModelSwitched':
        setCurrentModel(payload.model_id);
        setMessages(prev => [...prev, { id: Math.random().toString(), role: 'system', content: `Model switched to ${payload.model_id}` }]);
        break;

      case 'HistoryList':
        setMessages(payload.messages.map((m: any) => {
          let contentStr = "";
          if (m.content) {
            if (m.content.Text) contentStr = m.content.Text;
            else if (Array.isArray(m.content.Multimodal)) {
              contentStr = m.content.Multimodal.find((p: any) => p.type === 'text')?.text || "";
            } else if (typeof m.content === 'string') {
              contentStr = m.content;
            }
          }
          
          return {
            id: Math.random().toString(),
            role: (m.role === 'assistant' ? 'agent' : m.role) as Role,
            content: contentStr,
            thought: m.thought,
            toolCall: m.tool_calls?.[0] ? { 
              name: m.tool_calls[0].function.name, 
              args: typeof m.tool_calls[0].function.arguments === 'string' 
                ? JSON.parse(m.tool_calls[0].function.arguments) 
                : m.tool_calls[0].function.arguments 
            } : undefined,
            toolResult: m.role === 'tool' ? { result: contentStr } : undefined
          };
        }));
        break;

      case 'SessionList':
        setSessions(payload.sessions);
        break;

      case 'Action':
        if (payload === 'Session switched') {
          // Handled by HistoryList
        }
        break;

      case 'Error':
        setMessages(prev => [...prev, { id: Math.random().toString(), role: 'system', content: `Error: ${payload.message}` }]);
        break;
    }
  };

  const sendMessage = (e?: React.FormEvent) => {
    if (e) e.preventDefault();
    if (!input.trim() || !socket || !connected) return;

    // Add user message to UI
    setMessages(prev => [...prev, { id: Math.random().toString(), role: 'user', content: input }]);
    
    // Send to backend
    socket.send(JSON.stringify({
      type: "SendMessage",
      data: { message: input }
    }));
    
    setInput('');
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto';
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      sendMessage();
    }
  };

  const handleInput = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    setInput(e.target.value);
    e.target.style.height = 'auto';
    e.target.style.height = `${Math.min(e.target.scrollHeight, 200)}px`;
  };

  const cancelJob = (id: string) => {
    if (!socket || !connected) return;
    socket.send(JSON.stringify({
      type: "CancelCronJob",
      data: { id }
    }));
  };

  const refreshJobs = () => {
    if (!socket || !connected) return;
    socket.send(JSON.stringify({ type: "GetCronJobs" }));
  };

  const switchModel = (modelId: string) => {
    if (!socket || !connected) return;
    socket.send(JSON.stringify({ type: "SwitchModel", data: { model_id: modelId } }));
  };

  const switchSession = (sessionId: string) => {
    if (!socket || !connected) return;
    setCurrentSession(sessionId);
    socket.send(JSON.stringify({ type: "SwitchSession", data: { id: sessionId } }));
  };

  const startNewSession = () => {
    const newId = `session-${Date.now().toString().slice(-6)}`;
    switchSession(newId);
    // Backend will create the session on the first message or explicit switch
    setSessions(prev => [...prev, newId]);
  };

  const searchSessions = (query: string) => {
    setSearchQuery(query);
    if (!socket || !connected) return;
    socket.send(JSON.stringify({ type: "SearchSessions", data: { query } }));
  };

  return (
    <div className="app-container premium-theme">
      {/* Left Sidebar: Sessions & Metrics */}
      <aside className="sidebar left-sidebar glass-panel">
        <div className="sidebar-section">
          <div className="section-header">
            <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
              <Layout size={18} />
              <span>SESSIONS</span>
            </div>
            <button 
              className="icon-btn small-btn" 
              onClick={startNewSession}
              title="Start New Session"
              style={{ padding: '4px', background: 'var(--accent-muted)', borderRadius: '4px', color: '#fff' }}
            >
              <Plus size={14} />
            </button>
          </div>
          <div className="search-bar-mini" style={{ padding: '0 12px 12px' }}>
            <div className="search-input-wrapper" style={{ position: 'relative' }}>
              <Search size={14} style={{ position: 'absolute', left: '8px', top: '50%', transform: 'translateY(-50%)', opacity: 0.5 }} />
              <input 
                type="text" 
                placeholder="Search history..." 
                value={searchQuery}
                onChange={(e) => searchSessions(e.target.value)}
                style={{ width: '100%', padding: '6px 8px 6px 28px', fontSize: '12px', background: 'rgba(255,255,255,0.05)', border: '1px solid rgba(255,255,255,0.1)', borderRadius: '6px', color: '#fff' }}
              />
            </div>
          </div>
          <div className="session-list">
            {sessions.map((s) => (
              <div 
                key={s} 
                className={`session-item ${s === currentSession ? 'active' : ''}`}
                onClick={() => switchSession(s)}
              >
                <Clock size={14} />
                <span>{s}</span>
              </div>
            ))}
            {sessions.length === 0 && <div className="session-item-muted">No saved sessions</div>}
          </div>
        </div>
        
        <div className="sidebar-section metrics-section">
          <HealthMonitor stats={healthStats} />
        </div>

        <div className="sidebar-section">
          <SoulConfigurator socket={socket} />
        </div>

        <div className="sidebar-section">
          <SkillBrowser />
        </div>
      </aside>

      {/* Main Content: Chat & Console */}
      <main className="main-content">
        <div className="chat-section glass-panel">
          <header className="chat-header">
            <div className="agent-info">
              <div className="agent-avatar">P</div>
              <div>
                <h2>Pharmakon Supervisor</h2>
                <span className="status-text">Ready for instructions</span>
              </div>
            </div>
            <div className="header-actions">
              <div className="model-selector">
                <Bot size={14} />
                <select 
                  value={currentModel} 
                  onChange={(e) => switchModel(e.target.value)}
                  className="model-select-dropdown"
                >
                  <option value="" disabled>Select Model</option>
                  {availableModels.map(m => (
                    <option key={m} value={m}>{m}</option>
                  ))}
                </select>
              </div>
              <Zap size={18} className={connected ? 'pulsing' : ''} />
              <button onClick={() => setMessages([])}><Trash2 size={18} /></button>
            </div>
          </header>

          <div className="chat-messages">
            <AnimatePresence initial={false}>
              {messages.length === 0 ? (
                <div className="empty-state">
                  <Bot size={48} />
                  <p>Awaiting sequence deployment.</p>
                </div>
              ) : (
                messages.map(msg => (
                  <MessageItem key={msg.id} msg={msg} socket={socket} />
                ))
              )}
            </AnimatePresence>
            <div ref={messagesEndRef} />
          </div>

          <footer className="chat-input-area">
            <textarea
              ref={textareaRef}
              className="chat-input"
              placeholder="Deploy instruction..."
              value={input}
              onChange={handleInput}
              onKeyDown={handleKeyDown}
              rows={1}
            />
            <button onClick={sendMessage} className="send-btn"><Send size={18} /></button>
          </footer>
        </div>

        {/* Bottom Console */}
        <div className="console-panel glass-panel">
          <div className="console-header">
            <span>CONSOLE</span>
            <span className="gateway-ip">Gateway: 100.64.0.1 (Tailscale)</span>
          </div>
          <div className="console-body">
            {insights.map((insight, i) => (
              <div key={`insight-${i}`} className="log-line insight-line">
                <span className="log-tag">[INSIGHT]</span> {insight}
              </div>
            ))}
            {['[14:21:05] Initiating web search query...', 
              '[14:21:08] Researcher: Analyzing results...',
              '[14:21:12] Coder: Writing module_A.rs...'].map((log, i) => (
              <div key={i} className="log-line">{log}</div>
            ))}
          </div>
        </div>
      </main>

      {/* Right Sidebar: Swarm & Cron */}
      <aside className="sidebar right-sidebar glass-panel">
        <div className="sidebar-section">
          <div className="section-header">
            <Zap size={18} />
            <span>SUB-AGENT SWARM</span>
          </div>
          <div className="swarm-list">
            {activeSwarms.map((s, i) => (
              <div key={i} className="swarm-card">
                <div className="card-header">
                  <span className="role-tag">{s.role}</span>
                  <div className={`status-dot ${s.status === 'Active' ? 'online' : 'offline'}`} />
                </div>
                <div className="card-body">
                  <span className="agent-name">{s.id}</span>
                  <p className="agent-status-text">Status: {s.status}</p>
                </div>
              </div>
            ))}
          </div>
        </div>

        <div className="sidebar-section">
          <div className="section-header">
            <Clock size={18} />
            <span>CRON SEQUENCES</span>
          </div>
          <div className="cron-list-mini">
            {cronJobs.map(job => (
              <div key={job.id} className="cron-item-mini">
                <span className="cron-expr">{job.expr}</span>
                <span className="cron-msg">{job.message}</span>
              </div>
            ))}
          </div>
        </div>
      </aside>
    </div>
  );
}

export default App;
