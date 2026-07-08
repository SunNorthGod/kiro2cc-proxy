// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useEffect, useRef, useState } from 'react'
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
  const [region, setRegion] = useState('us-east-1')
  const [starting, setStarting] = useState(false)
  const [session, setSession] = useState<DeviceLoginStartResponse | null>(null)
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null)

  const clearTimer = () => {
    if (timerRef.current) {
      clearInterval(timerRef.current)
      timerRef.current = null
    }
  }

  const reset = () => {
    clearTimer()
    setStartUrl('')
    setRegion('us-east-1')
    setStarting(false)
    setSession(null)
  }

  // 轮询登录状态
  useEffect(() => {
    if (!session) return
    const deadline = Date.now() + Math.max(session.expiresIn, 60) * 1000
    const intervalMs = Math.max(session.interval, 1) * 1000

    const poll = async () => {
      if (Date.now() > deadline) {
        clearTimer()
        setSession(null)
        toast.error('登录超时，请重试')
        return
      }
      try {
        const res = await pollDeviceLogin(session.sessionId)
        if (res.status === 'complete') {
          clearTimer()
          toast.success(`登录成功，已添加账号 #${res.credentialId}`)
          queryClient.invalidateQueries({ queryKey: ['credentials'] })
          setSession(null)
          onOpenChange(false)
          reset()
        } else if (res.status === 'error') {
          clearTimer()
          setSession(null)
          toast.error(`登录失败: ${res.message || '未知错误'}`)
        }
        // pending: 继续等待
      } catch {
        // 网络抖动等瞬时错误，继续轮询直到超时
      }
    }

    timerRef.current = setInterval(poll, intervalMs)
    return clearTimer
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [session])

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
      })
      setSession(res)
      // 自动打开验证链接
      window.open(res.verificationUriComplete, '_blank', 'noopener,noreferrer')
    } catch (error: unknown) {
      toast.error(`发起登录失败: ${extractErrorMessage(error)}`)
    } finally {
      setStarting(false)
    }
  }

  const handleOpenChange = (next: boolean) => {
    if (!next) reset()
    onOpenChange(next)
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>SSO 设备登录（企业 IdC）</DialogTitle>
        </DialogHeader>

        {!session ? (
          <div className="space-y-4">
            <p className="text-sm text-muted-foreground">
              填写企业门户地址后点击「开始登录」，浏览器会打开 AWS 授权页面，
              用你的企业账号登录并授权即可，无需手动复制 token。
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
                onChange={(e) => setStartUrl(e.target.value)}
                disabled={starting}
              />
            </div>
            <div className="space-y-2">
              <label htmlFor="region" className="text-sm font-medium">
                Region
              </label>
              <Input
                id="region"
                type="text"
                placeholder="us-east-1"
                value={region}
                onChange={(e) => setRegion(e.target.value)}
                disabled={starting}
              />
            </div>
            <DialogFooter>
              <Button variant="outline" onClick={() => handleOpenChange(false)} disabled={starting}>
                取消
              </Button>
              <Button onClick={handleStart} disabled={starting}>
                {starting ? '正在发起...' : '开始登录'}
              </Button>
            </DialogFooter>
          </div>
        ) : (
          <div className="space-y-4">
            <p className="text-sm text-muted-foreground">
              在新打开的 AWS 页面完成登录并点击「Allow / 允许」授权(先核对验证码一致)。
              授权后 AWS 页面不会自动关闭,手动回到本面板即可,会自动检测并建号。
              若页面显示的是别的账号,请用浏览器无痕窗口打开授权链接。
            </p>
            <div className="rounded-md border p-4 text-center">
              <div className="text-xs text-muted-foreground mb-1">验证码 (User Code)</div>
              <div className="text-2xl font-mono font-bold tracking-widest">{session.userCode}</div>
            </div>
            <div className="flex items-center gap-2">
              <span className="inline-block h-2 w-2 animate-pulse rounded-full bg-green-500" />
              <span className="text-sm text-muted-foreground">等待浏览器完成授权...</span>
            </div>
            <DialogFooter className="flex-col sm:flex-row gap-2">
              <Button
                variant="outline"
                onClick={() =>
                  window.open(session.verificationUriComplete, '_blank', 'noopener,noreferrer')
                }
              >
                重新打开授权页面
              </Button>
              <Button variant="outline" onClick={() => handleOpenChange(false)}>
                取消
              </Button>
            </DialogFooter>
          </div>
        )}
      </DialogContent>
    </Dialog>
  )
}
