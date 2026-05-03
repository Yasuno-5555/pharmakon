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
    <div className="soul-config glass-card" style={{ margin: '0 20px 20px 20px', padding: '16px' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: '8px', marginBottom: '16px', fontSize: '0.9rem', fontWeight: 600 }}>
        <Sparkles size={16} color="var(--accent)" />
        Identity Matrix
      </div>

      <div style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
        <div className="input-group">
          <label style={{ fontSize: '0.7rem', color: 'var(--text-muted)', display: 'block', marginBottom: '4px' }}>TRAITS (COMMA SEPARATED)</label>
          <div style={{ position: 'relative' }}>
            <UserCircle size={14} style={{ position: 'absolute', left: '10px', top: '10px', opacity: 0.5 }} />
            <input 
              type="text" 
              value={soul.traits.join(', ')}
              onChange={e => setSoul({...soul, traits: e.target.value.split(',').map(t => t.trim())})}
              style={{
                width: '100%',
                padding: '8px 8px 8px 30px',
                background: 'rgba(255,255,255,0.03)',
                border: '1px solid var(--border-color)',
                borderRadius: '8px',
                color: '#fff',
                fontSize: '0.85rem'
              }}
            />
          </div>
        </div>

        <div className="input-group">
          <label style={{ fontSize: '0.7rem', color: 'var(--text-muted)', display: 'block', marginBottom: '4px' }}>SYSTEM DIRECTIVE</label>
          <div style={{ position: 'relative' }}>
            <MessageSquare size={14} style={{ position: 'absolute', left: '10px', top: '10px', opacity: 0.5 }} />
            <textarea 
              value={soul.system_prompt}
              onChange={e => setSoul({...soul, system_prompt: e.target.value})}
              style={{
                width: '100%',
                height: '80px',
                padding: '8px 8px 8px 30px',
                background: 'rgba(255,255,255,0.03)',
                border: '1px solid var(--border-color)',
                borderRadius: '8px',
                color: '#fff',
                fontSize: '0.8rem',
                resize: 'none',
                fontFamily: 'inherit'
              }}
            />
          </div>
        </div>

        <motion.button 
          whileHover={{ scale: 1.02 }}
          whileTap={{ scale: 0.98 }}
          onClick={updateSoul}
          style={{
            width: '100%',
            padding: '10px',
            background: 'var(--accent)',
            borderRadius: '8px',
            color: '#fff',
            fontWeight: 600,
            fontSize: '0.85rem',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            gap: '8px',
            marginTop: '4px'
          }}
        >
          <Save size={16} />
          Sync Neural State
        </motion.button>
      </div>
    </div>
  );
};

export default SoulConfigurator;
