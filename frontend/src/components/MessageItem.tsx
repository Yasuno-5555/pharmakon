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

        {msg.toolCall && (
          <div className="tool-block tool-call">
            <div className="tool-header">
              <Terminal size={14} />
              <span className="tool-label">EXECUTING TOOL</span>
            </div>
            <div className="tool-body">
              <span className="tool-name">{msg.toolCall.name}</span>
              <pre className="tool-args">{JSON.stringify(msg.toolCall.args, null, 2)}</pre>
            </div>
          </div>
        )}

        {msg.toolResult && (
          <div className="tool-block tool-result">
            <div className="tool-header">
              <Terminal size={14} />
              <span className="tool-label">SYSTEM OUTPUT</span>
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
