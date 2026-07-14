// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useMemo, useState } from 'react'
import { toast } from 'sonner'
import { CheckCircle2, XCircle, AlertCircle, Loader2 } from 'lucide-react'
import { DialogFooter } from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { useCredentials, useAddCredential } from '@/hooks/use-credentials'
import { getCredentialBalance } from '@/api/credentials'
import { extractErrorMessage } from '@/lib/utils'

interface ApiKeyPanelProps {
  onClose: () => void
}

interface KeyResult {
  index: number
  masked: string
  status: 'pending' | 'adding' | 'verifying' | 'added' | 'duplicate' | 'failed'
  error?: string
  usage?: string
}

async function sha256Hex(value: string): Promise<string> {
  const encoded = new TextEncoder().encode(value)
  const digest = await crypto.subtle.digest('SHA-256', encoded)
  return Array.from(new Uint8Array(digest))
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('')
}

const maskKey = (k: string) => (k.length > 12 ? `${k.slice(0, 8)}…${k.slice(-4)}` : k)

/** 批量添加 Kiro API Key（ksk_，每行一个），逐个添加并验活。 */
export function ApiKeyPanel({ onClose }: ApiKeyPanelProps) {
  const [apiKeys, setApiKeys] = useState('')
  const [nickname, setNickname] = useState('')
  const [priority, setPriority] = useState('0')
  const [adding, setAdding] = useState(false)
  const [results, setResults] = useState<KeyResult[]>([])

  const { data: existingCredentials } = useCredentials()
  const { mutateAsync: addCredential } = useAddCredential()

  const parsedKeys = useMemo(
    () =>
      apiKeys
        .split(/[\r\n]+/)
        .map((k) => k.trim())
        .filter((k) => k.length > 0),
    [apiKeys]
  )

  const handleImport = async () => {
    if (parsedKeys.length === 0) {
      toast.error('请至少输入一个 Kiro API Key（ksk_ 开头），每行一个')
      return
    }

    setAdding(true)
    const initial: KeyResult[] = parsedKeys.map((k, i) => ({
      index: i + 1,
      masked: maskKey(k),
      status: 'pending',
    }))
    setResults(initial)

    // 已存在 API Key 的哈希集合（本地去重，避免无谓请求）
    const existingHashes = new Set(
      existingCredentials?.credentials
        .map((c) => c.apiKeyHash)
        .filter((h): h is string => Boolean(h)) || []
    )

    const basePriority = parseInt(priority) || 0
    let added = 0
    let duplicate = 0
    let failed = 0
    // 记录失败的 key：导入完成后只把失败的留在输入框以便重试；已成功/重复的从输入框移除，
    // 避免再次点击"添加"时，刚成功添加的 key（此时已进入账号列表）被本地去重误判为"重复"。
    const failedKeys: string[] = []

    for (let i = 0; i < parsedKeys.length; i++) {
      const key = parsedKeys[i]
      const hash = await sha256Hex(key)

      if (existingHashes.has(hash)) {
        duplicate++
        setResults((prev) => {
          const next = [...prev]
          next[i] = { ...next[i], status: 'duplicate', error: '该 API Key 已存在' }
          return next
        })
        continue
      }

      setResults((prev) => {
        const next = [...prev]
        next[i] = { ...next[i], status: 'adding' }
        return next
      })

      try {
        // 昵称/用户名默认用 key 本身（便于在列表里区分每个账号）；填了备注名则用备注名
        const label = nickname.trim() || key
        const res = await addCredential({
          kiroApiKey: key,
          authMethod: 'api_key',
          email: label,
          nickname: label,
          priority: basePriority,
        })

        // 验活：查一次额度
        setResults((prev) => {
          const next = [...prev]
          next[i] = { ...next[i], status: 'verifying' }
          return next
        })
        let usage: string | undefined
        try {
          const balance = await getCredentialBalance(res.credentialId)
          usage = `${balance.currentUsage}/${balance.usageLimit}`
        } catch {
          // 验活失败不影响添加成功，仅不显示用量
        }

        added++
        existingHashes.add(hash)
        setResults((prev) => {
          const next = [...prev]
          next[i] = { ...next[i], status: 'added', usage }
          return next
        })
      } catch (error: unknown) {
        failed++
        failedKeys.push(key)
        setResults((prev) => {
          const next = [...prev]
          next[i] = { ...next[i], status: 'failed', error: extractErrorMessage(error) }
          return next
        })
      }
    }

    setAdding(false)
    // 只保留失败的 key（成功/重复的已处理完毕，从输入框移除）：这样再次点击"添加"时不会把
    // 刚加成功的 key 又当成"重复"，也方便对失败项直接重试。
    setApiKeys(failedKeys.join('\n'))

    const parts: string[] = []
    if (added > 0) parts.push(`成功 ${added}`)
    if (duplicate > 0) parts.push(`重复 ${duplicate}`)
    if (failed > 0) parts.push(`失败 ${failed}`)
    if (failed === 0 && duplicate === 0) {
      toast.success(`成功添加 ${added} 个 API Key 账号`)
    } else {
      toast.info(`导入完成：${parts.join('，')}`)
    }
  }

  const statusIcon = (s: KeyResult['status']) => {
    switch (s) {
      case 'pending':
        return <div className="w-5 h-5 rounded-full border-2 border-gray-300" />
      case 'adding':
      case 'verifying':
        return <Loader2 className="w-5 h-5 animate-spin text-blue-500" />
      case 'added':
        return <CheckCircle2 className="w-5 h-5 text-green-500" />
      case 'duplicate':
        return <AlertCircle className="w-5 h-5 text-yellow-500" />
      case 'failed':
        return <XCircle className="w-5 h-5 text-red-500" />
    }
  }

  const statusText = (r: KeyResult) => {
    switch (r.status) {
      case 'pending':
        return '等待中'
      case 'adding':
        return '添加中...'
      case 'verifying':
        return '验活中...'
      case 'added':
        return '添加成功'
      case 'duplicate':
        return '重复'
      case 'failed':
        return '失败'
    }
  }

  return (
    <div className="flex flex-col min-h-0 flex-1">
      <div className="space-y-4 py-4 overflow-y-auto flex-1 pr-1">
        <div className="space-y-2">
          <label htmlFor="apiKeys" className="text-sm font-medium">
            Kiro API Key <span className="text-red-500">*</span>
            <span className="text-muted-foreground text-xs">（每行一个，支持批量粘贴）</span>
          </label>
          <textarea
            id="apiKeys"
            placeholder={'ksk_xxxxxxxxxxxx\nksk_yyyyyyyyyyyy\nksk_zzzzzzzzzzzz'}
            value={apiKeys}
            onChange={(e) => setApiKeys(e.target.value)}
            disabled={adding}
            rows={6}
            className="flex w-full rounded-md border border-input bg-background px-3 py-2 text-sm font-mono ring-offset-background placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
          />
          <p className="text-xs text-muted-foreground">
            ksk_ 开头的 Kiro API Key，直接作为凭据使用、无需刷新。每行一个可一次添加多个，添加后自动验活。
            {parsedKeys.length > 0 && ` 已识别 ${parsedKeys.length} 个。`}
          </p>
        </div>

        <div className="grid grid-cols-2 gap-2">
          <div className="space-y-2">
            <label htmlFor="apiKeyNickname" className="text-sm font-medium">
              备注名（可选，留空则用 key 本身）
            </label>
            <Input
              id="apiKeyNickname"
              placeholder="留空则昵称/用户名都用 key"
              value={nickname}
              onChange={(e) => setNickname(e.target.value)}
              disabled={adding}
            />
          </div>
          <div className="space-y-2">
            <label htmlFor="apiKeyPriority" className="text-sm font-medium">
              优先级
            </label>
            <Input
              id="apiKeyPriority"
              type="number"
              min="0"
              value={priority}
              onChange={(e) => setPriority(e.target.value)}
              disabled={adding}
            />
          </div>
        </div>

        {results.length > 0 && (
          <>
            <div className="flex gap-4 text-sm">
              <span className="text-green-600 dark:text-green-400">
                ✓ 成功: {results.filter((r) => r.status === 'added').length}
              </span>
              <span className="text-yellow-600 dark:text-yellow-400">
                ⚠ 重复: {results.filter((r) => r.status === 'duplicate').length}
              </span>
              <span className="text-red-600 dark:text-red-400">
                ✗ 失败: {results.filter((r) => r.status === 'failed').length}
              </span>
            </div>
            <div className="border rounded-md divide-y max-h-[240px] overflow-y-auto">
              {results.map((r) => (
                <div key={r.index} className="p-3 flex items-start gap-3">
                  {statusIcon(r.status)}
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2">
                      <span className="text-sm font-mono">{r.masked}</span>
                      <span className="text-xs text-muted-foreground">{statusText(r)}</span>
                    </div>
                    {r.usage && (
                      <div className="text-xs text-muted-foreground mt-1">用量: {r.usage}</div>
                    )}
                    {r.error && (
                      <div className="text-xs text-red-600 dark:text-red-400 mt-1">{r.error}</div>
                    )}
                  </div>
                </div>
              ))}
            </div>
          </>
        )}
      </div>

      <DialogFooter>
        <Button type="button" variant="outline" onClick={onClose} disabled={adding}>
          {results.length > 0 && !adding ? '关闭' : '取消'}
        </Button>
        <Button
          type="button"
          onClick={handleImport}
          disabled={adding || parsedKeys.length === 0}
        >
          {adding ? '添加中...' : `添加${parsedKeys.length > 0 ? ` (${parsedKeys.length})` : ''}`}
        </Button>
      </DialogFooter>
    </div>
  )
}
