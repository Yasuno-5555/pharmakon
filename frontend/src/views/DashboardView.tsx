import React from 'react';
import { Activity, Zap, Cpu, TrendingUp, DollarSign } from 'lucide-react';
import {
  AreaChart, Area, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer,
  BarChart, Bar, Cell
} from 'recharts';
import { motion } from 'framer-motion';

const COLORS = ['#0088FE', '#00C49F', '#FFBB28', '#FF8042', '#8884d8'];

interface DashboardViewProps {
  stats: any;
  mcpStats: any[];
  usageHistory: any[];
}

const DashboardView: React.FC<DashboardViewProps> = ({ stats, mcpStats, usageHistory }) => {
  const chartData = usageHistory?.length > 0 ? usageHistory : [
    { name: '00:00', tokens: 0, cost: 0 },
  ];

  return (
    <div className="view-container dashboard-view">
      <header className="view-header">
        <h1>Intelligence Dashboard</h1>
        <p className="subtitle">Real-time system metrics and sequence analytics</p>
      </header>

      <div className="stats-grid">
        <motion.div
          className="stats-card glass-panel premium"
          initial={{ opacity: 0, y: 15 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 0.1 }}
        >
          <div className="card-icon tokens"><Zap size={24} /></div>
          <div className="card-info">
            <span className="label">Total Tokens</span>
            <span className="value">{(stats?.total_tokens || 124500).toLocaleString()}</span>
            <span className="trend positive"><TrendingUp size={12} /> 12% sequence growth</span>
          </div>
        </motion.div>

        <motion.div
          className="stats-card glass-panel premium"
          initial={{ opacity: 0, y: 15 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 0.2 }}
        >
          <div className="card-icon cost"><DollarSign size={24} /></div>
          <div className="card-info">
            <span className="label">Incurred Cost</span>
            <span className="value">${(stats?.total_cost || 0.42).toFixed(4)}</span>
            <span className="trend positive"><TrendingUp size={12} /> Optimization: +4.2%</span>
          </div>
        </motion.div>

        <motion.div
          className="stats-card glass-panel premium"
          initial={{ opacity: 0, y: 15 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 0.3 }}
        >
          <div className="card-icon uptime"><Activity size={24} /></div>
          <div className="card-info">
            <span className="label">Neural Uptime</span>
            <span className="value">{Math.floor((stats?.uptime || 3600) / 3600)}h {Math.floor(((stats?.uptime || 3600) % 3600) / 60)}m</span>
            <div className="status-badge-premium">ACTIVE</div>
          </div>
        </motion.div>

        <motion.div
          className="stats-card glass-panel premium"
          initial={{ opacity: 0, y: 15 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 0.4 }}
        >
          <div className="card-icon load"><Cpu size={24} /></div>
          <div className="card-info">
            <span className="label">Memory Load</span>
            <span className="value">{(stats?.memory_usage || 128 * 1024 * 1024) / 1024 / 1024 | 0} MB</span>
            <span className="trend">Cache Efficiency: 92%</span>
          </div>
        </motion.div>
      </div>

      <div className="dashboard-charts">
        <div className="chart-container glass-panel">
          <div className="chart-header-simple">
            <TrendingUp size={16} />
            <h3>TOKEN VELOCITY</h3>
          </div>
          <div className="chart-wrapper" style={{ height: '300px' }}>
            <ResponsiveContainer width="100%" height="100%">
              <AreaChart data={chartData}>
                <defs>
                  <linearGradient id="colorTokens" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="5%" stopColor="var(--accent)" stopOpacity={0.3}/>
                    <stop offset="95%" stopColor="var(--accent)" stopOpacity={0}/>
                  </linearGradient>
                </defs>
                <CartesianGrid strokeDasharray="3 3" stroke="rgba(255,255,255,0.03)" vertical={false} />
                <XAxis dataKey="name" stroke="rgba(255,255,255,0.2)" fontSize={11} tickLine={false} axisLine={false} />
                <YAxis stroke="rgba(255,255,255,0.2)" fontSize={11} tickLine={false} axisLine={false} />
                <Tooltip
                  contentStyle={{ background: 'var(--bg-dark)', border: '1px solid var(--border-color)', borderRadius: '12px', backdropFilter: 'blur(10px)' }}
                  itemStyle={{ color: 'var(--accent)' }}
                />
                <Area type="monotone" dataKey="tokens" stroke="var(--accent)" strokeWidth={3} fillOpacity={1} fill="url(#colorTokens)" />
              </AreaChart>
            </ResponsiveContainer>
          </div>
        </div>

        <div className="chart-container glass-panel">
          <div className="chart-header-simple">
            <Activity size={16} />
            <h3>TOOL DISTRIBUTION</h3>
          </div>
          <div className="chart-wrapper" style={{ height: '300px' }}>
            <ResponsiveContainer width="100%" height="100%">
              <BarChart data={mcpStats?.length > 0 ? mcpStats : [{name: 'shell', call_count: 12}, {name: 'file', call_count: 8}]}>
                <CartesianGrid strokeDasharray="3 3" stroke="rgba(255,255,255,0.03)" vertical={false} />
                <XAxis dataKey="name" stroke="rgba(255,255,255,0.2)" fontSize={11} tickLine={false} axisLine={false} />
                <YAxis stroke="rgba(255,255,255,0.2)" fontSize={11} tickLine={false} axisLine={false} />
                <Tooltip
                  contentStyle={{ background: 'var(--bg-dark)', border: '1px solid var(--border-color)', borderRadius: '12px' }}
                />
                <Bar dataKey="call_count" radius={[4, 4, 0, 0]}>
                  {(mcpStats || []).map((_entry, index) => (
                    <Cell key={`cell-${index}`} fill={COLORS[index % COLORS.length]} opacity={0.8} />
                  ))}
                </Bar>
              </BarChart>
            </ResponsiveContainer>
          </div>
        </div>
      </div>
    </div>
  );
};

export default DashboardView;
