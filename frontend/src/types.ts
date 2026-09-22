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
}

export const emptySnapshot: AppSnapshot = {
  connection: { status: 'disconnected', reason: null, server_name: null, server_address: null },
  channels: [],
  bookmarks: [],
  last_channel: null,
  runtime_available: true,
}
