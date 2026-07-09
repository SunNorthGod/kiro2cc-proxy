// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState } from 'react'
import { toast } from 'sonner'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { useAddCredential } from '@/hooks/use-credentials'
import { extractErrorMessage } from '@/lib/utils'

interface AddCredentialDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

type AuthMethod = 'social' | 'idc' | 'external_idp'

const inputClass =
  'flex w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50'

export function AddCredentialDialog({ open, onOpenChange }: AddCredentialDialogProps) {
  const [refreshToken, setRefreshToken] = useState('')
  const [email, setEmail] = useState('')
  const [authMethod, setAuthMethod] = useState<AuthMethod>('social')
  const [authRegion, setAuthRegion] = useState('')
  const [apiRegion, setApiRegion] = useState('')
  const [clientId, setClientId] = useState('')
  const [clientSecret, setClientSecret] = useState('')
  const [tokenEndpoint, setTokenEndpoint] = useState('')
  const [issuerUrl, setIssuerUrl] = useState('')
  const [scopes, setScopes] = useState('')
  const [profileArn, setProfileArn] = useState('')
  const [priority, setPriority] = useState('0')
  const [machineId, setMachineId] = useState('')
  const [proxyUrl, setProxyUrl] = useState('')
  const [proxyUsername, setProxyUsername] = useState('')
  const [proxyPassword, setProxyPassword] = useState('')
  const [importText, setImportText] = useState('')

  const { mutate, isPending } = useAddCredential()

  const resetForm = () => {
    setRefreshToken('')
    setEmail('')
    setAuthMethod('social')
    setAuthRegion('')
    setApiRegion('')
    setClientId('')
    setClientSecret('')
    setTokenEndpoint('')
    setIssuerUrl('')
    setScopes('')
    setProfileArn('')
    setPriority('0')
    setMachineId('')
    setProxyUrl('')
    setProxyUsername('')
    setProxyPassword('')
    setImportText('')
  }

  // 从本地 Kiro 登录信息（~/.aws/sso/cache/kiro-auth-token.json）粘贴导入，自动识别并填充字段。
  // 主要面向 external_idp（微软 Entra 等企业版）——其本地 refreshToken 是完整的、可直接刷新。
  const handleImportJson = (raw: string) => {
    const text = raw.trim()
    if (!text) {
      toast.error('请先粘贴 kiro-auth-token.json 内容')
      return
    }
    let obj: Record<string, unknown>
    try {
      obj = JSON.parse(text)
    } catch {
      toast.error('粘贴的内容不是有效的 JSON')
      return
    }
    const str = (v: unknown) => (typeof v === 'string' && v ? v : '')
    if (str(obj.refreshToken)) setRefreshToken(str(obj.refreshToken))
    const am = str(obj.authMethod).toLowerCase()
    const provider = str(obj.provider).toLowerCase()
    if (am === 'external_idp' || provider === 'externalidp') {
      setAuthMethod('external_idp')
    } else if (am === 'idc' || provider === 'builderid' || provider === 'enterprise') {
      setAuthMethod('idc')
    } else if (am === 'social') {
      setAuthMethod('social')
    }
    if (str(obj.clientId)) setClientId(str(obj.clientId))
    if (str(obj.tokenEndpoint)) setTokenEndpoint(str(obj.tokenEndpoint))
    if (str(obj.issuerUrl)) setIssuerUrl(str(obj.issuerUrl))
    if (str(obj.scopes)) setScopes(str(obj.scopes))
    if (str(obj.profileArn)) setProfileArn(str(obj.profileArn))
    if (str(obj.region)) setApiRegion(str(obj.region))
    toast.success('已自动填充，请核对后点击"添加"')
  }

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()

    // 验证必填字段
    if (!refreshToken.trim()) {
      toast.error('请输入 Refresh Token')
      return
    }

    // IdC/Builder-ID/IAM 需要额外字段
    if (authMethod === 'idc' && (!clientId.trim() || !clientSecret.trim())) {
      toast.error('IdC/Builder-ID/IAM 认证需要填写 Client ID 和 Client Secret')
      return
    }

    // External IdP（企业版/微软等）需要 Client ID + Token Endpoint（或 Issuer URL 用于自动发现）
    if (
      authMethod === 'external_idp' &&
      (!clientId.trim() || (!tokenEndpoint.trim() && !issuerUrl.trim()))
    ) {
      toast.error('External IdP 认证需要 Client ID，以及 Token Endpoint（或 Issuer URL）')
      return
    }

    mutate(
      {
        refreshToken: refreshToken.trim(),
        authMethod,
        email: email.trim() || undefined,
        authRegion: authRegion.trim() || undefined,
        apiRegion: apiRegion.trim() || undefined,
        clientId: clientId.trim() || undefined,
        clientSecret: clientSecret.trim() || undefined,
        tokenEndpoint: tokenEndpoint.trim() || undefined,
        issuerUrl: issuerUrl.trim() || undefined,
        scopes: scopes.trim() || undefined,
        profileArn: profileArn.trim() || undefined,
        priority: parseInt(priority) || 0,
        machineId: machineId.trim() || undefined,
        proxyUrl: proxyUrl.trim() || undefined,
        proxyUsername: proxyUsername.trim() || undefined,
        proxyPassword: proxyPassword.trim() || undefined,
      },
      {
        onSuccess: (data) => {
          toast.success(data.message)
          onOpenChange(false)
          resetForm()
        },
        onError: (error: unknown) => {
          toast.error(`添加失败: ${extractErrorMessage(error)}`)
        },
      }
    )
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg max-h-[85vh] flex flex-col">
        <DialogHeader>
          <DialogTitle>添加账号</DialogTitle>
        </DialogHeader>

        <form onSubmit={handleSubmit} className="flex flex-col min-h-0 flex-1">
          <div className="space-y-4 py-4 overflow-y-auto flex-1 pr-1">
            {/* 快速导入：粘贴本地 Kiro 登录信息自动填充 */}
            <div className="space-y-2 rounded-md border border-dashed border-input p-3">
              <label htmlFor="importText" className="text-sm font-medium">
                快速导入{' '}
                <span className="text-muted-foreground text-xs">
                  (粘贴本地 ~/.aws/sso/cache/kiro-auth-token.json 内容自动填充)
                </span>
              </label>
              <textarea
                id="importText"
                placeholder='粘贴 kiro-auth-token.json 全文，自动识别 external_idp / idc / social 并填写下方字段'
                value={importText}
                onChange={(e) => setImportText(e.target.value)}
                disabled={isPending}
                rows={3}
                className={inputClass}
              />
              <Button
                type="button"
                variant="outline"
                onClick={() => handleImportJson(importText)}
                disabled={isPending}
              >
                解析并填充
              </Button>
            </div>

            {/* Refresh Token */}
            <div className="space-y-2">
              <label htmlFor="refreshToken" className="text-sm font-medium">
                Refresh Token <span className="text-red-500">*</span>
              </label>
              <Input
                id="refreshToken"
                type="password"
                placeholder="请输入 Refresh Token"
                value={refreshToken}
                onChange={(e) => setRefreshToken(e.target.value)}
                disabled={isPending}
              />
            </div>

            {/* 用户名/邮箱 */}
            <div className="space-y-2">
              <label htmlFor="email" className="text-sm font-medium">
                用户名 / 邮箱
              </label>
              <Input
                id="email"
                type="text"
                placeholder="请输入账号邮箱（用于标识账号）"
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                disabled={isPending}
              />
            </div>

            {/* 认证方式 */}
            <div className="space-y-2">
              <label htmlFor="authMethod" className="text-sm font-medium">
                认证方式
              </label>
              <select
                id="authMethod"
                value={authMethod}
                onChange={(e) => setAuthMethod(e.target.value as AuthMethod)}
                disabled={isPending}
                className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
              >
                <option value="social">Social</option>
                <option value="idc">IdC/Builder-ID/IAM</option>
                <option value="external_idp">External IdP（企业版 / 微软 Entra 等）</option>
              </select>
            </div>

            {/* Region 配置 */}
            <div className="space-y-2">
              <label className="text-sm font-medium">Region 配置</label>
              <div className="grid grid-cols-2 gap-2">
                <div>
                  <Input
                    id="authRegion"
                    placeholder="Auth Region"
                    value={authRegion}
                    onChange={(e) => setAuthRegion(e.target.value)}
                    disabled={isPending}
                  />
                </div>
                <div>
                  <Input
                    id="apiRegion"
                    placeholder="API Region"
                    value={apiRegion}
                    onChange={(e) => setApiRegion(e.target.value)}
                    disabled={isPending}
                  />
                </div>
              </div>
              <p className="text-xs text-muted-foreground">
                均可留空使用全局配置。Auth Region 用于 Token 刷新，API Region 用于 API 请求
              </p>
            </div>

            {/* IdC/Builder-ID/IAM 额外字段 */}
            {authMethod === 'idc' && (
              <>
                <div className="space-y-2">
                  <label htmlFor="clientId" className="text-sm font-medium">
                    Client ID <span className="text-red-500">*</span>
                  </label>
                  <Input
                    id="clientId"
                    placeholder="请输入 Client ID"
                    value={clientId}
                    onChange={(e) => setClientId(e.target.value)}
                    disabled={isPending}
                  />
                </div>
                <div className="space-y-2">
                  <label htmlFor="clientSecret" className="text-sm font-medium">
                    Client Secret <span className="text-red-500">*</span>
                  </label>
                  <Input
                    id="clientSecret"
                    type="password"
                    placeholder="请输入 Client Secret"
                    value={clientSecret}
                    onChange={(e) => setClientSecret(e.target.value)}
                    disabled={isPending}
                  />
                </div>
                <div className="space-y-2">
                  <label htmlFor="profileArn" className="text-sm font-medium">
                    Profile ARN <span className="text-muted-foreground text-xs">(企业版可选，留空自动获取)</span>
                  </label>
                  <Input
                    id="profileArn"
                    type="text"
                    placeholder="arn:aws:codewhisperer:us-east-1:...:profile/... （留空则首次请求自动获取）"
                    value={profileArn}
                    onChange={(e) => setProfileArn(e.target.value)}
                    disabled={isPending}
                  />
                </div>
              </>
            )}

            {/* External IdP（企业版 / 微软 Entra 等）额外字段 */}
            {authMethod === 'external_idp' && (
              <>
                <div className="space-y-2">
                  <label htmlFor="extClientId" className="text-sm font-medium">
                    Client ID <span className="text-red-500">*</span>
                  </label>
                  <Input
                    id="extClientId"
                    placeholder="OIDC Client ID（如微软 Entra 应用 ID）"
                    value={clientId}
                    onChange={(e) => setClientId(e.target.value)}
                    disabled={isPending}
                  />
                </div>
                <div className="space-y-2">
                  <label htmlFor="tokenEndpoint" className="text-sm font-medium">
                    Token Endpoint <span className="text-red-500">*</span>
                  </label>
                  <Input
                    id="tokenEndpoint"
                    placeholder="https://login.microsoftonline.com/<tenant>/oauth2/v2.0/token"
                    value={tokenEndpoint}
                    onChange={(e) => setTokenEndpoint(e.target.value)}
                    disabled={isPending}
                  />
                </div>
                <div className="space-y-2">
                  <label htmlFor="issuerUrl" className="text-sm font-medium">
                    Issuer URL{' '}
                    <span className="text-muted-foreground text-xs">(可选，用于自动发现 Token Endpoint)</span>
                  </label>
                  <Input
                    id="issuerUrl"
                    placeholder="https://login.microsoftonline.com/<tenant>/v2.0"
                    value={issuerUrl}
                    onChange={(e) => setIssuerUrl(e.target.value)}
                    disabled={isPending}
                  />
                </div>
                <div className="space-y-2">
                  <label htmlFor="scopes" className="text-sm font-medium">
                    Scopes{' '}
                    <span className="text-muted-foreground text-xs">(空格分隔，建议含 offline_access)</span>
                  </label>
                  <Input
                    id="scopes"
                    placeholder="api://<app>/codewhisperer:conversations ... offline_access"
                    value={scopes}
                    onChange={(e) => setScopes(e.target.value)}
                    disabled={isPending}
                  />
                </div>
                <div className="space-y-2">
                  <label htmlFor="extProfileArn" className="text-sm font-medium">
                    Profile ARN <span className="text-muted-foreground text-xs">(留空自动获取)</span>
                  </label>
                  <Input
                    id="extProfileArn"
                    placeholder="留空则首次请求自动从 management.kiro.dev 获取"
                    value={profileArn}
                    onChange={(e) => setProfileArn(e.target.value)}
                    disabled={isPending}
                  />
                </div>
              </>
            )}

            {/* 优先级 */}
            <div className="space-y-2">
              <label htmlFor="priority" className="text-sm font-medium">
                优先级
              </label>
              <Input
                id="priority"
                type="number"
                min="0"
                placeholder="数字越小优先级越高"
                value={priority}
                onChange={(e) => setPriority(e.target.value)}
                disabled={isPending}
              />
              <p className="text-xs text-muted-foreground">
                数字越小优先级越高，默认为 0
              </p>
            </div>

            {/* Machine ID */}
            <div className="space-y-2">
              <label htmlFor="machineId" className="text-sm font-medium">
                Machine ID
              </label>
              <Input
                id="machineId"
                placeholder="留空使用配置中字段, 否则由刷新Token自动派生"
                value={machineId}
                onChange={(e) => setMachineId(e.target.value)}
                disabled={isPending}
              />
              <p className="text-xs text-muted-foreground">
                可选，64 位十六进制字符串，留空使用配置中字段, 否则由刷新Token自动派生
              </p>
            </div>

            {/* 代理配置 */}
            <div className="space-y-2">
              <label className="text-sm font-medium">代理配置</label>
              <Input
                id="proxyUrl"
                placeholder='代理 URL（留空使用全局配置，"direct" 不使用代理）'
                value={proxyUrl}
                onChange={(e) => setProxyUrl(e.target.value)}
                disabled={isPending}
              />
              <div className="grid grid-cols-2 gap-2">
                <Input
                  id="proxyUsername"
                  placeholder="代理用户名"
                  value={proxyUsername}
                  onChange={(e) => setProxyUsername(e.target.value)}
                  disabled={isPending}
                />
                <Input
                  id="proxyPassword"
                  type="password"
                  placeholder="代理密码"
                  value={proxyPassword}
                  onChange={(e) => setProxyPassword(e.target.value)}
                  disabled={isPending}
                />
              </div>
              <p className="text-xs text-muted-foreground">
                留空使用全局代理。输入 "direct" 可显式不使用代理
              </p>
            </div>
          </div>

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => onOpenChange(false)}
              disabled={isPending}
            >
              取消
            </Button>
            <Button type="submit" disabled={isPending}>
              {isPending ? '添加中...' : '添加'}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
