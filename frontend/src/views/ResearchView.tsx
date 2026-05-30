import React from 'react';
import { Search, BookOpen, CircleHelp, Link2, Ghost } from 'lucide-react';
import { motion } from 'framer-motion';
import type { ResearchNotebook } from '../types';

interface ResearchViewProps {
  notebook: ResearchNotebook | null;
}

const ResearchView: React.FC<ResearchViewProps> = ({ notebook }) => {
  if (!notebook) {
    return (
      <div className="research-empty-state">
        <Ghost size={64} opacity={0.3} />
        <p>No active research session. Start by asking a deep question.</p>
      </div>
    );
  }

  return (
    <div className="research-container">
      <header className="research-header">
        <div className="goal-banner">
          <Search size={20} />
          <span>CURRENT GOAL: {notebook.current_goal}</span>
        </div>
      </header>

      <div className="research-grid">
        <section className="research-section facts-section">
          <div className="section-header">
            <BookOpen size={18} />
            <span>VERIFIED FACTS</span>
          </div>
          <div className="facts-list">
            {notebook.verified_facts.map((fact, i) => (
              <motion.div
                key={i}
                className="fact-card"
                initial={{ opacity: 0, y: 10 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ delay: i * 0.1 }}
              >
                <p className="fact-content">{fact.content}</p>
                <a href={String(fact.source_url)} target="_blank" rel="noreferrer" className="fact-source">
                  <Link2 size={12} /> {fact.source_url}
                </a>
              </motion.div>
            ))}
          </div>
        </section>

        <section className="research-section questions-section">
          <div className="section-header">
            <CircleHelp size={18} />
            <span>PENDING QUESTIONS</span>
          </div>
          <div className="questions-list">
            {notebook.pending_questions.map((q, i) => (
              <div key={i} className="question-item">
                <span className="bullet">?</span>
                <span className="question-text">{q}</span>
              </div>
            ))}
          </div>
        </section>

        <section className="research-section tree-section">
          <div className="section-header">
            <Link2 size={18} />
            <span>RESEARCH TREE</span>
          </div>
          <div className="tree-viz">
            {Object.entries(notebook.research_tree).map(([query, urls], i) => (
              <div key={i} className="tree-node">
                <div className="query-node">{query}</div>
                <div className="url-children">
                  {urls.map((url, j) => (
                    <div key={j} className="url-node">
                      <span className="depth-indicator" data-depth={notebook.visited_urls[url]}></span>
                      <span className="url-text">{url}</span>
                    </div>
                  ))}
                </div>
              </div>
            ))}
          </div>
        </section>
      </div>
    </div>
  );
};

export default ResearchView;
