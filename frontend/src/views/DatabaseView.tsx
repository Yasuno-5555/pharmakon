import React, { useState } from 'react';
import { Database, Search, Share2, Activity, HardDrive } from 'lucide-react';
import { motion } from 'framer-motion';

interface DatabaseViewProps {
  relations: string[];
  onSearch: (query: string) => void;
}

const DatabaseView: React.FC<DatabaseViewProps> = ({ relations, onSearch }) => {
  const [query, setQuery] = useState('');

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    onSearch(query);
  };

  return (
    <div className="database-view">
      <header className="view-header">
        <div className="title-area">
          <Database size={24} className="text-primary" />
          <h1>Knowledge Nexus Explorer</h1>
        </div>
        <form className="search-bar glass-panel" onSubmit={handleSubmit}>
          <Search size={18} />
          <input
            type="text"
            placeholder="Search structural relationships (e.g. 'weaver.rs', 'smart_search')..."
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          <button type="submit" className="premium-button-sm">EXPLORE</button>
        </form>
      </header>

      <div className="database-grid">
        <section className="stats-row">
          <div className="stat-card glass-panel">
            <Activity size={20} />
            <div className="stat-info">
              <span className="stat-label">GRAPH NODES</span>
              <span className="stat-value">{relations.length * 2}+</span>
            </div>
          </div>
          <div className="stat-card glass-panel">
            <Share2 size={20} />
            <div className="stat-info">
              <span className="stat-label">ACTIVE EDGES</span>
              <span className="stat-value">{relations.length}</span>
            </div>
          </div>
          <div className="stat-card glass-panel">
            <HardDrive size={20} />
            <div className="stat-info">
              <span className="stat-label">STORAGE</span>
              <span className="stat-value">LanceDB + SQLite</span>
            </div>
          </div>
        </section>

        <section className="relations-section glass-panel">
          <div className="section-header">
            <Share2 size={18} />
            <span>STRUCTURAL RELATIONSHIPS</span>
          </div>
          <div className="relations-list">
            {relations.length === 0 ? (
              <div className="empty-relations">
                <p>No relationships found for "{query}". Try a different keyword.</p>
              </div>
            ) : (
              relations.map((rel, i) => (
                <motion.div
                  key={i}
                  className="relation-item"
                  initial={{ opacity: 0, x: -10 }}
                  animate={{ opacity: 1, x: 0 }}
                  transition={{ delay: i * 0.05 }}
                >
                  <div className="relation-content">
                    <span className="relation-text">{rel}</span>
                  </div>
                  <div className="relation-actions">
                    <button className="icon-button" title="Jump to Source"><Search size={14} /></button>
                  </div>
                </motion.div>
              ))
            )}
          </div>
        </section>

        <aside className="nexus-info glass-panel">
          <h3>Knowledge Nexus v4.0</h3>
          <p>
            The Nexus combines semantic embeddings with a structural graph.
            It allows the agent to "see" code relationships like dependencies,
            trait implementations, and call graphs.
          </p>
          <div className="nexus-features">
            <div className="feature-item">
              <span className="feature-dot"></span>
              <span>Decay-aware Vector Search</span>
            </div>
            <div className="feature-item">
              <span className="feature-dot"></span>
              <span>AST-Aware Relationship Mapping</span>
            </div>
            <div className="feature-item">
              <span className="feature-dot"></span>
              <span>Cross-node Context Augmentation</span>
            </div>
          </div>
        </aside>
      </div>
    </div>
  );
};

export default DatabaseView;
