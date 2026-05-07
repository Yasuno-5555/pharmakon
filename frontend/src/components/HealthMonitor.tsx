import React from 'react';
import { Activity, ShieldAlert } from 'lucide-react';
import { motion } from 'framer-motion';

import type { HealthStats } from '../App';

const HealthMonitor: React.FC<{ stats: HealthStats | null }> = ({ stats }) => {
  if (!stats) return null;

  return (
    <div className="health-monitor glass-panel">
      <div className="health-monitor-header">
        <Activity size={16} className={stats.is_healthy ? 'status-icon healthy' : 'status-icon critical'} />
        <span>AGENT VITALITY</span>
      </div>

      <div className="health-stats-grid">
        <div className="health-stat-item">
          <div className="stat-label">FAILURE RATE</div>
          <div className={`stat-value ${(stats.failure_rate || 0) > 20 ? 'danger' : ''}`}>
            {(stats.failure_rate || 0).toFixed(1)}%
          </div>
        </div>
        <div className="health-stat-item">
          <div className="stat-label">LATENCY</div>
          <div className="stat-value">
            {stats.last_latency || '0ms'}
          </div>
        </div>
      </div>

      {!stats.is_healthy && (
        <motion.div
          initial={{ opacity: 0, scale: 0.9 }}
          animate={{ opacity: 1, scale: 1 }}
          className="health-alert"
        >
          <ShieldAlert size={14} />
          <span>Anomalous failure rate detected.</span>
        </motion.div>
      )}
    </div>
  );
};

export default HealthMonitor;
