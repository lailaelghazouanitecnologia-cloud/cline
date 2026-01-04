import { useState, useRef, KeyboardEvent, useEffect } from 'react';
import { Send, Image, MoreHorizontal, FolderGit2, Cloud, Archive } from 'lucide-react';
import type { Session } from '../types';

interface SidebarProps {
  sessions: Session[];
  activeSessionId: string | null;
  onSelectSession: (id: string) => void;
  onNewSession: (task?: string) => void;
  onDeleteSession?: (id: string) => void;
  isConnected?: boolean;
}

function formatTime(date: Date): string {
  const now = new Date();
  const diff = now.getTime() - date.getTime();
  const minutes = Math.floor(diff / 60000);
  const hours = Math.floor(minutes / 60);
  const days = Math.floor(hours / 24);

  if (minutes < 1) return 'Just now';
  if (minutes < 60) return `${minutes}m ago`;
  if (hours < 24) {
    return date.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
  }
  if (days === 1) return 'Yesterday';
  if (days < 7) {
    return date.toLocaleDateString([], { weekday: 'short' });
  }
  return date.toLocaleDateString([], { month: 'short', day: 'numeric' });
}

export function Sidebar({
  sessions,
  activeSessionId,
  onSelectSession,
  onNewSession,
  onDeleteSession,
  isConnected = false,
}: SidebarProps) {
  const [inputValue, setInputValue] = useState('');
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto';
      textareaRef.current.style.height = Math.min(textareaRef.current.scrollHeight, 200) + 'px';
    }
  }, [inputValue]);

  const handleSubmit = () => {
    if (!inputValue.trim() || !isConnected) return;
    onNewSession(inputValue.trim());
    setInputValue('');
  };

  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSubmit();
    }
  };

  const handleArchive = (e: React.MouseEvent, sessionId: string) => {
    e.stopPropagation();
    onDeleteSession?.(sessionId);
  };

  return (
    <aside className="sidebar">
      <header className="sidebar-header">
        <div className="logo-row">
          <span className="logo-text">Cline Agent</span>
          <span className="badge">Preview</span>
        </div>

        <div className="input-container">
          <textarea
            ref={textareaRef}
            className="input-textarea"
            placeholder="Ask Claude to write code..."
            value={inputValue}
            onChange={(e) => setInputValue(e.target.value)}
            onKeyDown={handleKeyDown}
            disabled={!isConnected}
            rows={2}
          />

          <div className="input-actions">
            <div className="input-actions-left">
              <button className="btn-icon" type="button" title="Add images">
                <Image size={16} />
              </button>
              <button className="btn-icon" type="button" title="More options">
                <MoreHorizontal size={16} />
              </button>
            </div>
            <button
              className="btn-submit"
              onClick={handleSubmit}
              disabled={!inputValue.trim() || !isConnected}
              type="button"
            >
              <Send size={14} />
            </button>
          </div>

          <div className="selectors-row">
            <button className="selector-btn flex-grow" type="button">
              <FolderGit2 size={16} />
              <span>cline-agent</span>
            </button>
            <div className="divider-vertical" />
            <button className="selector-btn" type="button">
              <Cloud size={16} />
              <span>groq</span>
            </button>
          </div>
        </div>
      </header>

      <div className="sessions-header">
        <span className="sessions-title">Sessions</span>
      </div>

      <div className="sessions-list">
        {sessions.map((session, index) => (
          <div
            key={session.id}
            className={`session-item ${session.id === activeSessionId ? 'active' : ''}`}
            onClick={() => onSelectSession(session.id)}
            style={{ animationDelay: `${index * 30}ms` }}
          >
            <div className="session-content">
              <p className="session-title">{session.title}</p>
              <span className="session-meta">
                <span>{formatTime(session.createdAt)}</span>
              </span>
            </div>
            <div className="session-actions">
              <button
                className="session-archive-btn"
                onClick={(e) => handleArchive(e, session.id)}
                title="Archive"
              >
                <Archive size={14} />
              </button>
            </div>
          </div>
        ))}

        {sessions.length === 0 && (
          <div className="empty-sessions">
            {isConnected ? 'No sessions yet' : 'Connecting...'}
          </div>
        )}
      </div>

      <footer className="sidebar-footer">
        <button className="user-avatar-btn" type="button">
          <div className="user-avatar">U</div>
        </button>
        <div className="footer-actions">
          <div className="connection-indicator">
            <span className={`connection-dot ${isConnected ? 'connected' : 'disconnected'}`} />
            <span>{isConnected ? 'Connected' : 'Offline'}</span>
          </div>
        </div>
      </footer>
    </aside>
  );
}
