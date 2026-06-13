import { invoke, Channel } from "@tauri-apps/api/core";

export interface Conversation {
  id: string;
  title: string;
  createdAt: string;
  updatedAt: string;
}

export type TaskHint =
  | "quick"
  | "code-complete"
  | "write"
  | "explain-file"
  | "agentic"
  | "private"
  | "best";

export interface Message {
  id: string;
  conversationId: string;
  role: "user" | "assistant" | "system";
  content: string;
  createdAt: string;
  tier?: string | null;
  servedModel?: string | null;
}

export interface Settings {
  fireflyEndpoint: string;
  onDeviceEndpoint: string;
  onDeviceModel: string;
  modelCode: string;
  modelChatHeavy: string;
  modelFrontier: string;
  syncEndpoint: string;
  deviceName: string;
  memoryEnabled: boolean;
}

export type StreamEvent =
  | { type: "started" }
  | { type: "routed"; tier: string; model: string; degraded: boolean }
  | { type: "reasoning"; text: string }
  | { type: "token"; text: string }
  | { type: "done" }
  | { type: "error"; message: string };

export const listConversations = () =>
  invoke<Conversation[]>("list_conversations");

export const getMessages = (conversationId: string) =>
  invoke<Message[]>("get_messages", { conversationId });

export const createConversation = (title: string) =>
  invoke<Conversation>("create_conversation", { title });

export const renameConversation = (conversationId: string, title: string) =>
  invoke<Conversation>("rename_conversation", { conversationId, title });

export const deleteConversation = (conversationId: string) =>
  invoke<void>("delete_conversation", { conversationId });

export const generateConversationTitle = (conversationId: string, firstMessage: string) =>
  invoke<Conversation>("generate_conversation_title", { conversationId, firstMessage });

export interface Profile {
  userId: string;
  displayName: string;
  profile: "kid" | "adult";
  active: boolean;
}

export interface DeviceSummary {
  id: string;
  name: string;
  lastSync?: string | null;
}

export const listProfiles = () => invoke<Profile[]>("list_profiles");

export const switchProfile = (userId: string) =>
  invoke<Profile[]>("switch_profile", { userId });

export const refreshActiveProfile = () =>
  invoke<Profile[]>("refresh_active_profile");

// Clear the active profile's local identity and return the remaining profiles.
export const signOut = () => invoke<Profile[]>("sign_out");

// Onboarding: create an account, then register/claim this device.
export const signup = (username: string, password: string, displayName: string) =>
  invoke<void>("signup", { username, password, displayName });

export const login = (username: string, password: string) =>
  invoke<DeviceSummary[]>("login", { username, password });

export const registerDevice = (name: string) =>
  invoke<Profile[]>("register_device", { name });

export const claimDevice = (deviceId: string) =>
  invoke<Profile[]>("claim_device", { deviceId });

export interface SyncStatus {
  ok: boolean;
  pushed: number;
  pulled: number;
  cursor: string;
  message?: string | null;
}

export const syncNow = () => invoke<SyncStatus>("sync_now");

export const getSettings = () => invoke<Settings>("get_settings");

export const setSettings = (settings: Settings) =>
  invoke<void>("set_settings", { settings });

export const checkFirefly = () => invoke<boolean>("check_firefly");

export const platform = () => invoke<string>("platform");

export type OnDeviceStatus =
  | { state: "ready"; model: string }
  | { state: "modelMissing"; model: string }
  | { state: "unreachable" };

export interface PullProgress {
  status: string;
  completed?: number;
  total?: number;
}

export const checkOnDevice = () => invoke<OnDeviceStatus>("check_on_device");

export function pullOnDeviceModel(
  onProgress: (p: PullProgress) => void,
): Promise<void> {
  const channel = new Channel<PullProgress>();
  channel.onmessage = onProgress;
  return invoke<void>("pull_on_device_model", { onPull: channel });
}

/** Send a message and stream the assistant reply. Resolves with the assistant
 *  message id once the stream completes. The token never reaches the webview. */
export function sendMessage(
  conversationId: string,
  content: string,
  task: TaskHint,
  onEvent: (event: StreamEvent) => void,
): Promise<string> {
  const channel = new Channel<StreamEvent>();
  channel.onmessage = onEvent;
  return invoke<string>("send_message", {
    conversationId,
    content,
    task,
    onToken: channel,
  });
}
