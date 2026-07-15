// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState } from 'react'
import { toast } from 'sonner'
import { Plus, Trash2, RefreshCw, Pencil, Server, Zap, LifeBuoy } from 'lucide-react'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Badge } from '@/components/ui/badge'
import { Switch } from '@/components/ui/switch'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '@/components/ui/dialog'
import {
  useRelays,
  useCreateRelay,
  useUpdateRelay,
  useDeleteRelay,
  useFetchRelayModels,
} from '@/hooks/use-credentials'
import { extractErrorMessage } from '@/lib/utils'
import type { RelayItem, RouteRule, RouteMode } from '@/types/api'

interface RelayFormState {
  name: string
  baseUrl: string
  apiKey: string
  billingMultiplier: string
  enabled: boolean
  routes: RouteRule[]
}

const emptyForm: RelayFormState = {
  name: '',
  baseUrl: '',
  apiKey: '',
  billingMultiplier: '1.0',
  enabled: true,
  routes: [],
}

export function RelayPanel() {
  const { data: relays, isLoading } = useRelays()
  const { mutate: createRelay, isPending: creating } = useCreateRelay()
  const { mutate: updateRelay, isPending: updating } = useUpdateRelay()
  const { mutate: deleteRelay } = useDeleteRelay()
  const { mutate: fetchModels, isPending: fetchingModels } = useFetchRelayModels()

  const [dialogOpen, setDialogOpen] = useState(false)
  const [editingId, setEditingId] = useState<number | null>(null)
  const [form, setForm] = useState<RelayFormState>(emptyForm)
  const [fetchingId, setFetchingId] = useState<number | null>(null)

  const editingRelay = relays?.find((r) => r.id === editingId) ?? null
  const modelOptions = editingRelay?.models ?? []

  const openCreate = () => {
    setEditingId(null)
    setForm(emptyForm)
    setDialogOpen(true)
  }

  const openEdit = (relay: RelayItem) => {
    setEditingId(relay.id)
    setForm({
      name: relay.name,
      baseUrl: relay.baseUrl,
      apiKey: '', // 留空表示不修改
      billingMultiplier: String(relay.billingMultiplier ?? 1.0),
      enabled: relay.enabled,
      routes: relay.routes.map((r) => ({ ...r })),
    })
    setDialogOpen(true)
  }

  const addRoute = () => {
    setForm((f) => ({
      ...f,
      routes: [...f.routes, { pattern: '', target: '', mode: 'fallback' as RouteMode }],
    }))
  }

  const updateRoute = (idx: number, patch: Partial<RouteRule>) => {
    setForm((f) => ({
      ...f,
      routes: f.routes.map((r, i) => (i === idx ? { ...r, ...patch } : r)),
    }))
  }

  const removeRoute = (idx: number) => {
    setForm((f) => ({ ...f, routes: f.routes.filter((_, i) => i !== idx) }))
  }

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    if (!form.name.trim() || !form.baseUrl.trim()) {
      toast.error('请填写名称和 Base URL')
      return
    }
    const mult = parseFloat(form.billingMultiplier)
    const billingMultiplier = isNaN(mult) || mult <= 0 ? 1.0 : mult
    // target 留空 = 透传原模型号；只要求 pattern 非空
    const cleanRoutes = form.routes
      .filter((r) => r.pattern.trim())
      .map((r) => ({ ...r, pattern: r.pattern.trim(), target: r.target.trim() }))

    if (editingId === null) {
      if (!form.apiKey.trim()) {
        toast.error('请填写中转 API Key')
        return
      }
      createRelay(
        {
          name: form.name.trim(),
          baseUrl: form.baseUrl.trim(),
          apiKey: form.apiKey.trim(),
          enabled: form.enabled,
          routes: cleanRoutes,
          billingMultiplier,
        },
        {
          onSuccess: () => {
            toast.success('中转已添加，可点击「拉取模型」再配置路由目标')
            setDialogOpen(false)
          },
          onError: (err) => toast.error(`添加失败: ${extractErrorMessage(err)}`),
        }
      )
    } else {
      updateRelay(
        {
          id: editingId,
          data: {
            name: form.name.trim(),
            baseUrl: form.baseUrl.trim(),
            apiKey: form.apiKey.trim() || undefined, // 空 = 不改
            enabled: form.enabled,
            routes: cleanRoutes,
            billingMultiplier,
          },
        },
        {
          onSuccess: () => {
            toast.success('中转已更新')
            setDialogOpen(false)
          },
          onError: (err) => toast.error(`更新失败: ${extractErrorMessage(err)}`),
        }
      )
    }
  }

  const handleToggleEnabled = (relay: RelayItem, enabled: boolean) => {
    updateRelay(
      { id: relay.id, data: { enabled } },
      {
        onSuccess: () => toast.success(enabled ? `已启用「${relay.name}」` : `已停用「${relay.name}」`),
        onError: (err) => toast.error(extractErrorMessage(err)),
      }
    )
  }

  const handleDelete = (relay: RelayItem) => {
    if (!confirm(`确定删除中转「${relay.name}」吗？`)) return
    deleteRelay(relay.id, {
      onSuccess: () => toast.success('已删除'),
      onError: (err) => toast.error(extractErrorMessage(err)),
    })
  }

  const handleFetchModels = (relay: RelayItem) => {
    setFetchingId(relay.id)
    fetchModels(relay.id, {
      onSuccess: (res) => toast.success(`拉取到 ${res.models.length} 个模型`),
      onError: (err) => toast.error(`拉取失败: ${extractErrorMessage(err)}`),
      onSettled: () => setFetchingId(null),
    })
  }

  return (
    <div>
      <div className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-[22px] font-bold tracking-[-0.02em]">中转对接</h1>
          <p className="text-[13px] text-muted-foreground mt-0.5">
            对接 Anthropic 兼容中转（如 sub2api）：某些模型直连中转，或仅在 Kiro 池整体失败时兜底
          </p>
        </div>
        <Button onClick={openCreate} size="sm">
          <Plus className="h-4 w-4 sm:mr-2" />
          <span className="hidden sm:inline">添加中转</span>
        </Button>
      </div>

      {isLoading ? (
        <div className="text-muted-foreground text-sm py-8 text-center">加载中...</div>
      ) : !relays || relays.length === 0 ? (
        <Card>
          <CardContent className="py-10 text-center text-muted-foreground">
            <Server className="h-8 w-8 mx-auto mb-3 opacity-40" />
            暂无中转，点击右上角「添加中转」开始
          </CardContent>
        </Card>
      ) : (
        <div className="space-y-3">
          {relays.map((relay) => (
            <Card key={relay.id}>
              <CardHeader className="pb-2">
                <div className="flex items-center justify-between">
                  <CardTitle className="text-base flex items-center gap-2">
                    {relay.name}
                    {!relay.enabled && <Badge variant="secondary">已停用</Badge>}
                    <span className="text-xs font-normal text-muted-foreground">
                      x{relay.billingMultiplier} 倍率
                    </span>
                  </CardTitle>
                  <div className="flex items-center gap-1.5">
                    <Switch
                      checked={relay.enabled}
                      onCheckedChange={(v) => handleToggleEnabled(relay, v)}
                    />
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-8 w-8"
                      title="拉取模型"
                      disabled={fetchingModels && fetchingId === relay.id}
                      onClick={() => handleFetchModels(relay)}
                    >
                      <RefreshCw
                        className={`h-4 w-4 ${fetchingModels && fetchingId === relay.id ? 'animate-spin' : ''}`}
                      />
                    </Button>
                    <Button variant="ghost" size="icon" className="h-8 w-8" title="编辑" onClick={() => openEdit(relay)}>
                      <Pencil className="h-4 w-4" />
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-8 w-8 text-destructive hover:text-destructive"
                      title="删除"
                      onClick={() => handleDelete(relay)}
                    >
                      <Trash2 className="h-4 w-4" />
                    </Button>
                  </div>
                </div>
              </CardHeader>
              <CardContent className="space-y-2">
                <div className="text-xs text-muted-foreground font-mono break-all">
                  {relay.baseUrl} · {relay.maskedApiKey}
                </div>
                <div className="flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground">
                  <span>模型 {relay.models.length} 个{relay.models.length === 0 ? '（点右上刷新拉取）' : ''}</span>
                  <span>RPM <span className="text-foreground font-medium">{relay.rpm ?? 0}</span></span>
                  <span>累计承接 <span className="text-foreground font-medium">{relay.requests ?? 0}</span> 次</span>
                  <span>累计计费 <span className="text-foreground font-medium">{(relay.credits ?? 0).toFixed(2)}</span> credits</span>
                </div>
                {relay.routes.length === 0 ? (
                  <div className="text-xs text-amber-600 dark:text-amber-400">
                    未配置路由规则，当前不会承接任何请求
                  </div>
                ) : (
                  <div className="flex flex-wrap gap-1.5">
                    {relay.routes.map((r, i) => (
                      <Badge key={i} variant="outline" className="gap-1 font-normal">
                        {r.mode === 'direct' ? (
                          <Zap className="h-3 w-3 text-blue-500" />
                        ) : (
                          <LifeBuoy className="h-3 w-3 text-orange-500" />
                        )}
                        <span className="font-mono">{r.pattern}</span>
                        <span className="opacity-60">→</span>
                        <span className="font-mono">{r.target.trim() ? r.target : '透传原模型号'}</span>
                      </Badge>
                    ))}
                  </div>
                )}
              </CardContent>
            </Card>
          ))}
        </div>
      )}

      <Dialog open={dialogOpen} onOpenChange={setDialogOpen}>
        <DialogContent className="max-h-[90vh] flex flex-col">
          <DialogHeader>
            <DialogTitle>{editingId === null ? '添加中转' : '编辑中转'}</DialogTitle>
            <DialogDescription>
              direct=该模型直接走中转（跳过 Kiro）；fallback=仅 Kiro 账号池整体失败时兜底。无匹配目标则返回原始错误。
            </DialogDescription>
          </DialogHeader>

          <form onSubmit={handleSubmit} className="flex flex-col min-h-0 flex-1">
            <div className="space-y-4 py-2 overflow-y-auto flex-1 pr-1">
              <div className="space-y-1.5">
                <label className="text-sm font-medium">名称 <span className="text-red-500">*</span></label>
                <Input
                  placeholder="如 sub2api-主"
                  value={form.name}
                  onChange={(e) => setForm({ ...form, name: e.target.value })}
                />
              </div>
              <div className="space-y-1.5">
                <label className="text-sm font-medium">Base URL <span className="text-red-500">*</span></label>
                <Input
                  placeholder="https://ai.example.com:2053"
                  value={form.baseUrl}
                  onChange={(e) => setForm({ ...form, baseUrl: e.target.value })}
                />
                <p className="text-xs text-muted-foreground">中转根地址，会自动拼接 /v1/messages 与 /v1/models</p>
              </div>
              <div className="space-y-1.5">
                <label className="text-sm font-medium">
                  API Key {editingId === null && <span className="text-red-500">*</span>}
                </label>
                <Input
                  type="password"
                  placeholder={editingId === null ? 'sk-...' : '留空表示不修改'}
                  value={form.apiKey}
                  onChange={(e) => setForm({ ...form, apiKey: e.target.value })}
                />
              </div>
              <div className="space-y-1.5">
                <label className="text-sm font-medium">计费倍率</label>
                <Input
                  type="number"
                  step="0.1"
                  min="0.1"
                  value={form.billingMultiplier}
                  onChange={(e) => setForm({ ...form, billingMultiplier: e.target.value })}
                />
                <p className="text-xs text-muted-foreground">
                  credits = 中转用量 × GPT官方定价 × 自标定系数 × 倍率
                </p>
                <p className="text-xs text-muted-foreground">
                  1.0 = 与 Kiro 真实 GPT 同价；如 1.5 = 按基准 1.5 倍收（多赚 0.5 倍）
                </p>
              </div>
              <div className="flex items-center justify-between">
                <label className="text-sm font-medium">启用</label>
                <Switch checked={form.enabled} onCheckedChange={(v) => setForm({ ...form, enabled: v })} />
              </div>

              <div className="space-y-2">
                <div className="flex items-center justify-between">
                  <label className="text-sm font-medium">路由规则</label>
                  <Button type="button" size="sm" variant="outline" onClick={addRoute}>
                    <Plus className="h-3.5 w-3.5 mr-1" /> 添加规则
                  </Button>
                </div>
                {modelOptions.length > 0 && (
                  <datalist id="relay-model-options">
                    {modelOptions.map((m) => (
                      <option key={m} value={m} />
                    ))}
                  </datalist>
                )}
                {form.routes.length === 0 && (
                  <p className="text-xs text-muted-foreground">
                    尚无规则。示例：pattern=<span className="font-mono">gpt*</span>，target=中转模型，mode=fallback
                  </p>
                )}
                {form.routes.map((route, idx) => (
                  <div key={idx} className="flex items-center gap-1.5">
                    <Input
                      className="flex-1"
                      placeholder="匹配模型 如 gpt*"
                      value={route.pattern}
                      onChange={(e) => updateRoute(idx, { pattern: e.target.value })}
                    />
                    <Input
                      className="flex-1"
                      list="relay-model-options"
                      placeholder="目标模型（留空=透传原模型号）"
                      value={route.target}
                      onChange={(e) => updateRoute(idx, { target: e.target.value })}
                    />
                    <select
                      className="h-10 rounded-md border border-input bg-background px-2 text-sm"
                      value={route.mode}
                      onChange={(e) => updateRoute(idx, { mode: e.target.value as RouteMode })}
                    >
                      <option value="fallback">兜底</option>
                      <option value="direct">直连</option>
                    </select>
                    <Button
                      type="button"
                      variant="ghost"
                      size="icon"
                      className="h-9 w-9 text-destructive shrink-0"
                      onClick={() => removeRoute(idx)}
                    >
                      <Trash2 className="h-4 w-4" />
                    </Button>
                  </div>
                ))}
              </div>
            </div>

            <DialogFooter className="mt-2">
              <Button type="button" variant="outline" onClick={() => setDialogOpen(false)}>
                取消
              </Button>
              <Button type="submit" disabled={creating || updating}>
                {creating || updating ? '保存中...' : '保存'}
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </div>
  )
}
