// Copyright (c) 2026 Harllan He. Licensed under MIT.
import axios from 'axios'
import { storage } from '@/lib/storage'
import type {
  CredentialsStatusResponse,
  BalanceResponse,
  SuccessResponse,
  SetDisabledRequest,
  SetPriorityRequest,
  AddCredentialRequest,
  AddCredentialResponse,
  UpdateCredentialRequest,
  ApiKeyItem,
  CreateApiKeyRequest,
  UpdateApiKeyRequest,
  UsageSummary,
  RpmSnapshot,
  UsageRecordsResponse,
  RechargeRecordsResponse,
  DailySummary,
  OverviewResponse,
  CredentialDaySummary,
  ThrottleLogsResponse,
  FailureLogsResponse,
  StartDeviceLoginRequest,
  DeviceLoginStartResponse,
  DeviceLoginPollResponse,
  RelayItem,
  CreateRelayRequest,
  UpdateRelayRequest,
  RelayModelsResponse,
} from '@/types/api'

// 创建 axios 实例
const api = axios.create({
  baseURL: '/api/admin',
  headers: {
    'Content-Type': 'application/json',
  },
})

// 请求拦截器添加 API Key
api.interceptors.request.use((config) => {
  const apiKey = storage.getApiKey()
  if (apiKey) {
    config.headers['x-api-key'] = apiKey
  }
  return config
})

// 获取所有凭据状态
export async function getCredentials(): Promise<CredentialsStatusResponse> {
  const { data } = await api.get<CredentialsStatusResponse>('/credentials')
  return data
}

// 设置凭据禁用状态
export async function setCredentialDisabled(
  id: number,
  disabled: boolean
): Promise<SuccessResponse> {
  const { data } = await api.post<SuccessResponse>(
    `/credentials/${id}/disabled`,
    { disabled } as SetDisabledRequest
  )
  return data
}

// 设置凭据优先级
export async function setCredentialPriority(
  id: number,
  priority: number
): Promise<SuccessResponse> {
  const { data } = await api.post<SuccessResponse>(
    `/credentials/${id}/priority`,
    { priority } as SetPriorityRequest
  )
  return data
}

// 重置失败计数
export async function resetCredentialFailure(
  id: number
): Promise<SuccessResponse> {
  const { data } = await api.post<SuccessResponse>(`/credentials/${id}/reset`)
  return data
}

// 获取凭据余额
export async function getCredentialBalance(id: number): Promise<BalanceResponse> {
  const { data } = await api.get<BalanceResponse>(`/credentials/${id}/balance`)
  return data
}

// 添加新凭据
export async function addCredential(
  req: AddCredentialRequest
): Promise<AddCredentialResponse> {
  const { data } = await api.post<AddCredentialResponse>('/credentials', req)
  return data
}

// 删除凭据
export async function deleteCredential(id: number): Promise<SuccessResponse> {
  const { data } = await api.delete<SuccessResponse>(`/credentials/${id}`)
  return data
}

// 更新凭据
export async function updateCredential(id: number, req: UpdateCredentialRequest): Promise<SuccessResponse> {
  const { data } = await api.put<SuccessResponse>(`/credentials/${id}`, req)
  return data
}

// 发起设备授权登录（SSO）
export async function startDeviceLogin(
  req: StartDeviceLoginRequest
): Promise<DeviceLoginStartResponse> {
  const { data } = await api.post<DeviceLoginStartResponse>('/device-login/start', req)
  return data
}

// 完成授权码登录：提交粘贴回来的回调内容（含 code）换取 token
export async function pollDeviceLogin(
  sessionId: string,
  redirectResponse: string
): Promise<DeviceLoginPollResponse> {
  const { data } = await api.post<DeviceLoginPollResponse>('/device-login/poll', {
    sessionId,
    redirectResponse,
  })
  return data
}

// 发起 Social 登录（app.kiro.dev，支持 Google/GitHub/Microsoft 等，无需门户地址）
export async function startSocialLogin(
  req: { region?: string; name?: string }
): Promise<DeviceLoginStartResponse> {
  const { data } = await api.post<DeviceLoginStartResponse>('/social-login/start', req)
  return data
}

// 完成 Social 登录：提交粘贴回来的回调内容（含 code）换取 refreshToken
export async function pollSocialLogin(
  sessionId: string,
  redirectResponse: string
): Promise<DeviceLoginPollResponse> {
  const { data } = await api.post<DeviceLoginPollResponse>('/social-login/poll', {
    sessionId,
    redirectResponse,
  })
  return data
}

// 负载均衡模式：priority=优先级(粘住最高优先级账号) / balanced=全局轮询 / auto=优先级+同级负载均衡
export type LoadBalancingMode = 'priority' | 'balanced' | 'auto'

// 获取负载均衡模式
export async function getLoadBalancingMode(): Promise<{ mode: LoadBalancingMode }> {
  const { data } = await api.get<{ mode: LoadBalancingMode }>('/config/load-balancing')
  return data
}

// 设置负载均衡模式
export async function setLoadBalancingMode(mode: LoadBalancingMode): Promise<{ mode: LoadBalancingMode }> {
  const { data } = await api.put<{ mode: LoadBalancingMode }>('/config/load-balancing', { mode })
  return data
}

// ============ 服务器信息 ============

// 获取服务器连接信息
export async function getServerInfo(): Promise<{ masterApiKey: string | null; version: string }> {
  const { data } = await api.get<{ masterApiKey: string | null; version: string }>('/server-info')
  return data
}

// ============ API Key 管理 ============

// 获取所有 API Key
export async function getApiKeys(): Promise<ApiKeyItem[]> {
  const { data } = await api.get<ApiKeyItem[]>('/api-keys')
  return data
}

// 创建 API Key
export async function createApiKey(req: CreateApiKeyRequest): Promise<ApiKeyItem> {
  const { data } = await api.post<ApiKeyItem>('/api-keys', req)
  return data
}

// 更新 API Key
export async function updateApiKey(id: number, req: UpdateApiKeyRequest): Promise<ApiKeyItem> {
  const { data } = await api.put<ApiKeyItem>(`/api-keys/${id}`, req)
  return data
}

// 删除 API Key
export async function deleteApiKey(id: number): Promise<SuccessResponse> {
  const { data } = await api.delete<SuccessResponse>(`/api-keys/${id}`)
  return data
}

// 续费/充值 API Key（叠加时长或额度）
export async function topUpApiKey(
  id: number,
  req: { addDays?: number; addCredits?: number }
): Promise<ApiKeyItem> {
  const { data } = await api.post<ApiKeyItem>(`/api-keys/${id}/topup`, req)
  return data
}



// ============ API Key 用量 ============

// 获取所有 API Key 用量概览
export async function getAllUsage(): Promise<UsageSummary[]> {
  const { data } = await api.get<UsageSummary[]>('/api-keys/usage')
  return data
}

// 获取单个 API Key 用量
export async function getKeyUsage(id: number): Promise<UsageSummary> {
  const { data } = await api.get<UsageSummary>(`/api-keys/${id}/usage`)
  return data
}

// 重置单个 API Key 用量
export async function resetKeyUsage(id: number): Promise<SuccessResponse> {
  const { data } = await api.delete<SuccessResponse>(`/api-keys/${id}/usage`)
  return data
}

// 分页获取单个 API Key 的原始请求记录
export async function getKeyUsageRecords(
  id: number,
  page: number,
  pageSize: number
): Promise<UsageRecordsResponse> {
  const { data } = await api.get<UsageRecordsResponse>(
    `/api-keys/${id}/usage/records`,
    { params: { page, page_size: pageSize } }
  )
  return data
}

export async function getCredentialUsageRecords(
  id: number,
  page: number,
  pageSize: number
): Promise<UsageRecordsResponse> {
  const { data } = await api.get<UsageRecordsResponse>(
    `/credentials/${id}/usage/records`,
    { params: { page, page_size: pageSize } }
  )
  return data
}

// 分页获取单个 API Key 的充值/开卡流水
export async function getKeyRechargeRecords(
  id: number,
  page: number,
  pageSize: number
): Promise<RechargeRecordsResponse> {
  const { data } = await api.get<RechargeRecordsResponse>(
    `/api-keys/${id}/recharges`,
    { params: { page, page_size: pageSize } }
  )
  return data
}

// 获取单账号 CST 今日的用量汇总
export async function getCredentialTodaySummary(
  id: number
): Promise<CredentialDaySummary> {
  const { data } = await api.get<CredentialDaySummary>(
    `/credentials/${id}/usage/today`
  )
  return data
}

// ============ RPM 监控 ============

// 获取实时 RPM 数据
export async function getRpm(): Promise<RpmSnapshot> {
  const { data } = await api.get<RpmSnapshot>('/rpm')
  return data
}

// 获取首页概览聚合数据
export async function getOverview(): Promise<OverviewResponse> {
  const { data } = await api.get<OverviewResponse>('/overview')
  return data
}

// ============ 认证密钥管理 ============

export async function getAuthKeys(): Promise<{ apiKey: string; adminApiKey: string }> {
  const { data } = await api.get<{ apiKey: string; adminApiKey: string }>('/config/auth-keys')
  return data
}

export async function setAuthKeys(payload: { apiKey?: string; adminApiKey?: string }): Promise<{ success: boolean; message: string }> {
  const { data } = await api.put<{ success: boolean; message: string }>('/config/auth-keys', payload)
  return data
}

// ============ 每日用量统计 ============

export async function getDailyUsage(): Promise<DailySummary[]> {
  const { data } = await api.get<DailySummary[]>('/usage/daily')
  return data
}

export async function getDailyUsageRecords(
  date: string,
  page: number,
  pageSize: number
): Promise<UsageRecordsResponse> {
  const { data } = await api.get<UsageRecordsResponse>(
    `/usage/daily/${date}/records`,
    { params: { page, page_size: pageSize } }
  )
  return data
}

// ============ 失败日志 ============

export async function getFailureLogs(
  id: number,
  page: number,
  pageSize: number
): Promise<FailureLogsResponse> {
  const { data } = await api.get<FailureLogsResponse>(
    `/credentials/${id}/failure-logs`,
    { params: { page, page_size: pageSize } }
  )
  return data
}

// ============ 限流日志 ============

export async function getThrottleLogs(
  id: number,
  page: number,
  pageSize: number
): Promise<ThrottleLogsResponse> {
  const { data } = await api.get<ThrottleLogsResponse>(
    `/credentials/${id}/throttle-logs`,
    { params: { page, page_size: pageSize } }
  )
  return data
}

// ============ 中转对接（备用路由） ============

export async function getRelays(): Promise<RelayItem[]> {
  const { data } = await api.get<RelayItem[]>('/relays')
  return data
}

export async function createRelay(req: CreateRelayRequest): Promise<RelayItem> {
  const { data } = await api.post<RelayItem>('/relays', req)
  return data
}

export async function updateRelay(id: number, req: UpdateRelayRequest): Promise<RelayItem> {
  const { data } = await api.put<RelayItem>(`/relays/${id}`, req)
  return data
}

export async function deleteRelay(id: number): Promise<SuccessResponse> {
  const { data } = await api.delete<SuccessResponse>(`/relays/${id}`)
  return data
}

// 拉取（并缓存）该中转的模型列表
export async function fetchRelayModels(id: number): Promise<RelayModelsResponse> {
  const { data } = await api.post<RelayModelsResponse>(`/relays/${id}/models`)
  return data
}
