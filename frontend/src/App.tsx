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
    <div className="app-container">
      <div className="chat-section glass-panel">
        <div className="chat-header">
          <motion.div animate={{ rotate: connected ? 360 : 0 }} transition={{ duration: 2, repeat: Infinity, ease: "linear" }}>
            <Zap size={24} color="var(--accent)" fill={connected ? "var(--accent)" : "none"} />
          </motion.div>
          <h1>Pharmakon Interface</h1>
          <div className="status-indicator">
            <div className={`status-dot ${connected ? 'connected' : 'disconnected'}`}></div>
            {connected ? 'Active' : 'Offline'}
          </div>
          <button onClick={() => setMessages([])} className="header-action-btn" title="Clear history">
            <Trash2 size={18} />
          </button>
        </div>

        <div className="chat-messages">
          <AnimatePresence initial={false}>
            {messages.length === 0 ? (
              <motion.div 
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                className="empty-state" 
                style={{ marginTop: 'auto', marginBottom: 'auto' }}
              >
                <Bot size={64} opacity={0.2} />
                <p>Ready for sequence deployment. How can I assist?</p>
              </motion.div>
            ) : (
              messages.map(msg => (
                <MessageItem key={msg.id} msg={msg} socket={socket} />
              ))
            )}
          </AnimatePresence>
          <div ref={messagesEndRef} />
        </div>

        <form onSubmit={sendMessage} className="chat-input-area">
          <textarea
            ref={textareaRef}
            className="chat-input"
            placeholder="Type a message or press '/' for commands..."
            value={input}
            onChange={handleInput}
            onKeyDown={handleKeyDown}
            disabled={!connected}
            rows={1}
            style={{ resize: 'none', overflow: 'hidden' }}
          />
          <button type="submit" className="send-btn" disabled={!connected || !input.trim()}>
            <Send size={20} />
          </button>
        </form>
      </div>

      <div className="sidebar">
        <div className="panel-header">
          <Layout size={20} color="var(--accent)" />
          Operations Hub
        </div>

        <div className="sidebar-scroll">
          <HealthMonitor stats={healthStats} />
          
          <div className="swarm-panel glass-panel" style={{ margin: '0 20px' }}>
            <div className="panel-header" style={{ borderBottom: '1px solid var(--border-color)', fontSize: '0.9rem' }}>
              <Bot size={18} color="var(--accent)" />
              Autonomy Matrix
            </div>
            <div className="cron-list" style={{ padding: '12px' }}>
              <AnimatePresence>
                {activeSwarms.length === 0 ? (
                  <div style={{ textAlign: 'center', color: 'var(--text-muted)', fontSize: '0.8rem' }}>No active sub-agents.</div>
                ) : (
                  activeSwarms.map(swarm => (
                    <motion.div 
                      key={swarm.id} 
                      initial={{ opacity: 0, scale: 0.9 }}
                      animate={{ opacity: 1, scale: 1 }}
                      exit={{ opacity: 0, scale: 0.9 }}
                      className="cron-job-card"
                    >
                      <div className="cron-header">
                        <span className="cron-type" style={{ background: 'rgba(139, 92, 246, 0.2)', color: '#a78bfa' }}>{swarm.role}</span>
                        <div className="status-dot connected" style={{ width: '6px', height: '6px' }}></div>
                      </div>
                      <div className="cron-message">{swarm.status}</div>
                    </motion.div>
                  ))
                )}
              </AnimatePresence>
            </div>
          </div>

          <SoulConfigurator socket={socket} />
          
          <SkillBrowser />

          <div className="cron-panel glass-panel">
            <div className="panel-header" style={{ borderBottom: '1px solid var(--border-color)', fontSize: '0.9rem' }}>
              <Clock size={18} color="var(--accent)" />
              Scheduled Sequences
              <button onClick={refreshJobs} className="cancel-btn" style={{ marginLeft: 'auto' }}>↻</button>
            </div>

            <div className="cron-list">
              <AnimatePresence>
                {cronJobs.length === 0 ? (
                  <div style={{ padding: '20px', textAlign: 'center', color: 'var(--text-muted)', fontSize: '0.8rem' }}>No active cron sequences.</div>
                ) : (
                  cronJobs.map(job => (
                    <motion.div 
                      key={job.id} 
                      initial={{ x: 20, opacity: 0 }}
                      animate={{ x: 0, opacity: 1 }}
                      exit={{ x: -20, opacity: 0 }}
                      className="cron-job-card"
                    >
                      <div className="cron-header">
                        <span className={`cron-type ${job.schedule_type}`}>{job.schedule_type}</span>
                        <button onClick={() => cancelJob(job.id)} className="cancel-btn"><Trash2 size={16} /></button>
                      </div>
                      <div className="cron-expr">⏱️ {job.schedule_type === 'delay' ? `${job.expr}s` : job.expr}</div>
                      <div className="cron-message">"{job.message}"</div>
                    </motion.div>
                  ))
                )}
              </AnimatePresence>
            </div>
          </div>

          <div style={{ padding: '0 20px 20px 20px' }}>
            <CanvasRenderer primitives={canvasPrimitives} />
          </div>
        </div>
      </div>
    </div>
  );
}

export default App;
