import React from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { User, Bot, Terminal, Sparkles, BookOpen } from 'lucide-react';

import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { Prism as SyntaxHighlighter } from 'react-syntax-highlighter';
import { vscDarkPlus } from 'react-syntax-highlighter/dist/esm/styles/prism';

interface MessageItemProps {
  msg: {
    id: string;
    role: string;
    content?: string;
    thought?: string;
    images?: string[];
    toolCall?: { name: string; args: any };
    toolResult?: { result: string };
    interactive?: { id: string; components: any[] };
    context_used?: string[];
  };
  socket: WebSocket | null;
}

interface ToolFamilyInfo {
  glyph: string;
  label: string;
  color: string;
  bgColor: string;
  borderColor: string;
}

const getToolFamilyInfo = (name: string): ToolFamilyInfo => {
  const norm = name.toLowerCase();
  if (norm.includes('list_dir') || norm.includes('view_file') || norm.includes('read')) {
    return {
      glyph: '▷',
      label: 'READ',
      color: 'rgb(59, 130, 246)',
      bgColor: 'rgba(59, 130, 246, 0.04)',
      borderColor: 'rgba(59, 130, 246, 0.12)',
    };
  }
  if (
    norm.includes('modify') ||
    norm.includes('replace') ||
    norm.includes('write_to_file') ||
    norm.includes('edit') ||
    norm.includes('patch')
  ) {
    return {
      glyph: '◆',
      label: 'PATCH',
      color: 'rgb(16, 185, 129)',
      bgColor: 'rgba(16, 185, 129, 0.04)',
      borderColor: 'rgba(16, 185, 129, 0.12)',
    };
  }
  if (norm.includes('shell') || norm.includes('run') || norm.includes('codeact')) {
    return {
      glyph: '▶',
      label: 'RUN',
      color: 'rgb(168, 85, 247)',
      bgColor: 'rgba(168, 85, 247, 0.04)',
      borderColor: 'rgba(168, 85, 247, 0.12)',
    };
  }
  if (norm.includes('grep') || norm.includes('search') || norm.includes('find')) {
    return {
      glyph: '⌕',
      label: 'FIND',
      color: 'rgb(6, 182, 212)',
      bgColor: 'rgba(6, 182, 212, 0.04)',
      borderColor: 'rgba(6, 182, 212, 0.12)',
    };
  }
  return {
    glyph: '•',
    label: 'TOOL',
    color: 'rgb(156, 163, 175)',
    bgColor: 'rgba(156, 163, 175, 0.04)',
    borderColor: 'rgba(156, 163, 175, 0.12)',
  };
};

const MessageItem: React.FC<MessageItemProps> = ({ msg, socket }) => {
  return (
    <motion.div
      initial={{ opacity: 0, y: 15, scale: 0.98 }}
      animate={{ opacity: 1, y: 0, scale: 1 }}
      transition={{ duration: 0.4, ease: [0.23, 1, 0.32, 1] }}
      className={`message ${msg.role}`}
    >
      <div className={`message-avatar ${msg.role}`}>
        {msg.role === 'user' ? <User size={18} /> : <Bot size={18} />}
      </div>

      <div className="message-content">
        <AnimatePresence>
          {msg.thought && (
            <motion.div
              initial={{ height: 0, opacity: 0 }}
              animate={{ height: 'auto', opacity: 1 }}
              exit={{ height: 0, opacity: 0 }}
              className="thought-container"
            >
              <div className="thought-header">
                <Sparkles size={12} />
                <span>PHARMAKON COGNITION</span>
              </div>
              <div className="thought-body">
                {msg.thought}
              </div>
            </motion.div>
          )}
        </AnimatePresence>

        {msg.context_used && msg.context_used.length > 0 && (
          <div className="context-pill">
            <BookOpen size={12} />
            <span>Retrieved {msg.context_used.length} semantic fragments</span>
          </div>
        )}

        {msg.toolCall && (() => {
          const info = getToolFamilyInfo(msg.toolCall.name);
          return (
            <motion.div 
              initial={{ opacity: 0, scale: 0.98 }}
              animate={{ opacity: 1, scale: 1 }}
              className="tool-block tool-call"
              style={{
                background: info.bgColor,
                borderColor: info.borderColor,
                borderLeft: `4px solid ${info.color}`,
                boxShadow: `0 4px 20px -4px rgba(0, 0, 0, 0.3)`
              }}
            >
              <div className="tool-header" style={{ borderColor: info.borderColor, background: 'rgba(255,255,255,0.01)' }}>
                <Terminal size={14} style={{ color: info.color }} />
                <span className="tool-label" style={{ color: info.color, letterSpacing: '0.05em', fontWeight: 800 }}>
                  {info.glyph} {info.label}
                </span>
              </div>
              <div className="tool-body">
                <span className="tool-name" style={{ color: info.color }}>{msg.toolCall.name}</span>
                <pre className="tool-args">{JSON.stringify(msg.toolCall.args, null, 2)}</pre>
              </div>
            </motion.div>
          );
        })()}

        {msg.toolResult && (() => {
          const resultStr = msg.toolResult.result || '';
          const isError = resultStr.toLowerCase().includes('error') || resultStr.toLowerCase().includes('failed');
          const themeColor = isError ? 'rgb(239, 68, 68)' : 'rgb(59, 130, 246)';
          const themeBg = isError ? 'rgba(239, 68, 68, 0.04)' : 'rgba(59, 130, 246, 0.04)';
          const themeBorder = isError ? 'rgba(239, 68, 68, 0.12)' : 'rgba(59, 130, 246, 0.12)';
          const label = isError ? 'SYSTEM ERROR' : 'SYSTEM OUTPUT';
          const glyph = isError ? '❌' : '✓';
          
          return (
            <motion.div
              initial={{ opacity: 0, scale: 0.98 }}
              animate={{ opacity: 1, scale: 1 }}
              className="tool-block tool-result"
              style={{
                background: themeBg,
                borderColor: themeBorder,
                borderLeft: `4px solid ${themeColor}`,
                boxShadow: `0 4px 20px -4px rgba(0, 0, 0, 0.3)`
              }}
            >
              <div className="tool-header" style={{ borderColor: themeBorder, background: 'rgba(255,255,255,0.01)' }}>
                <Terminal size={14} style={{ color: themeColor }} />
                <span className="tool-label" style={{ color: themeColor, letterSpacing: '0.05em', fontWeight: 800 }}>
                  {glyph} {label}
                </span>
              </div>
              <pre className="tool-result-body" style={{ padding: '12px' }}>
                {resultStr}
              </pre>
            </motion.div>
          );
        })()}

        {(msg.content || (msg.images && msg.images.length > 0)) && (
          <div className="message-bubble markdown-body">
            {msg.content && (
              <ReactMarkdown
                remarkPlugins={[remarkGfm]}
                components={{
                  code({node, inline, className, children, ...props}: any) {
                    const match = /language-(\w+)/.exec(className || '')
                    return !inline && match ? (
                      <div className="code-block-container">
                        <div className="code-header">
                          <span>{match[1].toUpperCase()}</span>
                        </div>
                        <SyntaxHighlighter
                          {...props}
                          children={String(children).replace(/\n$/, '')}
                          style={vscDarkPlus}
                          language={match[1]}
                          PreTag="div"
                          customStyle={{
                            margin: 0,
                            padding: '16px',
                            background: 'rgba(0,0,0,0.3)',
                            fontSize: '0.85rem'
                          }}
                        />
                      </div>
                    ) : (
                      <code {...props} className="inline-code">
                        {children}
                      </code>
                    )
                  }
                }}
              >
                {msg.content}
              </ReactMarkdown>
            )}
            {msg.images && msg.images.length > 0 && (
              <div className="message-images">
                {msg.images.map((img, i) => (
                  <motion.img
                    key={i}
                    src={img}
                    whileHover={{ scale: 1.02 }}
                    className="chat-image"
                    alt="Multimodal content"
                  />
                ))}
              </div>
            )}
          </div>
        )}

        {msg.interactive && (
          <div className="interactive-container">
            {msg.interactive.components.map((comp: any, i: number) => {
              if (comp.type === 'Button') {
                return (
                  <button
                    key={i}
                    className={`premium-btn-${comp.payload.style || 'primary'}`}
                    onClick={() => socket?.send(JSON.stringify({ type: 'InteractiveResponse', payload: { element_id: comp.payload.id, action: 'click' } }))}
                  >
                    {comp.payload.label}
                  </button>
                )
              }
              // ...
              return null;
            })}
          </div>
        )}
      </div>
    </motion.div>
  );
};

export default MessageItem;
