// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState, useEffect } from 'react'
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
import { useUpdateCredential } from '@/hooks/use-credentials'
import { extractErrorMessage } from '@/lib/utils'
import type { CredentialStatusItem } from '@/types/api'

interface EditCredentialDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  credential: CredentialStatusItem
}

export function EditCredentialDialog({ open, onOpenChange, credential }: EditCredentialDialogProps) {
  const [authRegion, setAuthRegion] = useState('')
  const [apiRegion, setApiRegion] = useState('')
  const [profileArn, setProfileArn] = useState('')
  const [nickname, setNickname] = useState('')
  const [email, setEmail] = useState('')
  const [clientId, setClientId] = useState('')
  const [clientSecret, setClientSecret] = useState('')
  const [machineId, setMachineId] = useState('')
  const [proxyUrl, setProxyUrl] = useState('')
  const [proxyUsername, setProxyUsername] = useState('')
  const [proxyPassword, setProxyPassword] = useState('')

  const { mutate, isPending } = useUpdateCredential()

  // 当对话框打开或凭据变化时，回填已有信息
  useEffect(() => {
    if (open) {
      setAuthRegion(credential.authRegion || '')
      setApiRegion(credential.apiRegion || '')
      setProfileArn(credential.profileArn || '')
      setNickname(credential.nickname || '')
      setEmail(credential.email || '')
      setClientId('')
      setClientSecret('')
      setMachineId('')
      setProxyUrl(credential.proxyUrl || '')
      setProxyUsername('')
      setProxyPassword('')
    }
  }, [open, credential])

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()

    // 构建更新字段：可见字段按“与原值不同才提交”，敏感字段（密钥/密码）留空表示不改
    const data: Record<string, string> = {}
    if (authRegion !== (credential.authRegion || '')) data.authRegion = authRegion
    if (apiRegion !== (credential.apiRegion || '')) data.apiRegion = apiRegion
    if (profileArn !== (credential.profileArn || '')) data.profileArn = profileArn
    if (nickname !== (credential.nickname || '')) data.nickname = nickname
    if (email !== (credential.email || '')) data.email = email
    if (clientId !== '') data.clientId = clientId
    if (clientSecret !== '') data.clientSecret = clientSecret
    if (machineId !== '') data.machineId = machineId
    if (proxyUrl !== (credential.proxyUrl || '')) data.proxyUrl = proxyUrl
    if (proxyUsername !== '') data.proxyUsername = proxyUsername
    if (proxyPassword !== '') data.proxyPassword = proxyPassword

    if (Object.keys(data).length === 0) {
      toast.info('没有需要更新的字段')
      return
    }

    mutate(
      { id: credential.id, data },
      {
        onSuccess: (res) => {
          toast.success(res.message)
          onOpenChange(false)
        },
        onError: (error: unknown) => {
          toast.error(`更新失败: ${extractErrorMessage(error)}`)
        },
      }
    )
  }

  const isIdc = credential.authMethod === 'idc'

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg max-h-[85vh] flex flex-col">
        <DialogHeader>
          <DialogTitle>编辑账号 #{credential.id}</DialogTitle>
        </DialogHeader>

        <form onSubmit={handleSubmit} className="flex flex-col min-h-0 flex-1">
          <div className="space-y-4 py-4 overflow-y-auto flex-1 pr-1">
            <p className="text-xs text-muted-foreground">
              已自动回填现有信息，可直接修改。Client ID / Client Secret / Machine ID / 代理密码等敏感字段留空表示保持不变。
            </p>

            {/* 昵称 */}
            <div className="space-y-2">
              <label className="text-sm font-medium">昵称</label>
              <Input
                placeholder="显示名称（用于卡片标题）"
                value={nickname}
                onChange={(e) => setNickname(e.target.value)}
                disabled={isPending}
              />
            </div>

            {/* 用户名/邮箱 */}
            <div className="space-y-2">
              <label className="text-sm font-medium">用户名 / 邮箱</label>
              <Input
                placeholder="账号邮箱（用于标识账号）"
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                disabled={isPending}
              />
            </div>

            {/* Region 配置 */}
            <div className="space-y-2">
              <label className="text-sm font-medium">Region 配置</label>
              <div className="grid grid-cols-2 gap-2">
                <Input
                  placeholder="Auth Region"
                  value={authRegion}
                  onChange={(e) => setAuthRegion(e.target.value)}
                  disabled={isPending}
                />
                <Input
                  placeholder="API Region"
                  value={apiRegion}
                  onChange={(e) => setApiRegion(e.target.value)}
                  disabled={isPending}
                />
              </div>
              <p className="text-xs text-muted-foreground">
                Auth Region 用于 Token 刷新，API Region 用于 API 请求
              </p>
            </div>

            {/* IdC 字段 */}
            {isIdc && (
              <>
                <div className="space-y-2">
                  <label className="text-sm font-medium">Profile ARN</label>
                  <Input
                    placeholder="arn:aws:codewhisperer:...:profile/..."
                    value={profileArn}
                    onChange={(e) => setProfileArn(e.target.value)}
                    disabled={isPending}
                  />
                  <p className="text-xs text-muted-foreground">
                    企业版账号需要；留空则首次请求时自动获取
                  </p>
                </div>
                <div className="space-y-2">
                  <label className="text-sm font-medium">Client ID</label>
                  <Input
                    placeholder="留空不修改"
                    value={clientId}
                    onChange={(e) => setClientId(e.target.value)}
                    disabled={isPending}
                  />
                </div>
                <div className="space-y-2">
                  <label className="text-sm font-medium">Client Secret</label>
                  <Input
                    type="password"
                    placeholder="留空不修改"
                    value={clientSecret}
                    onChange={(e) => setClientSecret(e.target.value)}
                    disabled={isPending}
                  />
                </div>
              </>
            )}

            {/* Machine ID */}
            <div className="space-y-2">
              <label className="text-sm font-medium">Machine ID</label>
              <Input
                placeholder="留空不修改"
                value={machineId}
                onChange={(e) => setMachineId(e.target.value)}
                disabled={isPending}
              />
            </div>

            {/* 代理配置 */}
            <div className="space-y-2">
              <label className="text-sm font-medium">代理配置</label>
              <Input
                placeholder='代理 URL（"direct" 不使用代理）'
                value={proxyUrl}
                onChange={(e) => setProxyUrl(e.target.value)}
                disabled={isPending}
              />
              <div className="grid grid-cols-2 gap-2">
                <Input
                  placeholder="代理用户名"
                  value={proxyUsername}
                  onChange={(e) => setProxyUsername(e.target.value)}
                  disabled={isPending}
                />
                <Input
                  type="password"
                  placeholder="代理密码"
                  value={proxyPassword}
                  onChange={(e) => setProxyPassword(e.target.value)}
                  disabled={isPending}
                />
              </div>
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
              {isPending ? '更新中...' : '保存'}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
