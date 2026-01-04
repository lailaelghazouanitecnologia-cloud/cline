import { useState, useCallback, useRef } from 'react';
import { Sidebar } from './components/Sidebar';
import { ChatArea } from './components/ChatArea';
import { SettingsModal } from './components/SettingsModal';
import { useWebSocket } from './hooks/useWebSocket';
import { useSettings } from './hooks/useSettings';
import type { Session, Message, ToolCall, Central } from './types';

function generateId(): string {
  return Math.random().toString(36).substring(2, 9);
}

const WS_URL = `ws://${window.location.hostname}:3001/ws`;

interface StoredSession {
  id: string;
  title: string;
  created_at: string;
  updated_at: string;
}

export function App() {
  const [sessions, setSessions] = useState<Session[]>([]);
  const [centrals, setCentrals] = useState<Central[]>([
    { id: 'central-1', sessionId: null, title: 'Central 1' }
  ]);
  const [activeCentralId, setActiveCentralId] = useState('central-1');
  const [isLoading, setIsLoading] = useState(false);
  const [isConnected, setIsConnected] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const activeSessionRef = useRef<string | null>(null);
  const streamingMessageRef = useRef<string | null>(null);

  const { settings, updateSettings, hasApiKey } = useSettings();

  const activeCentral = centrals.find(c => c.id === activeCentralId);
  const activeSessionId = activeCentral?.sessionId || null;
  const activeSession = sessions.find(s => s.id === activeSessionId) || null;

  const handleMessage = useCallback((event: { type: string; data: unknown }) => {
    if (event.type === 'sessions') {
      const storedSessions = event.data as StoredSession[];
      setSessions(prev => {
        const existingIds = new Set(prev.map(s => s.id));
        const newSessions = storedSessions
          .filter(s => !existingIds.has(s.id))
          .map(s => ({
            id: s.id,
            title: s.title,
            createdAt: new Date(s.created_at),
            messages: [],
          }));
        return [...prev, ...newSessions];
      });
      return;
    }

    if (event.type === 'session_created') {
      const data = event.data as { id: string; title: string };
      activeSessionRef.current = data.id;
      setSessions(prev => {
        if (prev.some(s => s.id === data.id)) return prev;
        return [{
          id: data.id,
          title: data.title,
          createdAt: new Date(),
          messages: [],
        }, ...prev];
      });
      setCentrals(prev => prev.map(c =>
        c.id === activeCentralId ? { ...c, sessionId: data.id } : c
      ));
      return;
    }

    if (event.type === 'api_key_required') {
      setShowSettings(true);
      setIsLoading(false);
      return;
    }

    const sessionId = activeSessionRef.current;
    if (!sessionId) return;

    if (event.type === 'done') {
      setIsLoading(false);
      streamingMessageRef.current = null;
      return;
    }

    if (event.type === 'text_delta') {
      const data = event.data as { delta: string };
      setSessions(prev => prev.map(s => {
        if (s.id !== sessionId) return s;
        const messages = [...s.messages];
        const lastMsg = messages[messages.length - 1];

        if (lastMsg?.role === 'assistant' && streamingMessageRef.current === lastMsg.id) {
          lastMsg.content += data.delta;
        } else {
          const newId = generateId();
          streamingMessageRef.current = newId;
          messages.push({
            id: newId,
            role: 'assistant',
            content: data.delta,
            timestamp: new Date(),
          });
        }

        return { ...s, messages };
      }));
      return;
    }

    if (event.type === 'error') {
      setIsLoading(false);
      setSessions(prev => prev.map(s => {
        if (s.id !== sessionId) return s;
        return {
          ...s,
          messages: [...s.messages, {
            id: generateId(),
            role: 'assistant',
            content: `Error: ${event.data}`,
            timestamp: new Date(),
          }],
        };
      }));
      return;
    }

    setSessions(prev => prev.map(s => {
      if (s.id !== sessionId) return s;
      const messages = [...s.messages];

      if (event.type === 'message') {
        streamingMessageRef.current = null;
        const lastMsg = messages[messages.length - 1];
        if (lastMsg?.role === 'assistant' && !lastMsg.toolCalls) {
          lastMsg.content = event.data as string;
        } else {
          messages.push({
            id: generateId(),
            role: 'assistant',
            content: event.data as string,
            timestamp: new Date(),
          });
        }
      }

      if (event.type === 'approval_required') {
        const data = event.data as { id: string; tool: string; level: string };
        messages.push({
          id: generateId(),
          role: 'assistant',
          content: `Approval required for ${data.tool} (${data.level})`,
          timestamp: new Date(),
        });
      }

      if (event.type === 'tool_start') {
        const toolData = event.data as {
          id: string;
          name: string;
          input: Record<string, unknown>;
          requires_approval?: boolean;
        };
        const toolCall: ToolCall = {
          id: toolData.id,
          name: toolData.name,
          input: toolData.input,
          status: 'running',
        };
        const lastMsg = messages[messages.length - 1];
        if (lastMsg?.role === 'assistant' && lastMsg.toolCalls) {
          lastMsg.toolCalls.push(toolCall);
        } else {
          messages.push({
            id: generateId(),
            role: 'assistant',
            content: '',
            toolCalls: [toolCall],
            timestamp: new Date(),
          });
        }
      }

      if (event.type === 'tool_end') {
        const toolData = event.data as { id: string; output: string; error?: boolean };
        for (const msg of messages) {
          if (!msg.toolCalls) continue;
          const tool = msg.toolCalls.find(t => t.id === toolData.id);
          if (tool) {
            tool.status = toolData.error ? 'error' : 'success';
            tool.output = toolData.output;
            break;
          }
        }
      }

      return { ...s, messages };
    }));
  }, [activeCentralId]);

  const { send } = useWebSocket(WS_URL, {
    onMessage: handleMessage,
    onConnect: () => setIsConnected(true),
    onDisconnect: () => setIsConnected(false),
  });

  const selectSession = useCallback((sessionId: string) => {
    setCentrals(prev => prev.map(c =>
      c.id === activeCentralId ? { ...c, sessionId } : c
    ));
    activeSessionRef.current = sessionId;
  }, [activeCentralId]);

  const createSession = useCallback((initialTask?: string) => {
    if (!hasApiKey) {
      setShowSettings(true);
      return;
    }

    if (!initialTask) {
      const session: Session = {
        id: generateId(),
        title: 'New Task',
        createdAt: new Date(),
        messages: [],
      };
      setSessions(prev => [session, ...prev]);
      selectSession(session.id);
      return;
    }

    const tempId = generateId();
    activeSessionRef.current = tempId;

    const userMessage: Message = {
      id: generateId(),
      role: 'user',
      content: initialTask,
      timestamp: new Date(),
    };

    setSessions(prev => [{
      id: tempId,
      title: initialTask.slice(0, 50),
      createdAt: new Date(),
      messages: [userMessage],
    }, ...prev]);

    selectSession(tempId);
    setIsLoading(true);
    send('chat', {
      message: initialTask,
      apiKey: settings.apiKey,
      providerId: settings.providerId,
      modelId: settings.modelId,
    });
  }, [send, selectSession, hasApiKey, settings]);

  const sendMessage = useCallback((sessionId: string, content: string) => {
    if (!hasApiKey) {
      setShowSettings(true);
      return;
    }

    activeSessionRef.current = sessionId;

    const userMessage: Message = {
      id: generateId(),
      role: 'user',
      content,
      timestamp: new Date(),
    };

    setSessions(prev => prev.map(s => {
      if (s.id !== sessionId) return s;
      return { ...s, messages: [...s.messages, userMessage] };
    }));

    setIsLoading(true);
    send('chat', {
      message: content,
      sessionId,
      apiKey: settings.apiKey,
      providerId: settings.providerId,
      modelId: settings.modelId,
    });
  }, [send, hasApiKey, settings]);

  const handleSendMessage = useCallback((content: string) => {
    if (!activeSessionId) {
      createSession(content);
    } else {
      sendMessage(activeSessionId, content);
    }
  }, [activeSessionId, createSession, sendMessage]);

  const addCentral = useCallback(() => {
    const newCentral: Central = {
      id: `central-${Date.now()}`,
      sessionId: null,
      title: `Central ${centrals.length + 1}`,
    };
    setCentrals(prev => [...prev, newCentral]);
    setActiveCentralId(newCentral.id);
  }, [centrals.length]);

  const closeCentral = useCallback((centralId: string) => {
    if (centrals.length <= 1) return;
    setCentrals(prev => prev.filter(c => c.id !== centralId));
    if (activeCentralId === centralId) {
      const remaining = centrals.filter(c => c.id !== centralId);
      setActiveCentralId(remaining[0]?.id || '');
    }
  }, [centrals, activeCentralId]);

  return (
    <div className="app">
      <Sidebar
        sessions={sessions}
        activeSessionId={activeSessionId}
        onSelectSession={selectSession}
        onNewSession={(task) => createSession(task)}
        onDeleteSession={(id) => {
          send('delete_session', { sessionId: id });
          setSessions(prev => prev.filter(s => s.id !== id));
          if (activeSessionId === id) {
            selectSession('');
          }
        }}
        onOpenSettings={() => setShowSettings(true)}
        isConnected={isConnected}
        hasApiKey={hasApiKey}
        settings={settings}
      />
      <div className="centrals-area">
        <div className="centrals-tabs">
          {centrals.map(central => (
            <div
              key={central.id}
              className={`central-tab ${central.id === activeCentralId ? 'active' : ''}`}
              onClick={() => setActiveCentralId(central.id)}
            >
              <span className="central-tab-title">
                {sessions.find(s => s.id === central.sessionId)?.title || 'New'}
              </span>
              {centrals.length > 1 && (
                <button
                  className="central-tab-close"
                  onClick={(e) => {
                    e.stopPropagation();
                    closeCentral(central.id);
                  }}
                >
                  ×
                </button>
              )}
            </div>
          ))}
          <button className="central-add-btn" onClick={addCentral}>
            +
          </button>
        </div>
        <ChatArea
          session={activeSession}
          isLoading={isLoading}
          onSendMessage={handleSendMessage}
          hasApiKey={hasApiKey}
          onOpenSettings={() => setShowSettings(true)}
        />
      </div>
      <SettingsModal
        isOpen={showSettings}
        onClose={() => setShowSettings(false)}
        settings={settings}
        onSave={updateSettings}
      />
    </div>
  );
}
