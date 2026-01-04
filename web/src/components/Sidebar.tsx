import { useState, useRef, KeyboardEvent, useEffect } from 'react';
import {
  Send,
  Image,
  MoreHorizontal,
  FolderGit2,
  Cloud,
  Archive,
  ChevronDown,
  ChevronRight,
  Settings,
  Key,
} from 'lucide-react';
import { WorkspaceWidget } from './widgets/WorkspaceWidget';
import { BookmarksWidget } from './widgets/BookmarksWidget';
import type { Session, Workspace, FileNode, Bookmark, Widget, AppSettings } from '../types';

interface SidebarProps {
  sessions: Session[];
  activeSessionId: string | null;
  onSelectSession: (id: string) => void;
  onNewSession: (task?: string) => void;
  onDeleteSession?: (id: string) => void;
  onOpenSettings?: () => void;
  isConnected?: boolean;
  hasApiKey?: boolean;
  settings?: AppSettings;
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

const defaultWorkspaces: Workspace[] = [
  { id: '1', name: 'cline-agent', path: '/home/user/cline/agent-core', isActive: true },
];

const defaultFileTree: FileNode[] = [
  {
    name: 'src',
    path: '/src',
    type: 'folder',
    children: [
      { name: 'main.rs', path: '/src/main.rs', type: 'file' },
      { name: 'lib.rs', path: '/src/lib.rs', type: 'file' },
    ],
  },
  {
    name: 'web',
    path: '/web',
    type: 'folder',
    children: [
      { name: 'App.tsx', path: '/web/App.tsx', type: 'file' },
      { name: 'styles.css', path: '/web/styles.css', type: 'file' },
    ],
  },
  { name: 'Cargo.toml', path: '/Cargo.toml', type: 'file' },
];

const defaultWidgets: Widget[] = [
  { id: 'workspace', type: 'workspace', title: 'Workspace', isCollapsed: false },
  { id: 'bookmarks', type: 'bookmarks', title: 'Bookmarks', isCollapsed: true },
  { id: 'sessions', type: 'sessions', title: 'Sessions', isCollapsed: false },
];

export function Sidebar({
  sessions,
  activeSessionId,
  onSelectSession,
  onNewSession,
  onDeleteSession,
  onOpenSettings,
  isConnected = false,
  hasApiKey = false,
  settings,
}: SidebarProps) {
  const [inputValue, setInputValue] = useState('');
  const [workspaces, setWorkspaces] = useState<Workspace[]>(defaultWorkspaces);
  const [fileTree] = useState<FileNode[]>(defaultFileTree);
  const [bookmarks, setBookmarks] = useState<Bookmark[]>([]);
  const [widgets, setWidgets] = useState<Widget[]>(defaultWidgets);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const activeWorkspace = workspaces.find(w => w.isActive) || null;

  useEffect(() => {
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto';
      textareaRef.current.style.height = Math.min(textareaRef.current.scrollHeight, 200) + 'px';
    }
  }, [inputValue]);

  const handleSubmit = () => {
    if (!inputValue.trim()) return;
    if (!hasApiKey) {
      onOpenSettings?.();
      return;
    }
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

  const toggleWidget = (widgetId: string) => {
    setWidgets(prev => prev.map(w =>
      w.id === widgetId ? { ...w, isCollapsed: !w.isCollapsed } : w
    ));
  };

  const handleSelectWorkspace = (id: string) => {
    setWorkspaces(prev => prev.map(w => ({ ...w, isActive: w.id === id })));
  };

  const handleAddWorkspace = () => {
    const newWs: Workspace = {
      id: Date.now().toString(),
      name: 'New Workspace',
      path: '/path/to/workspace',
      isActive: false,
    };
    setWorkspaces(prev => [...prev, newWs]);
  };

  const handleAddBookmark = () => {
    const newBm: Bookmark = {
      id: Date.now().toString(),
      name: 'new-file.ts',
      path: '/src/new-file.ts',
    };
    setBookmarks(prev => [...prev, newBm]);
  };

  const handleRemoveBookmark = (id: string) => {
    setBookmarks(prev => prev.filter(b => b.id !== id));
  };

  const renderWidget = (widget: Widget) => {
    const isCollapsed = widget.isCollapsed;

    if (widget.type === 'workspace') {
      return (
        <div key={widget.id} className="widget-section">
          <button className="widget-toggle" onClick={() => toggleWidget(widget.id)}>
            {isCollapsed ? <ChevronRight size={14} /> : <ChevronDown size={14} />}
            <span>{widget.title}</span>
          </button>
          {!isCollapsed && (
            <WorkspaceWidget
              workspaces={workspaces}
              activeWorkspace={activeWorkspace}
              fileTree={fileTree}
              onSelectWorkspace={handleSelectWorkspace}
              onAddWorkspace={handleAddWorkspace}
              onRefresh={() => {}}
              onSelectFile={() => {}}
            />
          )}
        </div>
      );
    }

    if (widget.type === 'bookmarks') {
      return (
        <div key={widget.id} className="widget-section">
          <button className="widget-toggle" onClick={() => toggleWidget(widget.id)}>
            {isCollapsed ? <ChevronRight size={14} /> : <ChevronDown size={14} />}
            <span>{widget.title}</span>
          </button>
          {!isCollapsed && (
            <BookmarksWidget
              bookmarks={bookmarks}
              onSelectBookmark={() => {}}
              onRemoveBookmark={handleRemoveBookmark}
              onAddBookmark={handleAddBookmark}
            />
          )}
        </div>
      );
    }

    if (widget.type === 'sessions') {
      return (
        <div key={widget.id} className="widget-section sessions-section">
          <button className="widget-toggle" onClick={() => toggleWidget(widget.id)}>
            {isCollapsed ? <ChevronRight size={14} /> : <ChevronDown size={14} />}
            <span>{widget.title}</span>
          </button>
          {!isCollapsed && (
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
          )}
        </div>
      );
    }

    return null;
  };

  return (
    <aside className="sidebar">
      <header className="sidebar-header">
        <div className="logo-row">
          <span className="logo-text">Cline Agent</span>
          <span className="badge">Preview</span>
        </div>

        {!hasApiKey && (
          <button className="api-key-banner" onClick={onOpenSettings}>
            <Key size={14} />
            <span>Configure API Key</span>
          </button>
        )}

        <div className="input-container">
          <textarea
            ref={textareaRef}
            className="input-textarea"
            placeholder={hasApiKey ? "Ask Claude to write code..." : "Configure API key first..."}
            value={inputValue}
            onChange={(e) => setInputValue(e.target.value)}
            onKeyDown={handleKeyDown}
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
              disabled={!inputValue.trim()}
              type="button"
            >
              <Send size={14} />
            </button>
          </div>

          <div className="selectors-row">
            <button className="selector-btn flex-grow" type="button">
              <FolderGit2 size={16} />
              <span>{activeWorkspace?.name || 'Select workspace'}</span>
            </button>
            <div className="divider-vertical" />
            <button className="selector-btn" type="button" onClick={onOpenSettings}>
              <Cloud size={16} />
              <span>{settings?.providerId || 'groq'}</span>
            </button>
          </div>
        </div>
      </header>

      <div className="sidebar-widgets">
        {widgets.map(widget => renderWidget(widget))}
      </div>

      <footer className="sidebar-footer">
        <button className="user-avatar-btn" type="button">
          <div className="user-avatar">U</div>
        </button>
        <div className="footer-actions">
          <button className="settings-btn" onClick={onOpenSettings} title="Settings">
            <Settings size={16} />
          </button>
          <div className="connection-indicator">
            <span className={`connection-dot ${isConnected ? 'connected' : 'disconnected'}`} />
            <span>{isConnected ? 'Connected' : 'Offline'}</span>
          </div>
        </div>
      </footer>
    </aside>
  );
}
