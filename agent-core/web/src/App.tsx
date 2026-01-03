import { useState, useCallback } from 'react';
import { Sidebar } from './components/Sidebar';
import { ChatArea } from './components/ChatArea';
import type { Session, Message, ToolCall } from './types';

function generateId(): string {
  return Math.random().toString(36).substring(2, 9);
}

export function App() {
  const [sessions, setSessions] = useState<Session[]>([]);
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);

  const activeSession = sessions.find(s => s.id === activeSessionId) || null;

  const createSession = useCallback((initialTask?: string) => {
    const session: Session = {
      id: generateId(),
      title: initialTask?.slice(0, 50) || 'New Task',
      createdAt: new Date(),
      messages: [],
    };
    setSessions(prev => [session, ...prev]);
    setActiveSessionId(session.id);

    if (initialTask) {
      sendMessage(session.id, initialTask);
    }
  }, []);

  const sendMessage = async (sessionId: string, content: string) => {
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

    try {
      const response = await fetch('/chat', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ message: content, sessionId }),
      });

      const reader = response.body?.getReader();
      if (!reader) throw new Error('No response body');

      const decoder = new TextDecoder();
      let buffer = '';

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split('\n');
        buffer = lines.pop() || '';

        for (const line of lines) {
          if (!line.startsWith('data: ')) continue;
          const data = line.slice(6);
          if (data === '[DONE]') continue;

          try {
            const event = JSON.parse(data);
            handleAgentEvent(sessionId, event);
          } catch {
            continue;
          }
        }
      }
    } catch (error) {
      const errorMessage: Message = {
        id: generateId(),
        role: 'assistant',
        content: `Error: ${error instanceof Error ? error.message : 'Unknown error'}`,
        timestamp: new Date(),
      };
      setSessions(prev => prev.map(s => {
        if (s.id !== sessionId) return s;
        return { ...s, messages: [...s.messages, errorMessage] };
      }));
    } finally {
      setIsLoading(false);
    }
  };

  const handleAgentEvent = (sessionId: string, event: { type: string; data: unknown }) => {
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

      if (event.type === 'tool_start') {
        const toolData = event.data as { id: string; name: string; input: Record<string, unknown> };
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
  };

  const handleSendMessage = (content: string) => {
    if (!activeSessionId) {
      createSession(content);
    } else {
      sendMessage(activeSessionId, content);
    }
  };

  return (
    <div className="app">
      <Sidebar
        sessions={sessions}
        activeSessionId={activeSessionId}
        onSelectSession={setActiveSessionId}
        onNewSession={() => createSession()}
      />
      <ChatArea
        session={activeSession}
        isLoading={isLoading}
        onSendMessage={handleSendMessage}
      />
    </div>
  );
}
