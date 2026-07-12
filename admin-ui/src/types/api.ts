// Copyright (c) 2026 Harllan He. Licensed under MIT.
// 凭据状态响应
export interface CredentialsStatusResponse {
  total: number
  available: number
  currentId: number
  credentials: CredentialStatusItem[]
}

// 单个凭据状态
export interface CredentialStatusItem {
  id: number
  priority: number
  disabled: boolean
  failureCount: number
  isCurrent: boolean
  expiresAt: string | null
  authMethod: string | null
  hasProfileArn: boolean
  profileArn?: string
  region?: string
  authRegion?: string
  apiRegion?: string
  email?: string
  nickname?: string
  refreshTokenHash?: string
  successCount: number
  lastUsedAt: string | null
  hasProxy: boolean
  proxyUrl?: string
  healthStatus: 'healthy' | 'warning' | 'degraded' | 'unhealthy' | 'disabled'
  throttleCount: number
}

// 余额响应
export interface BalanceResponse {
  id: number
  subscriptionTitle: string | null
  currentUsage: number
  usageLimit: number
  remaining: number
  usagePercentage: number
  nextResetAt: number | null
}

// 成功响应
export interface SuccessResponse {
  success: boolean
  message: string
}

// 错误响应
export interface AdminErrorResponse {
  error: {
    type: string
    message: string
  }
}

// 请求类型
export interface SetDisabledRequest {
  disabled: boolean
}

export interface SetPriorityRequest {
  priority: number
}

// 添加凭据请求
export interface AddCredentialRequest {
  refreshToken: string
  authMethod?: 'social' | 'idc' | 'external_idp'
  email?: string
  nickname?: string
  clientId?: string
  clientSecret?: string
  tokenEndpoint?: string
  issuerUrl?: string
  scopes?: string
  profileArn?: string
  priority?: number
  authRegion?: string
  apiRegion?: string
  machineId?: string
  proxyUrl?: string
  proxyUsername?: string
  proxyPassword?: string
}

// 更新凭据请求
export interface UpdateCredentialRequest {
  refreshToken?: string
  authMethod?: string
  email?: string
  nickname?: string
  clientId?: string
  clientSecret?: string
  tokenEndpoint?: string
  issuerUrl?: string
  scopes?: string
  profileArn?: string
  authRegion?: string
  apiRegion?: string
  machineId?: string
  proxyUrl?: string
  proxyUsername?: string
  proxyPassword?: string
}

// 添加凭据响应
export interface AddCredentialResponse {
  success: boolean
  message: string
  credentialId: number
  email?: string
}

// API Key 类型
export interface ApiKeyItem {
  id: number
  key: string
  name: string
  enabled: boolean
  createdAt: string
  expiresAt: string | null
  spendingLimit: number | null
  creditLimit?: number | null
  durationDays: number | null
  activatedAt: string | null
  boundCredentialIds?: number[]
  /** 父卡密 ID（仅子卡密有值） */
  parentKeyId?: number | null
  /** 已结算额度（仅有子卡密的父卡密） */
  committedCredits?: number
}

export interface CreateApiKeyRequest {
  name: string
  expiresAt?: string | null
  spendingLimit?: number | null
  creditLimit?: number | null
  durationDays?: number | null
  boundCredentialIds?: number[] | null
}

export interface UpdateApiKeyRequest {
  name?: string
  enabled?: boolean
  expiresAt?: string | null
  spendingLimit?: number | null
  creditLimit?: number | null
  durationDays?: number | null
  boundCredentialIds?: number[] | null
}

// API Key 用量汇总
export interface UsageSummary {
  apiKeyId: number
  totalRequests: number
  totalInputTokens: number
  totalOutputTokens: number
  totalCost: number
  totalCredits?: number
  totalCreditsSaved?: number
  byModel: ModelUsage[]
}

export interface ModelUsage {
  model: string
  requests: number
  inputTokens: number
  outputTokens: number
  cost: number
  /** 真实 credits 消耗（后端已按 credits_used / k_ref 计算，直接展示） */
  credits: number
}

// RPM 实时监控
export interface RpmSnapshot {
  global: number
  byCredential: Record<string, number>
  byApiKey: Record<string, number>
  /** 全局 TPM（最近 60 秒处理的 token 总量，input+output） */
  tokensPerMin?: number
  tokensByCredential?: Record<string, number>
  tokensByApiKey?: Record<string, number>
  /** sticky 路由缓存命中/未命中（/rpm 一并返回） */
  stickyHits?: number
  stickyMisses?: number
}

// 首页概览（GET /api/admin/overview）
export interface OverviewTotals {
  totalRequests: number
  totalInputTokens: number
  totalOutputTokens: number
  totalCredits: number
  totalCreditsSaved: number
  totalCacheReadTokens: number
  totalCacheCreationTokens: number
}

export interface ApiKeyUsageBrief {
  apiKeyId: number
  requests: number
  inputTokens: number
  outputTokens: number
  credits: number
}

export interface OverviewResponse {
  allTime: OverviewTotals
  daily: DailySummary[]        // 近 30 天，按日期升序
  byModel: ModelUsage[]        // 全历史，按 credits 降序
  byApiKey: ApiKeyUsageBrief[] // 全历史，按 credits 降序
}

// 单条原始请求记录
export interface UsageRecord {
  model: string
  inputTokens: number
  outputTokens: number
  estimatedCost: number
  /** 计费用 credits（后端已计算，直接展示） */
  credits: number
  creditsUsed?: number
  creditsSaved?: number
  cacheReadInputTokens?: number
  cacheCreationInputTokens?: number
  createdAt: string
  credentialId?: number
  credentialLabel?: string
  clientIp?: string
}

// 分页原始记录响应
export interface UsageRecordsResponse {
  records: UsageRecord[]
  total: number
  page: number
  pageSize: number
  totalPages: number
}

// 每日用量汇总
export interface DailySummary {
  date: string          // "2026-05-21" CST
  totalRequests: number
  totalCost: number
  totalCredits: number
  totalCreditsSaved?: number
  totalInputTokens?: number
  totalOutputTokens?: number
}

// 单账号在某 CST 日期的用量汇总（后端 /credentials/:id/usage/today）
export interface CredentialDaySummary {
  date: string           // "2026-06-26" CST (UTC+8)
  credentialId: number
  totalRequests: number
  totalInputTokens: number
  totalOutputTokens: number
  totalCost: number
  totalCredits: number
  totalCreditsSaved?: number
}

// 单条限流日志记录
export interface ThrottleLogRecord {
  credentialId: number
  requestType: string
  statusCode: number
  responseBody: string
  createdAt: string
}

// 限流日志分页响应
export interface ThrottleLogsResponse {
  records: ThrottleLogRecord[]
  total: number
  page: number
  pageSize: number
  totalPages: number
}

// 单条失败日志记录
export interface FailureLogRecord {
  credentialId: number
  requestType: string
  statusCode: number
  responseBody: string
  createdAt: string
}

// 失败日志分页响应
export interface FailureLogsResponse {
  records: FailureLogRecord[]
  total: number
  page: number
  pageSize: number
  totalPages: number
}

// ============ 设备授权登录（SSO device login） ============

export interface StartDeviceLoginRequest {
  startUrl: string
  region?: string
  /** 账号名称/备注（可选），用于在列表中区分账号 */
  name?: string
}

export interface DeviceLoginStartResponse {
  sessionId: string
  /** 授权登录地址：用户在浏览器打开完成登录/授权 */
  authorizeUrl: string
  expiresIn: number
}

export interface DeviceLoginPollResponse {
  status: 'pending' | 'complete' | 'error'
  credentialId?: number
  message?: string
}
