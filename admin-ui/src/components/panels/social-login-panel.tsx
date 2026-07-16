// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState } from 'react'
import { toast } from 'sonner'
import { useQueryClient } from '@tanstack/react-query'
import { DialogFooter } from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { startSocialLogin, pollSocialLogin } from '@/api/credentials'
import { extractErrorMessage } from '@/lib/utils'
import type { DeviceLoginStartResponse } from '@/types/api'

interface SocialLoginPanelProps {
  onClose: () => void
}

/** Social 登录（Kiro 托管登录页 app.kiro.dev，支持 Google / GitHub / Microsoft 等）。 */
export function SocialLoginPanel({ onClose }: SocialLoginPanelProps) {
  const queryClient = useQueryClient()
  const [region, setRegion] = useState('')
  const [name, setName] = useState('')
  const [starting, setStarting] = useState(false)
  const [session, setSession] = useState<DeviceLoginStartResponse | null>(null)
  const [redirectResponse, setRedirectResponse] = useState('')
  const [completing, setCompleting] = useState(false)

  const handleStart = async () => {
    setStarting(true)
    try {
      const res = await startSocialLogin({
        region: region.trim() || undefined,
        name: name.trim() || undefined,
      })
      setSession(res)
    } catch (error: unknown) {
      toast.error(`发起登录失败: ${extractErrorMessage(error)}`)
    } finally {
      setStarting(false)
    }
  }

  const handleComplete = async () => {
    if (!session) return
    if (!redirectResponse.trim()) {
      toast.error('请粘贴授权后跳转的回调地址（或其中的 code）')
      return
    }
    setCompleting(true)
    try {
      const res = await pollSocialLogin(session.sessionId, redirectResponse.trim())
      if (res.status === 'complete') {
        queryClient.invalidateQueries({ queryKey: ['credentials'] })
        if (res.profileStatus && res.profileStatus !== 'ready') {
          toast.warning(
            `登录成功，账号 #${res.credentialId} 已添加，但额度校验暂未完成，稍后会自动重试`
          )
        } else {
          toast.success(`登录成功，已添加账号 #${res.credentialId}`)
        }
        onClose()
      } else {
        toast.error(`登录失败: ${res.message || '未知错误'}`)
      }
    } catch (error: unknown) {
      toast.error(`完成登录失败: ${extractErrorMessage(error)}`)
    } finally {
      setCompleting(false)
    }
  }

  const handleCopyUrl = async () => {
    if (!session) return
    try {
      await navigator.clipboard.writeText(session.authorizeUrl)
      toast.success('已复制登录链接')
    } catch {
      toast.error('复制失败，请手动选择链接复制')
    }
  }

  if (!session) {
    return (
      <div className="space-y-4 py-4">
        <p className="text-sm text-muted-foreground">
          适用于 Kiro 社交/联合登录（Google / GitHub / Microsoft 等，无需企业门户地址）。
          点「获取登录链接」后会给出授权链接，在浏览器登录后把跳转到的回调地址粘贴回来即可。
        </p>
        <div className="space-y-2">
          <label htmlFor="socialName" className="text-sm font-medium">
            账号名称（可选）
          </label>
          <Input
            id="socialName"
            type="text"
            placeholder="如 ms-r7i6bwco，方便在列表里区分"
            value={name}
            onChange={(e) => setName(e.target.value)}
            disabled={starting}
          />
        </div>
        <div className="space-y-2">
          <label htmlFor="socialRegion" className="text-sm font-medium">
            Region（可选）
          </label>
          <Input
            id="socialRegion"
            type="text"
            placeholder="留空使用全局配置（通常 us-east-1）"
            value={region}
            onChange={(e) => setRegion(e.target.value)}
            disabled={starting}
          />
          <p className="text-xs text-muted-foreground">
            仅决定 token 换取/刷新所属节点，一般留空即可。
          </p>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={onClose} disabled={starting}>
            取消
          </Button>
          <Button onClick={handleStart} disabled={starting}>
            {starting ? '正在发起...' : '获取登录链接'}
          </Button>
        </DialogFooter>
      </div>
    )
  }

  return (
    <div className="space-y-4 py-4">
      <div className="space-y-2">
        <p className="text-sm text-muted-foreground">
          第 1 步：点下面按钮打开登录页（<b>建议用无痕窗口</b>，避免用到浏览器里已登录的其它账号），
          选择对应的登录方式（如 Microsoft）完成登录。
        </p>
        <div className="flex gap-2">
          <Button onClick={() => window.open(session.authorizeUrl, '_blank', 'noopener,noreferrer')}>
            打开登录页
          </Button>
          <Button variant="outline" onClick={handleCopyUrl}>
            复制链接
          </Button>
        </div>
      </div>
      <div className="space-y-2">
        <p className="text-sm text-muted-foreground">
          第 2 步：登录后浏览器会跳转到 <code>http://localhost:3128/?code=...</code>，
          页面会显示<b>无法访问（这是正常的）</b>。把地址栏里那条完整地址复制，粘贴到下面。
        </p>
        <textarea
          className="flex min-h-[72px] w-full rounded-md border border-input bg-transparent px-3 py-2 text-sm font-mono shadow-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50"
          placeholder="http://localhost:3128/?code=...&state=..."
          value={redirectResponse}
          onChange={(e) => setRedirectResponse(e.target.value)}
          disabled={completing}
        />
      </div>
      <DialogFooter className="flex-col sm:flex-row gap-2">
        <Button
          variant="outline"
          onClick={() => {
            setSession(null)
            setRedirectResponse('')
          }}
          disabled={completing}
        >
          上一步
        </Button>
        <Button onClick={handleComplete} disabled={completing}>
          {completing ? '正在完成...' : '完成登录'}
        </Button>
      </DialogFooter>
    </div>
  )
}
