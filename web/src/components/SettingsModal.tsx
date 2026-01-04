import { useState, useEffect } from 'react';
import { X, Eye, EyeOff, Check } from 'lucide-react';
import type { AppSettings } from '../types';

interface SettingsModalProps {
  isOpen: boolean;
  onClose: () => void;
  settings: AppSettings;
  onSave: (settings: AppSettings) => void;
}

const PROVIDERS = [
  { id: 'groq', name: 'Groq', baseUrl: 'https://api.groq.com/openai/v1' },
  { id: 'openai', name: 'OpenAI', baseUrl: 'https://api.openai.com/v1' },
  { id: 'anthropic', name: 'Anthropic', baseUrl: 'https://api.anthropic.com' },
];

const MODELS: Record<string, string[]> = {
  groq: ['llama-3.3-70b-versatile', 'llama-3.1-8b-instant', 'mixtral-8x7b-32768'],
  openai: ['gpt-4o', 'gpt-4o-mini', 'gpt-4-turbo'],
  anthropic: ['claude-3-5-sonnet-20241022', 'claude-3-opus-20240229'],
};

export function SettingsModal({ isOpen, onClose, settings, onSave }: SettingsModalProps) {
  const [apiKey, setApiKey] = useState(settings.apiKey);
  const [providerId, setProviderId] = useState(settings.providerId);
  const [modelId, setModelId] = useState(settings.modelId);
  const [showKey, setShowKey] = useState(false);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    setApiKey(settings.apiKey);
    setProviderId(settings.providerId);
    setModelId(settings.modelId);
  }, [settings]);

  useEffect(() => {
    const models = MODELS[providerId] || [];
    if (models.length > 0 && !models.includes(modelId)) {
      setModelId(models[0]);
    }
  }, [providerId, modelId]);

  if (!isOpen) return null;

  const handleSave = () => {
    onSave({ apiKey, providerId, modelId });
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') onClose();
  };

  const models = MODELS[providerId] || [];

  return (
    <div className="modal-overlay" onClick={onClose} onKeyDown={handleKeyDown}>
      <div className="modal-content" onClick={e => e.stopPropagation()}>
        <div className="modal-header">
          <h2>Settings</h2>
          <button className="modal-close" onClick={onClose}>
            <X size={18} />
          </button>
        </div>

        <div className="modal-body">
          <div className="setting-group">
            <label className="setting-label">Provider</label>
            <select
              className="setting-select"
              value={providerId}
              onChange={e => setProviderId(e.target.value)}
            >
              {PROVIDERS.map(p => (
                <option key={p.id} value={p.id}>{p.name}</option>
              ))}
            </select>
          </div>

          <div className="setting-group">
            <label className="setting-label">Model</label>
            <select
              className="setting-select"
              value={modelId}
              onChange={e => setModelId(e.target.value)}
            >
              {models.map(m => (
                <option key={m} value={m}>{m}</option>
              ))}
            </select>
          </div>

          <div className="setting-group">
            <label className="setting-label">API Key</label>
            <div className="setting-input-wrapper">
              <input
                type={showKey ? 'text' : 'password'}
                className="setting-input"
                value={apiKey}
                onChange={e => setApiKey(e.target.value)}
                placeholder="Enter your API key"
              />
              <button
                className="setting-input-btn"
                onClick={() => setShowKey(!showKey)}
                type="button"
              >
                {showKey ? <EyeOff size={16} /> : <Eye size={16} />}
              </button>
            </div>
            <p className="setting-hint">
              Your API key is stored locally and sent securely to the backend.
            </p>
          </div>
        </div>

        <div className="modal-footer">
          <button className="btn-secondary" onClick={onClose}>
            Cancel
          </button>
          <button className="btn-primary" onClick={handleSave}>
            {saved ? <><Check size={16} /> Saved</> : 'Save'}
          </button>
        </div>
      </div>
    </div>
  );
}
