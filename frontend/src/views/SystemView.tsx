import React from 'react';
import { Server, Shield, Wifi, HardDrive, Terminal } from 'lucide-react';

interface SystemViewProps {
  insights: string[];
}

const SystemView: React.FC<SystemViewProps> = ({ insights }) => {
  return (
    <div className="view-container system-view">
      <header className="view-header">
        <h1>System & Logs</h1>
        <p className="subtitle">Gateway status, infrastructure health and operational logs</p>
      </header>

      <div className="system-grid">
        <div className="system-left">
          <div className="status-overview glass-panel">
            <h3>Gateway Status</h3>
            <div className="status-rows">
              <div className="status-row">
                <span className="label"><Wifi size={14} /> Connectivity</span>
                <span className="value status-online">SECURE (Tailscale)</span>
              </div>
              <div className="status-row">
                <span className="label"><Server size={14} /> Backend Version</span>
                <span className="value">Pharmakon Core v0.8.2-alpha</span>
              </div>
              <div className="status-row">
                <span className="label"><HardDrive size={14} /> Storage Usage</span>
                <span className="value">2.4 GB / 10 GB (Trajectory Data)</span>
              </div>
              <div className="status-row">
                <span className="label"><Shield size={14} /> Sandbox Mode</span>
                <span className="value">DOCKER_ENABLED</span>
              </div>
            </div>
          </div>

          <div className="health-check glass-panel">
            <h3>Health Diagnostics</h3>
            <div className="health-items">
              <div className="health-item good">
                <div className="dot" />
                <span>Memory Weaver: ACTIVE</span>
              </div>
              <div className="health-item good">
                <div className="dot" />
                <span>Semantic Search: CONNECTED</span>
              </div>
              <div className="health-item warning">
                <div className="dot" />
                <span>Docker Daemon: LAGGING (50ms)</span>
              </div>
            </div>
          </div>
        </div>

        <div className="system-right">
          <div className="log-viewer glass-panel">
            <div className="log-header">
              <div className="title"><Terminal size={16} /> GATEWAY_LOGS</div>
              <div className="actions">
                <button className="small-text-btn">CLEAR</button>
                <button className="small-text-btn">DOWNLOAD</button>
              </div>
            </div>
            <div className="log-body">
              {insights.map((insight, i) => {
                const isForensic = insight.startsWith('[FORENSIC]');
                return (
                  <div key={i} className={`log-entry ${isForensic ? 'forensic' : 'insight'}`}>
                    <span className="timestamp">{new Date().toLocaleTimeString()}</span>
                    <span className="tag">{isForensic ? '[TRUST]' : '[INSIGHT]'}</span>
                    <span className="message">{isForensic ? insight.replace('[FORENSIC] ', '') : insight}</span>
                  </div>
                );
              })}
              <div className="log-entry info">
                <span className="timestamp">14:21:05</span>
                <span className="tag">[INFO]</span>
                <span className="message">WebSocket connection established from 127.0.0.1</span>
              </div>
              <div className="log-entry warn">
                <span className="timestamp">14:21:08</span>
                <span className="tag">[WARN]</span>
                <span className="message">Trajectory buffer reaching 80% capacity. Auto-synthesizing...</span>
              </div>
              <div className="log-entry error">
                <span className="timestamp">14:21:12</span>
                <span className="tag">[ERROR]</span>
                <span className="message">Brave Search API: Rate limit exceeded for key_****</span>
              </div>
              <div className="log-entry info">
                <span className="timestamp">14:22:01</span>
                <span className="tag">[INFO]</span>
                <span className="message">Soul updated to 'RESEARCHER_EXPERT'</span>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};

export default SystemView;
