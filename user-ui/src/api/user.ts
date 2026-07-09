// Copyright (c) 2026 Harllan He. Licensed under MIT.
import axios from 'axios'
import { storage } from '@/lib/storage'
import type {
  LoginRequest, LoginResponse, UsageResponse, UsageRecordsPage,
  ResellerOverview, SubKey,
} from '@/types/api'

const api = axios.create({
  baseURL: '/api/user',
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

// 登录验证
export async function login(apiKey: string): Promise<LoginResponse> {
  const { data } = await api.post<LoginResponse>('/login', { apiKey } as LoginRequest)
  return data
}

// 获取用量数据
export async function getUsage(): Promise<UsageResponse> {
  const { data } = await api.get<UsageResponse>('/usage')
  return data
}

// 获取分页请求日志
export async function getUsageRecords(page = 1, pageSize = 50): Promise<UsageRecordsPage> {
  const { data } = await api.get<UsageRecordsPage>('/usage/records', {
    params: { page, page_size: pageSize },
  })
  return data
}

// ==================== 分销商（reseller）====================

// 获取分销商概览（预算、可分配额度、子卡密列表）
export async function getResellerOverview(): Promise<ResellerOverview> {
  const { data } = await api.get<ResellerOverview>('/reseller/overview')
  return data
}

// 开子卡密
export async function createSubKey(payload: {
  name: string
  creditLimit: number
  durationDays?: number | null
}): Promise<SubKey> {
  const { data } = await api.post<SubKey>('/reseller/sub-keys', payload)
  return data
}

// 更新子卡密（名称/启用/额度）
export async function updateSubKey(
  id: number,
  payload: { name?: string; enabled?: boolean; creditLimit?: number },
): Promise<SubKey> {
  const { data } = await api.put<SubKey>(`/reseller/sub-keys/${id}`, payload)
  return data
}

// 子卡密续费（叠加额度/时长）
export async function topupSubKey(
  id: number,
  payload: { addCredits?: number; addDays?: number },
): Promise<SubKey> {
  const { data } = await api.post<SubKey>(`/reseller/sub-keys/${id}/topup`, payload)
  return data
}

// 删除子卡密
export async function deleteSubKey(id: number): Promise<void> {
  await api.delete(`/reseller/sub-keys/${id}`)
}
