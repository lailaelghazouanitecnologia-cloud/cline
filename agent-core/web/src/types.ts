export interface Session {
  id: string;
  title: string;
  createdAt: Date;
  messages: Message[];
}

export interface Message {
  id: string;
  role: 'user' | 'assistant' | 'tool';
  content: string;
  toolCalls?: ToolCall[];
  timestamp: Date;
}

export interface ToolCall {
  id: string;
  name: string;
  input: Record<string, unknown>;
  status: 'running' | 'success' | 'error';
  output?: string;
}

export interface AgentEvent {
  type: 'message' | 'tool_start' | 'tool_end' | 'error' | 'done';
  data: unknown;
}

export interface Workspace {
  id: string;
  name: string;
  path: string;
  isActive: boolean;
}

export interface FileNode {
  name: string;
  path: string;
  type: 'file' | 'folder';
  children?: FileNode[];
  isExpanded?: boolean;
}

export interface Bookmark {
  id: string;
  name: string;
  path: string;
  line?: number;
}

export interface Widget {
  id: string;
  type: 'workspace' | 'bookmarks' | 'sessions' | 'custom';
  title: string;
  isCollapsed: boolean;
}

export interface Central {
  id: string;
  sessionId: string | null;
  title: string;
}
