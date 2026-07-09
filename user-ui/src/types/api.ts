// Copyright (c) 2026 Harllan He. Licensed under MIT.
export interface LoginRequest {
  apiKey: string
}

export interface LoginResponse {
  id: number
  name: string
  spendingLimit: number | null
  creditLimit: number | null
  totalCost: number
  totalCredits: number
  expiresAt: string | null
  durationDays: number | null
  activatedAt: string | null
  /** 是否为分销卡密 */
  isReseller: boolean
  /** 分销卡密可再分配额度（仅分销卡密有值） */
  allocatableCredits?: number | null
}

/** 单个子卡密视图 */
export interface SubKey {
  id: number
  key: string
  name: string
  enabled: boolean
  creditLimit: number | null
  usedCredits: number
  createdAt: string
  expiresAt: string | null
  durationDays: number | null
  activatedAt: string | null
}

/** 分销商概览 */
export interface ResellerOverview {
  id: number
  name: string
  budget: number | null
  allocated: number
  committed: number
  allocatable: number
  subKeyCount: number
  expiresAt: string | null
  subKeys: SubKey[]
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

export interface UsageResponse {
  id: number
  name: string
  spendingLimit: number | null
  creditLimit: number | null
  expiresAt: string | null
  durationDays: number | null
  activatedAt: string | null
  totalRequests: number
  totalInputTokens: number
  totalOutputTokens: number
  totalCost: number
  totalCredits: number
  byModel: ModelUsage[]
  /** 是否为分销卡密 */
  isReseller: boolean
  /** 分销卡密可再分配额度 */
  allocatableCredits?: number | null
}

export interface UsageRecordItem {
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
  clientIp?: string
  credentialLabel?: string
}

export interface UsageRecordsPage {
  records: UsageRecordItem[]
  total: number
  page: number
  pageSize: number
  totalPages: number
}
