// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState } from 'react'
import { toast } from 'sonner'
import { FileJson } from 'lucide-react'
import { DialogFooter } from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { useAddCredential } from '@/hooks/use-credentials'
import { extractErrorMessage } from '@/lib/utils'
import type { AddCredentialRequest } from '@/types/api'

interface KiroFileImportPanelProps {
  onClose: () => void
}

interface Parsed {
  refreshToken: string
  authMethod: 'idc' | 'social'
  region: string
  clientId?: string
  clientSecret?: string
  portal?: string
}

/** 从 clientSecret(JWT)里尽力解出门户地址(initiateLoginUri)用于展示，失败返回 undefined。 */
function portalFromSecret(secret?: string): string | undefined {
  if (!secret) return undefined
  try {
    const payload = secret.split('.')[1]
    if (!payload) return undefined
    const json = JSON.parse(atob(payload.replace(/-/g, '+').replace(/_/g, '/')))
    const inner = typeof json.serialized === 'string' ? JSON.parse(json.serialized) : json
    const uri: string | undefined = inner.initiateLoginUri
    if (uri) return uri.replace(/^https?:\/\//, '').split('/')[0]
  } catch {
    /* ignore */
  }
  return undefined
}

/** 从任意两份(或一份)Kiro 导出 JSON 中识别出 token 文件与客户端注册文件，拼出可导入的凭据。 */
function parseKiroJsons(objs: unknown[]): Parsed | { error: string } {
  let token: Record<string, unknown> | null = null
  let reg: Record<string, unknown> | null = null
  for (const o of objs) {
    if (!o || typeof o !== 'object') continue
    const obj = o as Record<string, unknown>
    if (typeof obj.clientSecret === 'string' && typeof obj.clientId === 'string') reg = obj
    if (typeof obj.refreshToken === 'string') token = obj
  }
  if (!token || typeof token.refreshToken !== 'string' || !token.refreshToken) {
    return { error: '未找到包含 refreshToken 的 token 文件(kiro-auth-token*.json)' }
  }
  const authRaw = String(token.authMethod || '').toLowerCase()
  // 有客户端注册(clientSecret)或声明 IdC/Enterprise → 按 IdC；否则按 social
  const isIdc = !!reg || authRaw.includes('idc') || authRaw.includes('enterprise')
  const region = (typeof token.region === 'string' && token.region) || 'us-east-1'
  if (isIdc) {
    if (!reg) {
      return {
        error:
          '这是企业 IdC 账号，但缺少客户端注册文件(内容含 clientId + clientSecret，文件名通常是一串哈希.json)。请把两个 JSON 都选上。',
      }
    }
    return {
      refreshToken: token.refreshToken,
      authMethod: 'idc',
      region,
      clientId: reg.clientId as string,
      clientSecret: reg.clientSecret as string,
      portal: portalFromSecret(reg.clientSecret as string),
    }
  }
  return { refreshToken: token.refreshToken, authMethod: 'social', region }
}

/** Kiro 文件导入：拖/选两个 JSON(token + 客户端注册)，自动解析并作为账号导入。不看文件名。 */
export function KiroFileImportPanel({ onClose }: KiroFileImportPanelProps) {
  const [parsed, setParsed] = useState<Parsed | null>(null)
  const [parseErr, setParseErr] = useState<string | null>(null)
  const [name, setName] = useState('')
  const { mutate, isPending } = useAddCredential()

  const handleFiles = async (files: FileList | null) => {
    setParsed(null)
    setParseErr(null)
    if (!files || files.length === 0) return
    if (files.length > 2) {
      setParseErr('最多选择 2 个文件(token 文件 + 客户端注册文件)')
      return
    }
    try {
      const objs: unknown[] = []
      for (const f of Array.from(files)) {
        const text = await f.text()
        objs.push(JSON.parse(text))
      }
      const res = parseKiroJsons(objs)
      if ('error' in res) {
        setParseErr(res.error)
      } else {
        setParsed(res)
        // 默认名称：门户名或 clientId 前缀
        if (!name) {
          setName(res.portal || (res.clientId ? res.clientId.slice(0, 12) : 'kiro-account'))
        }
      }
    } catch (e) {
      setParseErr(`JSON 解析失败: ${e instanceof Error ? e.message : String(e)}`)
    }
  }

  const handleImport = () => {
    if (!parsed) return
    const req: AddCredentialRequest = {
      refreshToken: parsed.refreshToken,
      authMethod: parsed.authMethod,
      authRegion: parsed.region,
      apiRegion: parsed.region,
      nickname: name.trim() || undefined,
      email: name.trim() || undefined,
    }
    if (parsed.authMethod === 'idc') {
      req.clientId = parsed.clientId
      req.clientSecret = parsed.clientSecret
    }
    mutate(req, {
      onSuccess: (data) => {
        toast.success(data.message)
        onClose()
      },
      onError: (error: unknown) => {
        toast.error(`导入失败: ${extractErrorMessage(error)}`)
      },
    })
  }

  return (
    <div className="flex flex-col min-h-0 flex-1">
      <div className="space-y-4 py-4 overflow-y-auto flex-1 pr-1">
        <p className="text-sm text-muted-foreground">
          选择 Kiro 导出的 <b>两个 JSON</b>(token 文件 + 客户端注册文件，<b>不用管文件名</b>，也不用管先后)。
          企业 IdC 账号两个都要选；纯 social 账号只有一个 token 文件也行。
        </p>

        <label
          className="flex flex-col items-center justify-center gap-2 rounded-lg border-2 border-dashed border-input px-4 py-8 text-center cursor-pointer hover:border-primary/50 transition-colors"
        >
          <FileJson className="h-7 w-7 text-muted-foreground" />
          <span className="text-sm font-medium">点击选择 JSON 文件(最多 2 个)</span>
          <span className="text-xs text-muted-foreground">kiro-auth-token*.json 和 那份哈希.json</span>
          <input
            type="file"
            accept=".json,application/json"
            multiple
            className="hidden"
            onChange={(e) => handleFiles(e.target.files)}
          />
        </label>

        {parseErr && (
          <div className="rounded-md border border-red-500/30 bg-red-500/5 px-3 py-2 text-xs text-red-600 dark:text-red-400">
            {parseErr}
          </div>
        )}

        {parsed && (
          <div className="rounded-md border border-green-500/30 bg-green-500/5 px-3 py-2.5 text-xs space-y-1">
            <div className="flex items-center gap-2 text-green-600 dark:text-green-400 font-medium">
              解析成功，可导入
            </div>
            <div className="text-muted-foreground space-y-0.5 font-mono">
              <div>类型: {parsed.authMethod === 'idc' ? '企业 IdC' : 'Social'}</div>
              <div>Region: {parsed.region}</div>
              {parsed.portal && <div>门户: {parsed.portal}</div>}
              {parsed.clientId && <div>clientId: {parsed.clientId.slice(0, 10)}…</div>}
              <div>refreshToken: {parsed.refreshToken.slice(0, 12)}…(已就绪)</div>
            </div>
          </div>
        )}

        {parsed && (
          <div className="space-y-2">
            <label htmlFor="kfName" className="text-sm font-medium">账号名称</label>
            <Input
              id="kfName"
              placeholder="用于在列表里区分"
              value={name}
              onChange={(e) => setName(e.target.value)}
              disabled={isPending}
            />
          </div>
        )}
      </div>

      <DialogFooter>
        <Button type="button" variant="outline" onClick={onClose} disabled={isPending}>
          取消
        </Button>
        <Button type="button" onClick={handleImport} disabled={!parsed || isPending}>
          {isPending ? '导入中…' : '导入'}
        </Button>
      </DialogFooter>
    </div>
  )
}
