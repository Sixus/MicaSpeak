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
}

export const emptySnapshot: AppSnapshot = {
  connection: { status: 'disconnected', reason: null, server_name: null, server_address: null },
  channels: [],
  bookmarks: [],
  last_channel: null,
  runtime_available: true,
  talking: [],
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
