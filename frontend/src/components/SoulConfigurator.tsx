import React, { useState } from 'react';
import { Sparkles, Save, UserCircle, MessageSquare } from 'lucide-react';
import { motion } from 'framer-motion';

interface Soul {
  name: string;
  traits: string[];
  system_prompt: string;
}

const SoulConfigurator: React.FC<{ socket: WebSocket | null }> = ({ socket }) => {
  const [soul, setSoul] = useState<Soul>({
    name: 'Pharmakon',
    traits: ['helpful', 'efficient', 'precise'],
    system_prompt: 'You are a powerful agentic AI...'
  });

  const updateSoul = () => {
    if (!socket) return;
    socket.send(JSON.stringify({
      type: 'UpdateSoul',
      payload: {
        traits: soul.traits,
        system_prompt: soul.system_prompt
      }
    }));
  };

  return (
    <div className="soul-config glass-panel">
      <div className="soul-config-header">
        <Sparkles size={16} className="sparkle-icon" />
        <span>IDENTITY MATRIX</span>
      </div>

      <div className="soul-form">
        <div className="input-group">
          <label>TRAITS</label>
          <div className="input-wrapper">
            <UserCircle size={14} className="input-icon" />
            <input
              type="text"
              className="premium-input"
              placeholder="e.g. analytical, creative..."
              value={soul.traits.join(', ')}
              onChange={e => setSoul({...soul, traits: e.target.value.split(',').map(t => t.trim())})}
            />
          </div>
        </div>

        <div className="input-group">
          <label>SYSTEM DIRECTIVE</label>
          <div className="input-wrapper">
            <MessageSquare size={14} className="input-icon top" />
            <textarea
              className="premium-textarea"
              placeholder="Define agent core behavior..."
              value={soul.system_prompt}
              onChange={e => setSoul({...soul, system_prompt: e.target.value})}
            />
          </div>
        </div>

        <motion.button
          whileHover={{ scale: 1.02, backgroundColor: 'var(--accent)' }}
          whileTap={{ scale: 0.98 }}
          onClick={updateSoul}
          className="sync-button"
        >
          <Save size={16} />
          <span>SYNC NEURAL STATE</span>
        </motion.button>
      </div>
    </div>
  );
};

export default SoulConfigurator;
