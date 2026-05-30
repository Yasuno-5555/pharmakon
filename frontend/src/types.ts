export type Role = 'user' | 'agent' | 'assistant' | 'system' | 'tool';

export interface ToolCall {
  name: string;
  args: Record<string, unknown>;
}

export interface ToolResult {
  result: string;
}

export interface InteractiveComponent {
  type: string;
  payload: {
    id?: string;
    label?: string;
    style?: string;
    [key: string]: unknown;
  };
}

export interface InteractivePayload {
  id: string;
  components: InteractiveComponent[];
}

export interface Message {
  id: string;
  role: Role;
  content?: string;
  thought?: string;
  images?: string[];
  toolCall?: ToolCall;
  toolResult?: ToolResult;
  interactive?: InteractivePayload;
  context_used?: string[];
}

export interface CronJobInfo {
  id: string;
  schedule_type: string;
  expr: string;
  message: string;
}

export interface HealthStats {
  failure_rate?: number;
  last_latency?: string;
  is_healthy?: boolean;
  uptime?: number;
  connected_clients?: number;
  memory_usage?: number;
  total_tokens?: number;
  total_cost?: number;
  [key: string]: unknown;
}

export interface SwarmStatus {
  id: string;
  role: string;
  status: string;
}

export interface McpStatEntry {
  name: string;
  call_count: number;
}

export interface ToolInfo {
  name: string;
  description: string;
  category?: string;
  parameters?: {
    properties?: Record<string, unknown>;
  } | Record<string, unknown> | null;
}

export interface UsageEntry {
  timestamp?: string;
  name?: string;
  tokens: number;
  cost: number;
}

export interface Fact {
  content: string;
  source_url: string;
  confidence: number;
  timestamp: string;
}

export interface ResearchNotebook {
  current_goal: string;
  verified_facts: Fact[];
  pending_questions: string[];
  visited_urls: Record<string, string>;
  dead_ends: string[];
  research_tree: Record<string, string[]>;
}

export interface AppSettings {
  model: string;
  temperature: number;
  auto_approval: boolean;
  max_tokens: number;
  api_key?: string;
}

export interface ServerEvent {
  type: string;
  data?: unknown;
}
