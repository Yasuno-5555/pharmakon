import React from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { User, Bot, Terminal } from 'lucide-react';

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

const MessageItem: React.FC<MessageItemProps> = ({ msg, socket }) => {
  return (
    <motion.div
      initial={{ opacity: 0, y: 20, scale: 0.95 }}
      animate={{ opacity: 1, y: 0, scale: 1 }}
      transition={{ duration: 0.3, ease: [0.4, 0, 0.2, 1] }}
      className={`message ${msg.role}`}
    >
      <div className="message-avatar">
        {msg.role === 'user' ? <User size={20} /> : <Bot size={20} />}
      </div>

      <div className="message-content">
        <AnimatePresence>
          {msg.thought && (
            <motion.div
              initial={{ height: 0, opacity: 0 }}
              animate={{ height: 'auto', opacity: 1 }}
              exit={{ height: 0, opacity: 0 }}
              className="thought-bubble"
            >
              {msg.thought.split('\n').map((line, i) => (
                <span key={i}>{line}<br/></span>
              ))}
            </motion.div>
          )}
        </AnimatePresence>

        {msg.context_used && msg.context_used.length > 0 && (
          <div className="context-pill">
            <span>🧠 Recalled {msg.context_used.length} memory fragment(s)</span>
          </div>
        )}

        {msg.toolCall && (
          <div className="tool-call">
            <Terminal size={14} />
            <span className="tool-label">CALL:</span>
            <span className="tool-name">{msg.toolCall.name}</span>
            <span className="tool-args">{JSON.stringify(msg.toolCall.args)}</span>
          </div>
        )}

        {msg.toolResult && (
          <div className="tool-result">
            <div className="tool-result-header">
              <Terminal size={12} />
              <span>OUTPUT</span>
            </div>
            <pre className="tool-result-body">
              {msg.toolResult.result}
            </pre>
          </div>
        )}

        {(msg.content || (msg.images && msg.images.length > 0)) && (
          <div className="message-bubble markdown-body">
            {msg.content && (
              <ReactMarkdown
                remarkPlugins={[remarkGfm]}
                components={{
                  code({node, inline, className, children, ...props}: any) {
                    const match = /language-(\w+)/.exec(className || '')
                    return !inline && match ? (
                      <SyntaxHighlighter
                        {...props}
                        children={String(children).replace(/\n$/, '')}
                        style={vscDarkPlus}
                        language={match[1]}
                        PreTag="div"
                      />
                    ) : (
                      <code {...props} className={className}>
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
                    whileHover={{ scale: 1.05 }}
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
                    className={`btn-${comp.payload.style}`}
                    onClick={() => socket?.send(JSON.stringify({ type: 'InteractiveResponse', payload: { element_id: comp.payload.id, action: 'click' } }))}
                  >
                    {comp.payload.label}
                  </button>
                )
              }
              if (comp.type === 'Poll') {
                return (
                  <div key={i} className="poll-component glass-card">
                    <h3>{comp.payload.question}</h3>
                    {comp.payload.options.map((opt: string, j: number) => (
                      <button key={j} onClick={() => socket?.send(JSON.stringify({ type: 'InteractiveResponse', payload: { element_id: comp.payload.id, action: 'vote', value: opt } }))}>{opt}</button>
                    ))}
                  </div>
                )
              }
              // ... Form implementation ...
              return null;
            })}
          </div>
        )}
      </div>
    </motion.div>
  );
};

export default MessageItem;
