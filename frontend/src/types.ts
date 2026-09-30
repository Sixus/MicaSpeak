export type ConnectionStatus = 'disconnected' | 'connecting' | 'connected'

export type ClientNode = {
  id: number
  name: string
  channel_id: number
  is_self: boolean
}

export type ChannelNode = {
  id: number
  parent_id: number | null
  name: string
  order: number
  password: boolean
  clients: ClientNode[]
}

export type Bookmark = {
  id: string
  address: string
  nickname: string
  password_saved: boolean
  last_channel: string | null
}

// M6a 身份视图：脱敏（无密钥材料），结构镜像 Rust identity.rs。
export type IdentityView = {
  id: string
  label: string
  uid_masked: string
  level: number
  active: boolean
}

export type TalkerState = {
  client_id: number
  name: string
  is_self: boolean
  last_active_ms: number
}

export type VoiceMode = 'ptt' | 'vad'

export type VoiceSettings = {
  mode: VoiceMode
  ptt_key_vk: number
  vad_threshold: number
  denoise: boolean
  input_device: string | null
  output_device: string | null
  hotkey_installed: boolean
}

// M4 聊天：结构镜像 Rust chat.rs（snake_case 序列化）。
export type ChatMessageView = {
  id: number
  from_client_id: number
  from_name: string
  is_self: boolean
  text: string
  time_ms: number
}

export type ChatTabView = {
  kind: 'channel' | 'private'
  target_id: number
  title: string
  unread: number
  messages: ChatMessageView[]
}

export type ChatMessageEvent = {
  kind: 'channel' | 'private'
  target_id: number
  unread: number
  message: ChatMessageView
}

export type AppSnapshot = {
  connection: {
    status: ConnectionStatus
    reason: string | null
    server_name: string | null
    server_address: string | null
  }
  channels: ChannelNode[]
  bookmarks: Bookmark[]
  last_channel: string | null
  runtime_available: boolean
  talking: TalkerState[]
  voice: VoiceSettings
  overlay_enabled: boolean
  own_channel_id: number
  chat: ChatTabView[]
  /** M5a：'Mica' 或 '实体（回退…）'；旧后端缺省 undefined */
  material?: string
  /** M5c：Evergreen WebView2 版本；旧后端缺省 undefined */
  webview2_version?: string | null
  /** M6a：身份列表（脱敏）；旧后端缺省 undefined */
  identities?: IdentityView[]
  /** 四轮：服务器备注表（键=连接地址字符串）；旧后端缺省 undefined */
  server_remarks?: Record<string, string>
}

export const emptySnapshot: AppSnapshot = {
  connection: { status: 'disconnected', reason: null, server_name: null, server_address: null },
  channels: [],
  bookmarks: [],
  last_channel: null,
  runtime_available: true,
  talking: [],
  own_channel_id: 0,
  chat: [],
  overlay_enabled: false,
  voice: {
    mode: 'ptt',
    ptt_key_vk: 0xa2,
    vad_threshold: 0.5,
    denoise: false,
    input_device: null,
    output_device: null,
    hotkey_installed: false,
  },
}
