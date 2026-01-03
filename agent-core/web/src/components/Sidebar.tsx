import { useState, KeyboardEvent } from 'react';
import { Send, FolderGit2, Cpu, MessageSquare, Archive, ChevronDown } from 'lucide-react';
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

  if (minutes < 1) return 'Just now';
  if (minutes < 60) return `${minutes}m ago`;
  if (minutes < 1440) return `${Math.floor(minutes / 60)}h ago`;
  return date.toLocaleDateString();
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

  const handleSubmit = () => {
    if (!inputValue.trim() || !isConnected) return;
    onNewSession(inputValue.trim());
    setInputValue('');
  };

  const handleKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
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
      <div className="sidebar-input-area">
        <div className="sidebar-input-wrapper">
          <input
            type="text"
            className="sidebar-input"
            placeholder="What do you want to do?"
            value={inputValue}
            onChange={(e) => setInputValue(e.target.value)}
            onKeyDown={handleKeyDown}
            disabled={!isConnected}
          />
          <button
            className="sidebar-input-btn"
            onClick={handleSubmit}
            disabled={!inputValue.trim() || !isConnected}
          >
            <Send size={16} />
          </button>
        </div>
      </div>

      <div className="sidebar-dropdowns">
        <button className="dropdown-btn">
          <FolderGit2 size={14} />
          <span>cline-agent</span>
          <ChevronDown size={12} />
        </button>
        <button className="dropdown-btn">
          <Cpu size={14} />
          <span>groq</span>
          <ChevronDown size={12} />
        </button>
      </div>

      <div className="sessions-header">
        <span className="sessions-label">Sessions</span>
      </div>

      <div className="sessions-list">
        {sessions.map(session => (
          <div
            key={session.id}
            className={`session-item ${session.id === activeSessionId ? 'active' : ''}`}
            onClick={() => onSelectSession(session.id)}
          >
            <div className="session-icon">
              <MessageSquare size={16} />
            </div>
            <div className="session-content">
              <div className="session-title">{session.title}</div>
              <div className="session-time">{formatTime(session.createdAt)}</div>
            </div>
            <div className="session-actions">
              <button
                className="session-action-btn"
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

      <div className="sidebar-footer">
        <div className="user-avatar">U</div>
        <div className="user-info">
          <div className="user-name">User</div>
        </div>
        <div className="connection-status">
          <span className={`connection-dot ${isConnected ? 'connected' : 'disconnected'}`} />
          <span>{isConnected ? 'Connected' : 'Offline'}</span>
        </div>
      </div>
    </aside>
  );
}
