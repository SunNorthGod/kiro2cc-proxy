// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState } from 'react'
import { toast } from 'sonner'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import {
  useLoadBalancingMode, useSetLoadBalancingMode,
  useAuthKeys, useSetAuthKeys,
} from '@/hooks/use-credentials'
import { extractErrorMessage } from '@/lib/utils'

export function SettingsPanel() {
  const { data: loadBalancingData, isLoading: isLoadingMode } = useLoadBalancingMode()
  const { mutate: setLoadBalancingMode, isPending: isSettingMode } = useSetLoadBalancingMode()
  const { data: authKeysData, isLoading: isLoadingAuthKeys } = useAuthKeys()
  const { mutate: setAuthKeysMut, isPending: isSettingAuthKeys } = useSetAuthKeys()
  const [apiKeyDraft, setApiKeyDraft] = useState('')
  const [adminApiKeyDraft, setAdminApiKeyDraft] = useState('')
  const [editingApiKey, setEditingApiKey] = useState(false)
  const [editingAdminApiKey, setEditingAdminApiKey] = useState(false)

  return (
    <div className="space-y-6">
      <h2 className="text-xl font-semibold">设置</h2>

      <div className="grid gap-4 md:grid-cols-2">
        {/* 认证密钥 */}
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium">认证密钥</CardTitle>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <span className="text-sm font-medium">主 API Key</span>
                {!editingApiKey && (
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => { setApiKeyDraft(''); setEditingApiKey(true) }}
                    disabled={isLoadingAuthKeys}
                  >
                    修改
                  </Button>
                )}
              </div>
              {editingApiKey ? (
                <div className="flex gap-2">
                  <Input
                    type="text"
                    placeholder="输入新的 API Key"
                    value={apiKeyDraft}
                    onChange={(e) => setApiKeyDraft(e.target.value)}
                    className="text-sm"
                  />
                  <Button
                    size="sm"
                    disabled={!apiKeyDraft.trim() || isSettingAuthKeys}
                    onClick={() => {
                      setAuthKeysMut({ apiKey: apiKeyDraft.trim() }, {
                        onSuccess: () => {
                          toast.success('主 API Key 已更新')
                          setEditingApiKey(false)
                          setApiKeyDraft('')
                        },
                        onError: (e) => toast.error(extractErrorMessage(e)),
                      })
                    }}
                  >
                    保存
                  </Button>
                  <Button variant="ghost" size="sm" onClick={() => setEditingApiKey(false)}>
                    取消
                  </Button>
                </div>
              ) : (
                <p className="text-xs text-muted-foreground font-mono">
                  {isLoadingAuthKeys ? '加载中...' : authKeysData?.apiKey ?? '—'}
                </p>
              )}
            </div>
            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <span className="text-sm font-medium">Admin Password</span>
                {!editingAdminApiKey && (
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => { setAdminApiKeyDraft(''); setEditingAdminApiKey(true) }}
                    disabled={isLoadingAuthKeys}
                  >
                    修改
                  </Button>
                )}
              </div>
              {editingAdminApiKey ? (
                <div className="flex gap-2">
                  <Input
                    type="text"
                    placeholder="输入新的 Admin Password"
                    value={adminApiKeyDraft}
                    onChange={(e) => setAdminApiKeyDraft(e.target.value)}
                    className="text-sm"
                  />
                  <Button
                    size="sm"
                    disabled={!adminApiKeyDraft.trim() || isSettingAuthKeys}
                    onClick={() => {
                      setAuthKeysMut({ adminApiKey: adminApiKeyDraft.trim() }, {
                        onSuccess: () => {
                          toast.success('Admin Password 已更新，请使用新密码重新登录')
                          setEditingAdminApiKey(false)
                          setAdminApiKeyDraft('')
                        },
                        onError: (e) => toast.error(extractErrorMessage(e)),
                      })
                    }}
                  >
                    保存
                  </Button>
                  <Button variant="ghost" size="sm" onClick={() => setEditingAdminApiKey(false)}>
                    取消
                  </Button>
                </div>
              ) : (
                <p className="text-xs text-muted-foreground font-mono">
                  {isLoadingAuthKeys ? '加载中...' : authKeysData?.adminApiKey ?? '—'}
                </p>
              )}
            </div>
            <p className="text-xs text-muted-foreground">
              修改后立即生效，旧密码将失效。修改 Admin Password 后需要用新密码重新登录。
            </p>
          </CardContent>
        </Card>

        {/* 负载均衡 */}
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium">负载均衡</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="flex items-center justify-between py-3">
              <div className="flex flex-col">
                <span className="text-sm font-medium">均衡模式</span>
                <span className="text-xs text-muted-foreground mt-1">
                  优先级：粘住最高优先级账号 · 均衡负载：全部账号轮询 · 自动：最高优先级档内负载均衡
                </span>
              </div>
              <Button
                variant="outline"
                size="sm"
                onClick={() => {
                  const order = ['priority', 'balanced', 'auto'] as const
                  const cur = loadBalancingData?.mode ?? 'priority'
                  const newMode = order[(order.indexOf(cur) + 1) % order.length]
                  const label = newMode === 'priority' ? '优先级模式' : newMode === 'balanced' ? '均衡负载' : '自动'
                  setLoadBalancingMode(newMode, {
                    onSuccess: () => toast.success(`已切换为${label}`),
                    onError: (e) => toast.error(extractErrorMessage(e)),
                  })
                }}
                disabled={isLoadingMode || isSettingMode}
              >
                {isLoadingMode
                  ? '加载中...'
                  : loadBalancingData?.mode === 'priority'
                    ? '优先级模式'
                    : loadBalancingData?.mode === 'balanced'
                      ? '均衡负载'
                      : '自动'}
              </Button>
            </div>
          </CardContent>
        </Card>
      </div>
    </div>
  )
}