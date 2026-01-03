import { MessageSquarePlus, Zap } from 'lucide-react';
import type { Session } from '../types';

interface SidebarProps {
  sessions: Session[];
  activeSessionId: string | null;
  onSelectSession: (id: string) => void;
  onNewSession: () => void;
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

export function Sidebar({ sessions, activeSessionId, onSelectSession, onNewSession }: SidebarProps) {
  return (
    <aside className="sidebar">
      <header className="sidebar-header">
        <Zap size={20} />
        <h1>Cline Agent</h1>
      </header>

      <button className="new-task-btn" onClick={onNewSession}>
        <MessageSquarePlus size={16} style={{ marginRight: 8, verticalAlign: 'middle' }} />
        New Task
      </button>

      <div className="sessions-list">
        {sessions.map(session => (
          <div
            key={session.id}
            className={`session-item ${session.id === activeSessionId ? 'active' : ''}`}
            onClick={() => onSelectSession(session.id)}
          >
            <div className="session-title">{session.title}</div>
            <div className="session-time">{formatTime(session.createdAt)}</div>
          </div>
        ))}

        {sessions.length === 0 && (
          <div style={{ padding: 16, color: 'var(--text-muted)', fontSize: 13 }}>
            No tasks yet. Start a new one!
          </div>
        )}
      </div>
    </aside>
  );
}
