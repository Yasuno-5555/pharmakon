import React, { useState } from 'react';
import { Save, Shield, Bot, Key } from 'lucide-react';

interface SettingsViewProps {
  settings: any;
  onUpdate: (settings: any) => void;
}

const SettingsView: React.FC<SettingsViewProps> = ({ settings, onUpdate }) => {
  const [localSettings, setLocalSettings] = useState(settings || {
    model: 'gemini-2.0-flash',
    temperature: 0.7,
    auto_approval: false,
    max_tokens: 100000,
    api_key: '************'
  });

  const handleSave = () => {
    onUpdate(localSettings);
  };

  return (
    <div className="view-container settings-view">
      <header className="view-header">
        <h1>Settings</h1>
        <p className="subtitle">Configure Pharmakon core parameters and AI behaviors</p>
      </header>

      <div className="settings-grid">
        <section className="settings-section glass-panel">
          <h3><Bot size={18} /> Model Configuration</h3>
          <div className="setting-item">
            <label>Primary Model</label>
            <select
              value={localSettings.model}
              onChange={(e) => setLocalSettings({...localSettings, model: e.target.value})}
            >
              <option value="gemini-2.0-flash">Gemini 2.0 Flash</option>
              <option value="gemini-1.5-pro">Gemini 1.5 Pro</option>
              <option value="claude-3-5-sonnet">Claude 3.5 Sonnet</option>
              <option value="gpt-4o">GPT-4o</option>
            </select>
          </div>
          <div className="setting-item">
            <label>Temperature ({localSettings.temperature})</label>
            <input
              type="range" min="0" max="1" step="0.1"
              value={localSettings.temperature}
              onChange={(e) => setLocalSettings({...localSettings, temperature: parseFloat(e.target.value)})}
            />
          </div>
        </section>

        <section className="settings-section glass-panel">
          <h3><Shield size={18} /> Safety & Governance</h3>
          <div className="setting-item toggle">
            <label>Auto-Approval Mode</label>
            <input
              type="checkbox"
              checked={localSettings.auto_approval}
              onChange={(e) => setLocalSettings({...localSettings, auto_approval: e.target.checked})}
            />
          </div>
          <div className="setting-item">
            <label>Token Budget Limit</label>
            <input
              type="number"
              value={localSettings.max_tokens}
              onChange={(e) => setLocalSettings({...localSettings, max_tokens: parseInt(e.target.value)})}
            />
          </div>
        </section>

        <section className="settings-section glass-panel">
          <h3><Key size={18} /> Secrets & Auth</h3>
          <div className="setting-item">
            <label>API Key</label>
            <input
              type="password"
              value={localSettings.api_key}
              onChange={(e) => setLocalSettings({...localSettings, api_key: e.target.value})}
            />
          </div>
        </section>
      </div>

      <footer className="view-footer">
        <button className="primary-btn" onClick={handleSave}>
          <Save size={18} /> Save Configuration
        </button>
      </footer>
    </div>
  );
};

export default SettingsView;
