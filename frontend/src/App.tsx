import React, { useState, useEffect, useRef } from 'react';
import { Send, Clock, Trash2, Bot, Layout, Zap } from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';
import './App.css';
import CanvasRenderer from './CanvasRenderer';
import type { CanvasPrimitive } from './CanvasRenderer';
import MessageItem from './components/MessageItem';
import HealthMonitor from './components/HealthMonitor';
import SoulConfigurator from './components/SoulConfigurator';
import SkillBrowser from './components/SkillBrowser';

// Types
type Role = 'user' | 'agent' | 'system';

interface Message {
  id: string;
  role: Role;
  content?: string;
  thought?: string;
  images?: string[];
  toolCall?: { name: string; args: any };
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
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Initialize WebSocket
  useEffect(() => {
    // Mock Swarms for UI demonstration
    setActiveSwarms([
      { id: '1', role: 'RESEARCHER', status: 'Analyzing trajectory for implicit goals...' }
    ]);

    const ws = new WebSocket('ws://localhost:18789/ws');

    ws.onopen = () => {
      setConnected(true);
      // Request initial cron jobs list
      ws.send(JSON.stringify({ type: "GetCronJobs" }));
    };

    ws.onclose = () => setConnected(false);

    ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data);
        handleServerEvent(data);
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

  const handleServerEvent = (event: any) => {
    const { type, payload } = event;

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
              content: (lastMsg.content || '') + text,
              images: [...(lastMsg.images || []), ...images]
            }];
          }
          return [...prev, { id: Math.random().toString(), role: 'agent', content: text, images }];
        });
        break;

      case 'ToolCall':
        setMessages(prev => [
          ...prev, 
          { id: Math.random().toString(), role: 'agent', toolCall: { name: payload.name, args: payload.args } }
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

      case 'AgentInsight':
        setInsights(prev => [...prev.slice(-4), payload.insight]);
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
      payload: { message: input }
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
      payload: { id }
    }));
  };

  const refreshJobs = () => {
    if (!socket || !connected) return;
    socket.send(JSON.stringify({ type: "GetCronJobs" }));
  };

  return (
    <div className="app-container premium-theme">
      {/* Left Sidebar: Sessions & Metrics */}
      <aside className="sidebar left-sidebar glass-panel">
        <div className="sidebar-section">
          <div className="section-header">
            <Layout size={18} />
            <span>SESSIONS</span>
          </div>
          <div className="session-list">
            {['Current Session', 'Research - Project X', 'Debug Log - May 5'].map((s, i) => (
              <div key={i} className={`session-item ${i === 0 ? 'active' : ''}`}>
                <Clock size={14} />
                <span>{s}</span>
              </div>
            ))}
          </div>
        </div>
        
        <div className="sidebar-section metrics-section">
          <HealthMonitor stats={healthStats} />
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
            {[
              { role: 'SUPERVISOR', name: 'Coordination', status: 'Active' },
              { role: 'RESEARCHER', name: 'Knowledge', status: 'Idle' },
              { role: 'CODER', name: 'Implementation', status: 'Active' },
            ].map((s, i) => (
              <div key={i} className="swarm-card">
                <div className="card-header">
                  <span className="role-tag">{s.role}</span>
                  <div className={`status-dot ${s.status === 'Active' ? 'online' : 'offline'}`} />
                </div>
                <div className="card-body">
                  <span className="agent-name">{s.name}</span>
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
