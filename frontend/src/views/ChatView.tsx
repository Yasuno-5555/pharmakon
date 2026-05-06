import React from 'react';
import { Send, Trash2, Bot, Zap } from 'lucide-react';
import { AnimatePresence } from 'framer-motion';
import MessageItem from '../components/MessageItem';

interface ChatViewProps {
  messages: any[];
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
}

const ChatView: React.FC<ChatViewProps> = ({
  messages, input, sendMessage, handleKeyDown, handleInput,
  connected, clearMessages, currentModel, availableModels, switchModel,
  textareaRef, messagesEndRef
}) => {
  return (
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
          <button onClick={clearMessages}><Trash2 size={18} /></button>
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
              <MessageItem key={msg.id} msg={msg} socket={null as any} />
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
  );
};

export default ChatView;
