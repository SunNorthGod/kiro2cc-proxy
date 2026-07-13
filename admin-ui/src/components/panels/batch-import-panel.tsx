// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState, useMemo } from 'react'
import { toast } from 'sonner'
import { CheckCircle2, XCircle, AlertCircle, Loader2 } from 'lucide-react'
import { DialogFooter } from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { useCredentials, useAddCredential, useDeleteCredential } from '@/hooks/use-credentials'
import { getCredentialBalance, setCredentialDisabled } from '@/api/credentials'
import { extractErrorMessage } from '@/lib/utils'

interface BatchImportPanelProps {
  onClose: () => void
}

interface KamAccount {
  email?: string
  userId?: string | null
  nickname?: string
  provider?: string
  credentials: {
    refreshToken: string
    clientId?: string
    clientSecret?: string
    region?: string
    authMethod?: string
    startUrl?: string
    tokenEndpoint?: string
    issuerUrl?: string
    scopes?: string
  }
  machineId?: string
  status?: string
  profileArn?: string
  apiRegion?: string
  authRegion?: string
  priority?: number
}

interface VerificationResult {
  index: number
  status: 'pending' | 'checking' | 'verifying' | 'verified' | 'duplicate' | 'failed' | 'skipped'
  error?: string
  usage?: string
  email?: string
  credentialId?: number
  rollbackStatus?: 'success' | 'failed' | 'skipped'
  rollbackError?: string
}

async function sha256Hex(value: string): Promise<string> {
  const encoded = new TextEncoder().encode(value)
  const digest = await crypto.subtle.digest('SHA-256', encoded)
  return Array.from(new Uint8Array(digest))
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('')
}

function isValidKamAccount(item: unknown): item is KamAccount {
  if (typeof item !== 'object' || item === null) return false
  const obj = item as Record<string, unknown>
  if (typeof obj.credentials !== 'object' || obj.credentials === null) return false
  const cred = obj.credentials as Record<string, unknown>
  return typeof cred.refreshToken === 'string' && cred.refreshToken.trim().length > 0
}

function normalizeToKamAccount(item: unknown): unknown {
  if (typeof item !== 'object' || item === null) return item
  const obj = item as Record<string, unknown>
  if (typeof obj.credentials === 'object' && obj.credentials !== null) return item
  if (typeof obj.refreshToken === 'string' && obj.refreshToken.trim().length > 0) {
    const { refreshToken, clientId, clientSecret, region, authMethod, startUrl, tokenEndpoint, issuerUrl, scopes, ...rest } = obj
    return {
      ...rest,
      credentials: { refreshToken, clientId, clientSecret, region, authMethod, startUrl, tokenEndpoint, issuerUrl, scopes },
    }
  }
  return item
}

function parseKamJson(raw: string): KamAccount[] {
  const parsed = JSON.parse(raw)
  let rawItems: unknown[]

  if (parsed.accounts && Array.isArray(parsed.accounts)) {
    rawItems = parsed.accounts
  } else if (Array.isArray(parsed)) {
    rawItems = parsed
  } else if (parsed.credentials && typeof parsed.credentials === 'object') {
    rawItems = [parsed]
  } else if (typeof parsed.refreshToken === 'string') {
    rawItems = [parsed]
  } else {
    throw new Error('无法识别的 JSON 格式')
  }

  rawItems = rawItems.map(normalizeToKamAccount)
  const validAccounts = rawItems.filter(isValidKamAccount)

  if (rawItems.length > 0 && validAccounts.length === 0) {
    throw new Error(`共 ${rawItems.length} 条记录，但均缺少有效的 credentials.refreshToken`)
  }
  return validAccounts
}

/** 批量导入账号（KAM / kiro-go / 本地登录 JSON），自动识别 + 逐个验活 + 失败回滚。 */
export function BatchImportPanel({ onClose }: BatchImportPanelProps) {
  const [jsonInput, setJsonInput] = useState('')
  const [importing, setImporting] = useState(false)
  const [skipErrorAccounts, setSkipErrorAccounts] = useState(true)
  const [progress, setProgress] = useState({ current: 0, total: 0 })
  const [currentProcessing, setCurrentProcessing] = useState<string>('')
  const [results, setResults] = useState<VerificationResult[]>([])

  const { data: existingCredentials } = useCredentials()
  const { mutateAsync: addCredential } = useAddCredential()
  const { mutateAsync: deleteCredential } = useDeleteCredential()

  const rollbackCredential = async (id: number): Promise<{ success: boolean; error?: string }> => {
    try {
      await setCredentialDisabled(id, true)
    } catch (error) {
      return { success: false, error: `禁用失败: ${extractErrorMessage(error)}` }
    }
    try {
      await deleteCredential(id)
      return { success: true }
    } catch (error) {
      return { success: false, error: `删除失败: ${extractErrorMessage(error)}` }
    }
  }

  const handleImport = async () => {
    try {
      const accounts = parseKamJson(jsonInput)
      const validAccounts = accounts.filter((a) => a.credentials?.refreshToken)
      if (validAccounts.length === 0) {
        toast.error('没有包含有效 refreshToken 的账号')
        return
      }

      setImporting(true)
      setProgress({ current: 0, total: validAccounts.length })

      const initialResults: VerificationResult[] = validAccounts.map((account, i) => {
        if (skipErrorAccounts && account.status === 'error') {
          return { index: i + 1, status: 'skipped' as const, email: account.email || account.nickname }
        }
        return { index: i + 1, status: 'pending' as const, email: account.email || account.nickname }
      })
      setResults(initialResults)

      const existingTokenHashes = new Set(
        existingCredentials?.credentials
          .map((c) => c.refreshTokenHash)
          .filter((hash): hash is string => Boolean(hash)) || []
      )

      let successCount = 0
      let duplicateCount = 0
      let failCount = 0
      let skippedCount = 0

      for (let i = 0; i < validAccounts.length; i++) {
        const account = validAccounts[i]

        if (skipErrorAccounts && account.status === 'error') {
          skippedCount++
          setProgress({ current: i + 1, total: validAccounts.length })
          continue
        }

        const cred = account.credentials
        const token = cred.refreshToken.trim()
        const tokenHash = await sha256Hex(token)

        setCurrentProcessing(`正在处理 ${account.email || account.nickname || `账号 ${i + 1}`}`)
        setResults((prev) => {
          const next = [...prev]
          next[i] = { ...next[i], status: 'checking' }
          return next
        })

        if (existingTokenHashes.has(tokenHash)) {
          duplicateCount++
          const existingCred = existingCredentials?.credentials.find((c) => c.refreshTokenHash === tokenHash)
          setResults((prev) => {
            const next = [...prev]
            next[i] = { ...next[i], status: 'duplicate', error: '该账号已存在', email: existingCred?.email || account.email }
            return next
          })
          setProgress({ current: i + 1, total: validAccounts.length })
          continue
        }

        setResults((prev) => {
          const next = [...prev]
          next[i] = { ...next[i], status: 'verifying' }
          return next
        })

        let addedCredId: number | null = null

        try {
          const clientId = cred.clientId?.trim() || undefined
          const clientSecret = cred.clientSecret?.trim() || undefined
          const tokenEndpoint = cred.tokenEndpoint?.trim() || undefined
          const issuerUrl = cred.issuerUrl?.trim() || undefined
          const scopes = cred.scopes?.trim() || undefined
          const rawAuth = (cred.authMethod || '').toLowerCase()
          const providerStr = (account.provider || '').toLowerCase()
          const isExternalIdp =
            rawAuth === 'external_idp' || providerStr === 'externalidp' || !!tokenEndpoint || !!issuerUrl

          let authMethod: 'social' | 'idc' | 'external_idp'
          if (isExternalIdp) {
            authMethod = 'external_idp'
          } else if (clientId && clientSecret) {
            authMethod = 'idc'
          } else {
            authMethod = 'social'
          }

          if (authMethod === 'external_idp' && (!clientId || (!tokenEndpoint && !issuerUrl))) {
            throw new Error('external_idp 需要 clientId 和 tokenEndpoint（或 issuerUrl）')
          }
          if (authMethod === 'social' && (clientId || clientSecret)) {
            throw new Error('idc 模式需要同时提供 clientId 和 clientSecret')
          }

          const acctRegion = account.apiRegion?.trim() || account.authRegion?.trim() || cred.region?.trim() || undefined
          const authRegion = account.authRegion?.trim() || cred.region?.trim() || acctRegion
          const apiRegion = account.apiRegion?.trim() || acctRegion
          const profileArn = account.profileArn?.trim() || undefined
          const priority = typeof account.priority === 'number' ? account.priority : undefined

          const addedCred = await addCredential({
            refreshToken: token,
            authMethod,
            email: account.email || account.nickname || undefined,
            authRegion,
            apiRegion,
            profileArn,
            priority,
            clientId,
            clientSecret,
            tokenEndpoint,
            issuerUrl,
            scopes,
            machineId: account.machineId?.trim() || undefined,
          })

          addedCredId = addedCred.credentialId
          await new Promise((resolve) => setTimeout(resolve, 1000))
          const balance = await getCredentialBalance(addedCred.credentialId)

          successCount++
          existingTokenHashes.add(tokenHash)
          setCurrentProcessing(`验活成功: ${addedCred.email || account.email || `账号 ${i + 1}`}`)
          setResults((prev) => {
            const next = [...prev]
            next[i] = {
              ...next[i],
              status: 'verified',
              usage: `${balance.currentUsage}/${balance.usageLimit}`,
              email: addedCred.email || account.email,
              credentialId: addedCred.credentialId,
            }
            return next
          })
        } catch (error) {
          let rollbackStatus: VerificationResult['rollbackStatus'] = 'skipped'
          let rollbackError: string | undefined

          if (addedCredId) {
            const result = await rollbackCredential(addedCredId)
            if (result.success) {
              rollbackStatus = 'success'
            } else {
              rollbackStatus = 'failed'
              rollbackError = result.error
            }
          }

          failCount++
          setResults((prev) => {
            const next = [...prev]
            next[i] = { ...next[i], status: 'failed', error: extractErrorMessage(error), rollbackStatus, rollbackError }
            return next
          })
        }

        setProgress({ current: i + 1, total: validAccounts.length })
      }

      const parts: string[] = []
      if (successCount > 0) parts.push(`成功 ${successCount}`)
      if (duplicateCount > 0) parts.push(`重复 ${duplicateCount}`)
      if (failCount > 0) parts.push(`失败 ${failCount}`)
      if (skippedCount > 0) parts.push(`跳过 ${skippedCount}`)

      if (failCount === 0 && duplicateCount === 0 && skippedCount === 0) {
        toast.success(`成功导入并验活 ${successCount} 个账号`)
      } else {
        toast.info(`导入完成：${parts.join('，')}`)
      }
    } catch (error) {
      toast.error('JSON 格式错误: ' + extractErrorMessage(error))
    } finally {
      setImporting(false)
    }
  }

  const getStatusIcon = (status: VerificationResult['status']) => {
    switch (status) {
      case 'pending':
        return <div className="w-5 h-5 rounded-full border-2 border-gray-300" />
      case 'checking':
      case 'verifying':
        return <Loader2 className="w-5 h-5 animate-spin text-blue-500" />
      case 'verified':
        return <CheckCircle2 className="w-5 h-5 text-green-500" />
      case 'duplicate':
        return <AlertCircle className="w-5 h-5 text-yellow-500" />
      case 'skipped':
        return <AlertCircle className="w-5 h-5 text-gray-400" />
      case 'failed':
        return <XCircle className="w-5 h-5 text-red-500" />
    }
  }

  const getStatusText = (result: VerificationResult) => {
    switch (result.status) {
      case 'pending':
        return '等待中'
      case 'checking':
        return '检查重复...'
      case 'verifying':
        return '验活中...'
      case 'verified':
        return '验活成功'
      case 'duplicate':
        return '重复账号'
      case 'skipped':
        return '已跳过（error 状态）'
      case 'failed':
        if (result.rollbackStatus === 'success') return '验活失败（已排除）'
        if (result.rollbackStatus === 'failed') return '验活失败（未排除）'
        return '验活失败（未创建）'
    }
  }

  const { previewAccounts, parseError } = useMemo(() => {
    if (!jsonInput.trim()) return { previewAccounts: [] as KamAccount[], parseError: '' }
    try {
      return { previewAccounts: parseKamJson(jsonInput), parseError: '' }
    } catch (e) {
      return { previewAccounts: [] as KamAccount[], parseError: extractErrorMessage(e) }
    }
  }, [jsonInput])

  const errorAccountCount = previewAccounts.filter((a) => a.status === 'error').length

  return (
    <div className="flex flex-col min-h-0 flex-1">
      <div className="flex-1 overflow-y-auto space-y-4 py-4 pr-1">
        <div className="space-y-2">
          <label className="text-sm font-medium">导入 JSON（KAM 导出 / kiro-go 批量配置 / 本地 kiro-auth-token.json）</label>
          <textarea
            placeholder={'粘贴任意格式：KAM 导出 JSON、kiro-go 批量配置（accounts 数组）、账号数组，或本地 ~/.aws/sso/cache/kiro-auth-token.json 全文（自动识别 external_idp / idc / social），也可拖入 .json 文件'}
            value={jsonInput}
            onChange={(e) => setJsonInput(e.target.value)}
            onDragOver={(e) => {
              e.preventDefault()
              e.stopPropagation()
            }}
            onDrop={(e) => {
              e.preventDefault()
              e.stopPropagation()
              const file = e.dataTransfer.files[0]
              if (file) {
                const reader = new FileReader()
                reader.onload = (ev) => {
                  const text = ev.target?.result
                  if (typeof text === 'string') setJsonInput(text)
                }
                reader.readAsText(file)
              }
            }}
            disabled={importing}
            className="flex min-h-[180px] w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50 font-mono"
          />
          <p className="text-xs text-muted-foreground">
            支持 KAM 批量导出、kiro-go 批量配置（accounts 数组，含 profileArn / region）、账号数组，以及单个本地 kiro-auth-token.json（含微软 Entra external_idp）。IdC/BuilderId 建议改用「SSO 登录」，本地 refreshToken 可能已被截断失效。
          </p>
        </div>

        {parseError && <div className="text-sm text-red-600 dark:text-red-400">解析失败: {parseError}</div>}
        {previewAccounts.length > 0 && !importing && results.length === 0 && (
          <div className="space-y-2">
            <div className="text-sm text-muted-foreground">
              识别到 {previewAccounts.length} 个账号
              {errorAccountCount > 0 && `（其中 ${errorAccountCount} 个为 error 状态）`}
            </div>
            {errorAccountCount > 0 && (
              <label className="flex items-center gap-2 text-sm">
                <input
                  type="checkbox"
                  checked={skipErrorAccounts}
                  onChange={(e) => setSkipErrorAccounts(e.target.checked)}
                  className="rounded border-gray-300"
                />
                跳过 error 状态的账号
              </label>
            )}
          </div>
        )}

        {(importing || results.length > 0) && (
          <>
            <div className="space-y-2">
              <div className="flex justify-between text-sm">
                <span>{importing ? '导入进度' : '导入完成'}</span>
                <span>
                  {progress.current} / {progress.total}
                </span>
              </div>
              <div className="w-full bg-secondary rounded-full h-2">
                <div
                  className="bg-primary h-2 rounded-full transition-all"
                  style={{ width: `${progress.total > 0 ? (progress.current / progress.total) * 100 : 0}%` }}
                />
              </div>
              {importing && currentProcessing && <div className="text-xs text-muted-foreground">{currentProcessing}</div>}
            </div>

            <div className="flex gap-4 text-sm">
              <span className="text-green-600 dark:text-green-400">✓ 成功: {results.filter((r) => r.status === 'verified').length}</span>
              <span className="text-yellow-600 dark:text-yellow-400">⚠ 重复: {results.filter((r) => r.status === 'duplicate').length}</span>
              <span className="text-red-600 dark:text-red-400">✗ 失败: {results.filter((r) => r.status === 'failed').length}</span>
              <span className="text-gray-500">○ 跳过: {results.filter((r) => r.status === 'skipped').length}</span>
            </div>

            <div className="border rounded-md divide-y max-h-[260px] overflow-y-auto">
              {results.map((result) => (
                <div key={result.index} className="p-3">
                  <div className="flex items-start gap-3">
                    {getStatusIcon(result.status)}
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2">
                        <span className="text-sm font-medium">{result.email || `账号 #${result.index}`}</span>
                        <span className="text-xs text-muted-foreground">{getStatusText(result)}</span>
                      </div>
                      {result.usage && <div className="text-xs text-muted-foreground mt-1">用量: {result.usage}</div>}
                      {result.error && <div className="text-xs text-red-600 dark:text-red-400 mt-1">{result.error}</div>}
                      {result.rollbackError && <div className="text-xs text-red-600 dark:text-red-400 mt-1">回滚失败: {result.rollbackError}</div>}
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </>
        )}
      </div>

      <DialogFooter>
        <Button type="button" variant="outline" onClick={onClose} disabled={importing}>
          {importing ? '导入中...' : results.length > 0 ? '关闭' : '取消'}
        </Button>
        {results.length === 0 && (
          <Button
            type="button"
            onClick={handleImport}
            disabled={importing || !jsonInput.trim() || previewAccounts.length === 0 || !!parseError}
          >
            开始导入并验活
          </Button>
        )}
      </DialogFooter>
    </div>
  )
}
