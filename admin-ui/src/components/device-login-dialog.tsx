// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState } from 'react'
import { toast } from 'sonner'
import { useQueryClient } from '@tanstack/react-query'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { startDeviceLogin, pollDeviceLogin } from '@/api/credentials'
import { extractErrorMessage } from '@/lib/utils'
import type { DeviceLoginStartResponse } from '@/types/api'

interface DeviceLoginDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

export function DeviceLoginDialog({ open, onOpenChange }: DeviceLoginDialogProps) {
  const queryClient = useQueryClient()
  const [startUrl, setStartUrl] = useState('')
  const [region, setRegion] = useState('')
  const [name, setName] = useState('')
  const [starting, setStarting] = useState(false)
  const [session, setSession] = useState<DeviceLoginStartResponse | null>(null)
  const [redirectResponse, setRedirectResponse] = useState('')
  const [completing, setCompleting] = useState(false)

  const reset = () => {
    setStartUrl('')
    setRegion('')
    setName('')
    setStarting(false)
    setSession(null)
    setRedirectResponse('')
    setCompleting(false)
  }

  // 支持一次性粘贴 "用户名|密码|门户地址|区域" 自动拆分填充。
  // 例：4F1GTc-user85|******|https://d-xxxx.awsapps.com/start|us-east-1
  const handleStartUrlInput = (value: string) => {
    if (value.includes('|')) {
      const parts = value.split('|').map((p) => p.trim())
      const urlIdx = parts.findIndex((p) => /awsapps\.com|^https?:\/\//i.test(p))
      if (urlIdx >= 0) {
        setStartUrl(parts[urlIdx])
        // 第一段作为账号名（便于在列表里区分）
        if (urlIdx > 0 && parts[0]) {
          setName(parts[0])
        }
        // 门户地址后面一段若是区域格式（如 us-east-1）则填入区域
        const maybeRegion = parts[urlIdx + 1]
        if (maybeRegion && /^[a-z]{2}-[a-z]+-\d+$/i.test(maybeRegion)) {
          setRegion(maybeRegion)
        }
        return
      }
    }
    setStartUrl(value)
  }

  const handleStart = async () => {
    if (!startUrl.trim()) {
      toast.error('请输入企业门户地址 (Start URL)')
      return
    }
    setStarting(true)
    try {
      const res = await startDeviceLogin({
        startUrl: startUrl.trim(),
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
      const res = await pollDeviceLogin(session.sessionId, redirectResponse.trim())
      if (res.status === 'complete') {
        toast.success(`登录成功，已添加账号 #${res.credentialId}`)
        queryClient.invalidateQueries({ queryKey: ['credentials'] })
        onOpenChange(false)
        reset()
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

  const handleOpenChange = (next: boolean) => {
    if (!next) reset()
    onOpenChange(next)
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>SSO 登录（企业 IdC）</DialogTitle>
        </DialogHeader>

        {!session ? (
          <div className="space-y-4">
            <p className="text-sm text-muted-foreground">
              填写企业门户地址和区域后点击「获取登录链接」。下一步会给出一个授权链接，
              在浏览器登录授权后，把跳转到的回调地址粘贴回来即可完成添加。
            </p>
            <div className="space-y-2">
              <label htmlFor="startUrl" className="text-sm font-medium">
                Start URL <span className="text-red-500">*</span>
              </label>
              <Input
                id="startUrl"
                type="text"
                placeholder="https://d-xxxxxxxxxx.awsapps.com/start"
                value={startUrl}
                onChange={(e) => handleStartUrlInput(e.target.value)}
                disabled={starting}
              />
              <p className="text-xs text-muted-foreground">
                可直接粘贴 <code>用户名|密码|门户地址|区域</code> 整行，会自动拆分填充。
              </p>
            </div>
            <div className="space-y-2">
              <label htmlFor="accName" className="text-sm font-medium">
                账号名称（可选）
              </label>
              <Input
                id="accName"
                type="text"
                placeholder="如 4F1GTc-user85，方便在列表里区分"
                value={name}
                onChange={(e) => setName(e.target.value)}
                disabled={starting}
              />
            </div>
            <div className="space-y-2">
              <label htmlFor="region" className="text-sm font-medium">
                Region（可选）
              </label>
              <Input
                id="region"
                type="text"
                placeholder="留空自动探测"
                value={region}
                onChange={(e) => setRegion(e.target.value)}
                disabled={starting}
              />
              <p className="text-xs text-muted-foreground">
                留空即可自动探测门户所属区域（Kiro 账号通常是 us-east-1）。只有确有需要时才手动填写。
              </p>
            </div>
            <DialogFooter>
              <Button variant="outline" onClick={() => handleOpenChange(false)} disabled={starting}>
                取消
              </Button>
              <Button onClick={handleStart} disabled={starting}>
                {starting ? '正在发起...' : '获取登录链接'}
              </Button>
            </DialogFooter>
          </div>
        ) : (
          <div className="space-y-4">
            <div className="space-y-2">
              <p className="text-sm text-muted-foreground">
                第 1 步：点下面按钮打开授权链接（<b>建议用无痕窗口</b>，避免用到浏览器里已登录的其它账号），
                用目标企业账号登录并授权。
              </p>
              <div className="flex gap-2">
                <Button
                  onClick={() =>
                    window.open(session.authorizeUrl, '_blank', 'noopener,noreferrer')
                  }
                >
                  打开授权页
                </Button>
                <Button variant="outline" onClick={handleCopyUrl}>
                  复制链接
                </Button>
              </div>
            </div>
            <div className="space-y-2">
              <p className="text-sm text-muted-foreground">
                第 2 步：授权后浏览器会跳转到 <code>http://127.0.0.1/oauth/callback?code=...</code>，
                页面会显示<b>无法访问（这是正常的）</b>。把地址栏里那条完整地址复制，粘贴到下面。
              </p>
              <textarea
                className="flex min-h-[72px] w-full rounded-md border border-input bg-transparent px-3 py-2 text-sm font-mono shadow-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50"
                placeholder="http://127.0.0.1/oauth/callback?code=...&state=..."
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
        )}
      </DialogContent>
    </Dialog>
  )
}
