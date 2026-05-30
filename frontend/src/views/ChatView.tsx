import React, { useEffect } from 'react';
import { Send, Trash2, Bot } from 'lucide-react';
import { AnimatePresence, motion } from 'framer-motion';
import MessageItem from '../components/MessageItem';
import type { Message } from '../types';

interface ChatViewProps {
  messages: Message[];
  input: string;
  setInput: (val: string) => void;
  sendMessage: (e?: React.FormEvent) => void;
  handleKeyDown: (e: React.KeyboardEvent<HTMLTextAreaElement>) => void;
  handleInput: (e: React.ChangeEvent<HTMLTextAreaElement>) => void;
  connected: boolean;
  clearMessages: () => void;
  currentModel: string;
  availableModels: string[];
  switchModel: (id: string) => void;
  textareaRef: React.RefObject<HTMLTextAreaElement | null>;
  messagesEndRef: React.RefObject<HTMLDivElement | null>;
  onInteractiveResponse: (elementId: string, action: string) => void;
}

const ChatView: React.FC<ChatViewProps> = ({
  messages, input, setInput, sendMessage, handleKeyDown, handleInput,
  connected, clearMessages, currentModel, availableModels, switchModel,
  textareaRef, messagesEndRef, onInteractiveResponse,
}) => {
  // Auto-expand textarea
  useEffect(() => {
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto';
      textareaRef.current.style.height = `${Math.min(textareaRef.current.scrollHeight, 200)}px`;
    }
  }, [input, textareaRef]);

  return (
    <div className="chat-view">
      <header className="view-header-chat glass-panel">
        <div className="agent-identity">
          <div className="agent-avatar-large">Φ</div>
          <div className="agent-meta">
            <h1>Pharmakon Supervisor</h1>
            <div className="status-indicator">
              <div className={`status-dot ${connected ? 'online' : 'offline'}`} />
              <span>{connected ? 'Neural Link Active' : 'Offline'}</span>
            </div>
          </div>
        </div>

        <div className="chat-header-actions">
          <div className="premium-select-wrapper">
            <Bot size={14} className="select-icon" />
            <select
              value={currentModel}
              onChange={(e) => switchModel(e.target.value)}
              className="premium-select"
            >
              <option value="" disabled>Select Model</option>
              {availableModels.map(m => (
                <option key={m} value={m}>{m}</option>
              ))}
            </select>
          </div>
          <button className="icon-btn" onClick={clearMessages} title="Clear Session">
            <Trash2 size={18} />
          </button>
        </div>
      </header>

      <div className="chat-messages-container">
        <AnimatePresence initial={false}>
          {messages.length === 0 ? (
            <motion.div
              initial={{ opacity: 0, scale: 0.9 }}
              animate={{ opacity: 1, scale: 1 }}
              className="chat-empty-state"
            >
              <div className="empty-icon">Φ</div>
              <h2>Awaiting Sequence</h2>
              <p>Deploy your first instruction to begin the autonomous cycle.</p>
            </motion.div>
          ) : (
            messages.map(msg => (
              <MessageItem
                key={msg.id}
                msg={msg}
                onInteractiveResponse={onInteractiveResponse}
              />
            ))
          )}
        </AnimatePresence>
        <div ref={messagesEndRef} />
      </div>

      <footer className="chat-input-container">
        <div className="chat-input-wrapper glass-panel">
          <textarea
            ref={textareaRef}
            className="chat-textarea"
            placeholder="Deploy instruction..."
            value={input}
            onChange={(e) => {
              handleInput(e);
              setInput(e.target.value);
            }}
            onKeyDown={handleKeyDown}
            rows={1}
          />
          <motion.button
            whileHover={{ scale: 1.05 }}
            whileTap={{ scale: 0.95 }}
            onClick={sendMessage}
            className={`chat-send-btn ${input.trim() ? 'active' : ''}`}
          >
            <Send size={18} />
          </motion.button>
        </div>
      </footer>
    </div>
  );
};

export default ChatView;
