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
