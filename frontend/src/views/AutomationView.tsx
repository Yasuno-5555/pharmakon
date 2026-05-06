import React from 'react';
import { Pause, Trash2, Calendar, RefreshCw } from 'lucide-react';
import { motion } from 'framer-motion';

interface CronJobInfo {
  id: string;
  schedule_type: string;
  expr: string;
  message: string;
}

interface AutomationViewProps {
  cronJobs: CronJobInfo[];
  onCancel: (id: string) => void;
  onRefresh: () => void;
}

const AutomationView: React.FC<AutomationViewProps> = ({ cronJobs, onCancel, onRefresh }) => {
  return (
    <div className="view-container automation-view">
      <header className="view-header">
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', width: '100%' }}>
          <div>
            <h1>Automation</h1>
            <p className="subtitle">Scheduled sequences and recurrent background tasks</p>
          </div>
          <button className="action-btn secondary" onClick={onRefresh}>
            <RefreshCw size={16} /> Refresh
          </button>
        </div>
      </header>

      <div className="cron-grid">
        {cronJobs.length === 0 ? (
          <div className="empty-state glass-panel">
            <Calendar size={48} />
            <p>No active cron sequences found.</p>
            <button className="action-btn primary" style={{ marginTop: '1rem' }}>Create New Sequence</button>
          </div>
        ) : (
          cronJobs.map((job) => (
            <motion.div
              key={job.id}
              className="cron-card glass-panel"
              initial={{ opacity: 0, scale: 0.95 }}
              animate={{ opacity: 1, scale: 1 }}
            >
              <div className="cron-header">
                <div className="id-badge">{job.id}</div>
                <div className="schedule-badge">{job.expr}</div>
              </div>
              <div className="cron-body">
                <p className="instruction">"{job.message}"</p>
              </div>
              <div className="cron-footer">
                <div className="next-run">Next run: ~5 minutes</div>
                <div className="actions">
                  <button className="icon-btn" title="Pause"><Pause size={16} /></button>
                  <button className="icon-btn delete" title="Delete" onClick={() => onCancel(job.id)}><Trash2 size={16} /></button>
                </div>
              </div>
            </motion.div>
          ))
        )}
      </div>
    </div>
  );
};

export default AutomationView;
