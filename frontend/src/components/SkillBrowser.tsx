import React, { useState } from 'react';
import { Search, Box, ChevronRight, Cpu } from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';

interface Skill {
  name: string;
  description: string;
}

const SkillBrowser: React.FC = () => {
  const [isOpen, setIsOpen] = useState(false);
  const [search, setSearch] = useState('');

  // Mock skills based on the 53 ported ones
  const skills: Skill[] = [
    { name: '1password', description: 'Retrieve secrets and items from 1Password.' },
    { name: 'apple-notes', description: 'Read and create macOS Obsidian notes.' },
    { name: 'github', description: 'Interact with GitHub repositories and issues.' },
    { name: 'obsidian', description: 'Manage notes in local Obsidian vaults.' },
    { name: 'weather', description: 'Real-time weather data and forecasts.' },
    { name: 'trello', description: 'Manage boards, lists, and cards.' },
    { name: 'slack', description: 'Send and receive Slack messages.' },
    { name: 'tmux', description: 'Control terminal multiplexer sessions.' },
  ].sort((a, b) => a.name.localeCompare(b.name));

  const filtered = skills.filter(s =>
    s.name.toLowerCase().includes(search.toLowerCase()) ||
    s.description.toLowerCase().includes(search.toLowerCase())
  );

  return (
    <div className="skill-browser-container" style={{ margin: '0 20px' }}>
      <button
        className="glass-card skill-toggle"
        onClick={() => setIsOpen(!isOpen)}
        style={{
          width: '100%',
          padding: '12px 16px',
          display: 'flex',
          alignItems: 'center',
          gap: '10px',
          color: 'var(--text-secondary)',
          background: isOpen ? 'var(--accent-muted)' : 'var(--bg-card)'
        }}
      >
        <Box size={18} color="var(--accent)" />
        <span style={{ flex: 1, textAlign: 'left', fontWeight: 600 }}>Skill Registry</span>
        <motion.div animate={{ rotate: isOpen ? 90 : 0 }}>
          <ChevronRight size={16} />
        </motion.div>
      </button>

      <AnimatePresence>
        {isOpen && (
          <motion.div
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: 'auto', opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            className="skill-list-dropdown"
            style={{ overflow: 'hidden' }}
          >
            <div className="search-bar-mini" style={{ margin: '12px 0', position: 'relative' }}>
              <Search size={14} style={{ position: 'absolute', left: '10px', top: '50%', transform: 'translateY(-50%)', opacity: 0.5 }} />
              <input
                type="text"
                placeholder="Search 53 skills..."
                value={search}
                onChange={e => setSearch(e.target.value)}
                style={{
                  width: '100%',
                  padding: '8px 8px 8px 30px',
                  background: 'rgba(255,255,255,0.05)',
                  border: '1px solid var(--border-color)',
                  borderRadius: '8px',
                  color: '#fff',
                  fontSize: '0.8rem'
                }}
              />
            </div>

            <div className="skills-scroll" style={{ maxHeight: '300px', overflowY: 'auto', paddingRight: '4px' }}>
              {filtered.map(skill => (
                <div key={skill.name} className="skill-item-mini" style={{ padding: '8px', borderBottom: '1px solid var(--border-color)', fontSize: '0.8rem' }}>
                  <div style={{ fontWeight: 700, color: 'var(--accent)', marginBottom: '2px', display: 'flex', alignItems: 'center', gap: '4px' }}>
                    <Cpu size={12} />
                    {skill.name}
                  </div>
                  <div style={{ color: 'var(--text-muted)', fontSize: '0.75rem' }}>{skill.description}</div>
                </div>
              ))}
              {filtered.length === 0 && <div style={{ padding: '20px', textAlign: 'center', color: 'var(--text-muted)', fontSize: '0.8rem' }}>No skills matching query.</div>}
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
};

export default SkillBrowser;
