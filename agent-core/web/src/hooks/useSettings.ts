import { useState, useEffect, useCallback } from 'react';
import type { AppSettings } from '../types';

const STORAGE_KEY = 'cline-agent-settings';

const defaultSettings: AppSettings = {
  apiKey: '',
  providerId: 'groq',
  modelId: 'llama-3.3-70b-versatile',
};

function loadSettings(): AppSettings {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored) {
      const parsed = JSON.parse(stored);
      return { ...defaultSettings, ...parsed };
    }
  } catch {
    console.warn('Failed to load settings from localStorage');
  }
  return defaultSettings;
}

function saveSettings(settings: AppSettings): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
  } catch {
    console.warn('Failed to save settings to localStorage');
  }
}

export function useSettings() {
  const [settings, setSettings] = useState<AppSettings>(loadSettings);

  useEffect(() => {
    saveSettings(settings);
  }, [settings]);

  const updateSettings = useCallback((newSettings: AppSettings) => {
    setSettings(newSettings);
  }, []);

  const hasApiKey = settings.apiKey.length > 0;

  return {
    settings,
    updateSettings,
    hasApiKey,
  };
}
