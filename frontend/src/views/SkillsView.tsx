import React, { useState } from 'react';
import { Search, Package, Code, Globe, Terminal, Star, ExternalLink } from 'lucide-react';
import { motion } from 'framer-motion';
import type { ToolInfo } from '../types';

interface SkillsViewProps {
  tools: ToolInfo[];
}

const SkillsView: React.FC<SkillsViewProps> = ({ tools }) => {
  const [filter, setFilter] = useState('All');
  const [search, setSearch] = useState('');

  const categories = ['All', 'System', 'Network', 'FileSystem', 'Code', 'Media'];

  const filteredTools = (tools || []).map((t) => ({
    ...t,
    category: t.category || 'System', // Default category for display
    parameters: t.parameters?.properties || t.parameters || {},
  })).filter(t =>
    (filter === 'All' || t.category === filter) &&
    (t.name.includes(search) || t.description.toLowerCase().includes(search.toLowerCase()))
  );

  return (
    <div className="view-container skills-view">
      <header className="view-header">
        <h1>Skills & Tools</h1>
        <p className="subtitle">Explore and manage the agent's capabilities and MCP extensions</p>
      </header>

      <div className="filter-bar">
        <div className="search-wrapper">
          <Search size={18} />
          <input
            type="text"
            placeholder="Search skills..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </div>
        <div className="category-tabs">
          {categories.map(c => (
            <button
              key={c}
              className={`tab-btn ${filter === c ? 'active' : ''}`}
              onClick={() => setFilter(c)}
            >
              {c}
            </button>
          ))}
        </div>
      </div>

      <div className="skills-grid">
        {filteredTools.map((tool, i) => (
          <motion.div
            key={tool.name}
            className="skill-card glass-panel"
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ delay: i * 0.05 }}
          >
            <div className="skill-header">
              <div className="skill-icon">
                {tool.category === 'System' && <Terminal size={20} />}
                {tool.category === 'Network' && <Globe size={20} />}
                {tool.category === 'Code' && <Code size={20} />}
                {tool.category === 'FileSystem' && <Package size={20} />}
              </div>
              <div className="skill-meta">
                <span className="skill-name">{tool.name}</span>
                <span className="skill-category">{tool.category}</span>
              </div>
              <button className="favorite-btn"><Star size={16} /></button>
            </div>
            <div className="skill-body">
              <p>{tool.description}</p>
            </div>
            <div className="skill-footer">
              <div className="params-preview">
                {Object.keys(tool.parameters).map(p => (
                  <span key={p} className="param-tag">{p}</span>
                ))}
              </div>
              <button className="icon-btn small"><ExternalLink size={14} /></button>
            </div>
          </motion.div>
        ))}
      </div>
    </div>
  );
};

export default SkillsView;
