import { useState, useCallback, useRef } from 'react';
import { Sidebar } from './components/Sidebar';
import { ChatArea } from './components/ChatArea';
import { useWebSocket } from './hooks/useWebSocket';
import type { Session, Message, ToolCall } from './types';

function generateId(): string {
  return Math.random().toString(36).substring(2, 9);
}

const WS_URL = `ws://${window.location.hostname}:3001/ws`;

export function App() {
  const [sessions, setSessions] = useState<Session[]>([]);
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [isConnected, setIsConnected] = useState(false);
  const activeSessionRef = useRef<string | null>(null);

  const activeSession = sessions.find(s => s.id === activeSessionId) || null;

  const handleMessage = useCallback((event: { type: string; data: unknown }) => {
    const sessionId = activeSessionRef.current;
    if (!sessionId) return;

    if (event.type === 'done') {
      setIsLoading(false);
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
        messages.push({
          id: generateId(),
          role: 'assistant',
          content: event.data as string,
          timestamp: new Date(),
        });
      }

      if (event.type === 'approval_required') {
        const data = event.data as { id: string; tool: string; level: string };
        messages.push({
          id: generateId(),
          role: 'assistant',
          content: `⚠️ Approval required for ${data.tool} (${data.level})`,
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
  }, []);

  const { send } = useWebSocket(WS_URL, {
    onMessage: handleMessage,
    onConnect: () => setIsConnected(true),
    onDisconnect: () => setIsConnected(false),
  });

  const createSession = useCallback((initialTask?: string) => {
    const session: Session = {
      id: generateId(),
      title: initialTask?.slice(0, 50) || 'New Task',
      createdAt: new Date(),
      messages: [],
    };
    setSessions(prev => [session, ...prev]);
    setActiveSessionId(session.id);
    activeSessionRef.current = session.id;

    if (initialTask) {
      const userMessage: Message = {
        id: generateId(),
        role: 'user',
        content: initialTask,
        timestamp: new Date(),
      };
      setSessions(prev => prev.map(s => {
        if (s.id !== session.id) return s;
        return { ...s, messages: [userMessage] };
      }));
      setIsLoading(true);
      send('chat', { message: initialTask });
    }
  }, [send]);

  const sendMessage = useCallback((sessionId: string, content: string) => {
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
    send('chat', { message: content });
  }, [send]);

  const handleSendMessage = useCallback((content: string) => {
    if (!activeSessionId) {
      createSession(content);
    } else {
      sendMessage(activeSessionId, content);
    }
  }, [activeSessionId, createSession, sendMessage]);

  return (
    <div className="app">
      <Sidebar
        sessions={sessions}
        activeSessionId={activeSessionId}
        onSelectSession={(id) => {
          setActiveSessionId(id);
          activeSessionRef.current = id;
        }}
        onNewSession={() => createSession()}
        isConnected={isConnected}
      />
      <ChatArea
        session={activeSession}
        isLoading={isLoading}
        onSendMessage={handleSendMessage}
      />
    </div>
  );
}
