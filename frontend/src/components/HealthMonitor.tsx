import React from 'react';
import { Activity, ShieldAlert } from 'lucide-react';
import { motion } from 'framer-motion';

interface HealthStats {
  failure_rate: number;
  last_latency: string;
  is_healthy: boolean;
}

const HealthMonitor: React.FC<{ stats: HealthStats | null }> = ({ stats }) => {
  if (!stats) return null;

  return (
    <div className="health-monitor glass-card" style={{ margin: '0 20px 20px 20px', padding: '16px' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: '8px', marginBottom: '12px', fontSize: '0.9rem', fontWeight: 600 }}>
        <Activity size={16} color={stats.is_healthy ? 'var(--success)' : 'var(--danger)'} />
        Agent Vitality
      </div>
      
      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '12px' }}>
        <div className="stat-item">
          <div style={{ fontSize: '0.7rem', color: 'var(--text-muted)', marginBottom: '4px' }}>FAILURE RATE</div>
          <div style={{ fontSize: '1.1rem', fontWeight: 700, color: stats.failure_rate > 30 ? 'var(--danger)' : 'var(--text-primary)' }}>
            {stats.failure_rate.toFixed(1)}%
          </div>
        </div>
        <div className="stat-item">
          <div style={{ fontSize: '0.7rem', color: 'var(--text-muted)', marginBottom: '4px' }}>LATENCY</div>
          <div style={{ fontSize: '1.1rem', fontWeight: 700 }}>
            {stats.last_latency}
          </div>
        </div>
      </div>
      
      {!stats.is_healthy && (
        <motion.div 
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          style={{ marginTop: '12px', color: 'var(--danger)', fontSize: '0.75rem', display: 'flex', alignItems: 'center', gap: '4px' }}
        >
          <ShieldAlert size={14} />
          High failure rate detected. Rescue protocol active.
        </motion.div>
      )}
    </div>
  );
};

export default HealthMonitor;
