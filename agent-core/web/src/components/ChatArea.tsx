import { useState, useRef, useEffect } from 'react';
import { ArrowUp, Terminal, Zap, BookOpen } from 'lucide-react';
import type { Session, Message, ToolCall } from '../types';

interface ChatAreaProps {
  session: Session | null;
  isLoading: boolean;
  onSendMessage: (content: string) => void;
  hasApiKey?: boolean;
  onOpenSettings?: () => void;
}

function ToolCallCard({ tool }: { tool: ToolCall }) {
  return (
    <div className="tool-call">
      <div className="tool-header">
        <Terminal className="tool-icon" />
        <span className="tool-name">{tool.name}</span>
        <span className={`tool-status ${tool.status}`}>
          {tool.status === 'running' ? 'Running...' : tool.status}
        </span>
      </div>
      {tool.output && (
        <div className="tool-body">
          <pre>{tool.output.slice(0, 500)}{tool.output.length > 500 ? '...' : ''}</pre>
        </div>
      )}
    </div>
  );
}

function MessageBubble({ message }: { message: Message }) {
  if (message.toolCalls && message.toolCalls.length > 0) {
    return (
      <>
        {message.content && (
          <div className={`message ${message.role}`}>{message.content}</div>
        )}
        {message.toolCalls.map(tool => (
          <ToolCallCard key={tool.id} tool={tool} />
        ))}
      </>
    );
  }

  return (
    <div className={`message ${message.role}`}>
      {message.content}
    </div>
  );
}

export function ChatArea({
  session,
  isLoading,
  onSendMessage,
  hasApiKey = true,
  onOpenSettings,
}: ChatAreaProps) {
  const [input, setInput] = useState('');
  const chatRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    if (chatRef.current) {
      chatRef.current.scrollTop = chatRef.current.scrollHeight;
    }
  }, [session?.messages]);

  useEffect(() => {
    if (inputRef.current) {
      inputRef.current.style.height = 'auto';
      inputRef.current.style.height = Math.min(inputRef.current.scrollHeight, 200) + 'px';
    }
  }, [input]);

  const handleSubmit = () => {
    const trimmed = input.trim();
    if (!trimmed || isLoading) return;
    if (!hasApiKey) {
      onOpenSettings?.();
      return;
    }
    onSendMessage(trimmed);
    setInput('');
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSubmit();
    }
  };

  if (!session) {
    return (
      <main className="main-area">
        <div className="main-container">
          <div className="main-gradient" />
          <div className="empty-state">
            <Zap size={48} style={{ opacity: 0.3 }} />
            <h2>Start a new task</h2>
            <p>Type a task in the sidebar to begin</p>
          </div>
          <div className="input-wrapper">
            <div className="input-area">
              <div className="input-inner">
                <textarea
                  ref={inputRef}
                  className="task-input"
                  placeholder="Ask anything..."
                  value={input}
                  onChange={e => setInput(e.target.value)}
                  onKeyDown={handleKeyDown}
                  rows={1}
                />
                <button
                  className="send-btn"
                  onClick={handleSubmit}
                  disabled={!input.trim() || isLoading}
                >
                  <ArrowUp size={20} />
                </button>
              </div>
              <div className="input-footer">
                <div className="input-footer-left">
                  <button className="input-footer-btn" type="button">
                    <BookOpen size={16} />
                    <span>Deep Thinking</span>
                  </button>
                </div>
                <span className="input-footer-text">Press Enter to send</span>
              </div>
            </div>
          </div>
        </div>
      </main>
    );
  }

  return (
    <main className="main-area">
      <div className="main-container">
        <div className="main-gradient" />
        <div className="chat-container" ref={chatRef}>
          <div className="chat-messages">
            {session.messages.map(msg => (
              <MessageBubble key={msg.id} message={msg} />
            ))}
            {isLoading && (
              <div className="message assistant" style={{ opacity: 0.6 }}>
                Thinking...
              </div>
            )}
          </div>
        </div>
        <div className="input-wrapper">
          <div className="input-area">
            <div className="input-inner">
              <textarea
                ref={inputRef}
                className="task-input"
                placeholder="Ask anything..."
                value={input}
                onChange={e => setInput(e.target.value)}
                onKeyDown={handleKeyDown}
                rows={1}
              />
              <button
                className="send-btn"
                onClick={handleSubmit}
                disabled={!input.trim() || isLoading}
              >
                <ArrowUp size={20} />
              </button>
            </div>
            <div className="input-footer">
              <div className="input-footer-left">
                <button className="input-footer-btn" type="button">
                  <BookOpen size={16} />
                  <span>Deep Thinking</span>
                </button>
              </div>
              <span className="input-footer-text">Press Enter to send</span>
            </div>
          </div>
        </div>
      </div>
    </main>
  );
}
