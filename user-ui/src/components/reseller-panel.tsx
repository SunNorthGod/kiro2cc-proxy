// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import {
  ArrowLeft, RefreshCw, Plus, Trash2, Copy, Check, Coins, Wallet,
  Power, PowerOff, Loader2, PlusCircle,
} from 'lucide-react'
import {
  getResellerOverview, createSubKey, updateSubKey, topupSubKey, deleteSubKey,
} from '@/api/user'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Badge } from '@/components/ui/badge'
import { Progress } from '@/components/ui/progress'
import { toast } from 'sonner'
import type { SubKey } from '@/types/api'

interface ResellerPanelProps {
  onBack: () => void
}

function formatCredits(n: number) {
  return `${n.toFixed(2)}`
}

function formatDate(iso: string | null) {
  if (!iso) return '永不过期'
  return new Date(iso).toLocaleString('zh-CN', {
    year: 'numeric', month: '2-digit', day: '2-digit',
    hour: '2-digit', minute: '2-digit',
  })
}

export function ResellerPanel({ onBack }: ResellerPanelProps) {
  const qc = useQueryClient()
  const { data, isLoading, isFetching, refetch } = useQuery({
    queryKey: ['resellerOverview'],
    queryFn: getResellerOverview,
    refetchInterval: 30000,
  })

  // 新建子卡密表单
  const [showCreate, setShowCreate] = useState(false)
  const [newName, setNewName] = useState('')
  const [newCredits, setNewCredits] = useState('')
  const [newDays, setNewDays] = useState('')

  const [copiedId, setCopiedId] = useState<number | null>(null)

  const invalidate = () => qc.invalidateQueries({ queryKey: ['resellerOverview'] })

  const createMut = useMutation({
    mutationFn: () =>
      createSubKey({
        name: newName.trim(),
        creditLimit: Number(newCredits),
        durationDays: newDays.trim() ? Number(newDays) : null,
      }),
    onSuccess: () => {
      toast.success('子卡密已创建')
      setShowCreate(false)
      setNewName(''); setNewCredits(''); setNewDays('')
      invalidate()
    },
    onError: (e: unknown) => toast.error(errMsg(e, '创建失败')),
  })

  const toggleMut = useMutation({
    mutationFn: (k: SubKey) => updateSubKey(k.id, { enabled: !k.enabled }),
    onSuccess: () => invalidate(),
    onError: (e: unknown) => toast.error(errMsg(e, '操作失败')),
  })

  const deleteMut = useMutation({
    mutationFn: (id: number) => deleteSubKey(id),
    onSuccess: () => { toast.success('子卡密已删除'); invalidate() },
    onError: (e: unknown) => toast.error(errMsg(e, '删除失败')),
  })

  const handleCopy = (k: SubKey) => {
    navigator.clipboard.writeText(k.key).then(() => {
      setCopiedId(k.id)
      setTimeout(() => setCopiedId(null), 1500)
    })
  }

  const handleTopup = async (k: SubKey) => {
    const input = window.prompt(`为「${k.name}」增加多少额度（credits）？`, '1000')
    if (input == null) return
    const add = Number(input)
    if (!Number.isFinite(add) || add <= 0) { toast.error('请输入正数'); return }
    try {
      await topupSubKey(k.id, { addCredits: add })
      toast.success(`已充值 ${add} credits`)
      invalidate()
    } catch (e) {
      toast.error(errMsg(e, '充值失败'))
    }
  }

  const budget = data?.budget ?? 0
  // 共享额度池占用 = 自己已花 + 已分配给子卡 + 已结算
  const usedOfBudget = data ? data.ownUsed + data.allocated + data.committed : 0
  const budgetPercent = budget > 0 ? Math.min((usedOfBudget / budget) * 100, 100) : 0

  return (
    <div className="min-h-screen bg-background">
      <header className="border-b">
        <div className="max-w-5xl mx-auto px-4 py-4 flex items-center justify-between">
          <div className="flex items-center gap-3">
            <Button variant="ghost" size="icon" onClick={onBack} aria-label="返回">
              <ArrowLeft className="h-4 w-4" />
            </Button>
            <h1 className="text-xl font-semibold">子卡密管理</h1>
          </div>
          <Button variant="ghost" size="icon" onClick={() => refetch()} disabled={isFetching} aria-label="刷新">
            <RefreshCw className={`h-4 w-4 ${isFetching ? 'animate-spin' : ''}`} />
          </Button>
        </div>
      </header>

      <main className="max-w-5xl mx-auto px-4 py-6 space-y-6">
        {isLoading ? (
          <div className="flex justify-center py-20">
            <RefreshCw className="h-8 w-8 animate-spin text-muted-foreground" />
          </div>
        ) : data ? (
          <>
            {/* 预算概览 */}
            <div className="grid gap-4 grid-cols-2 md:grid-cols-5">
              <Card>
                <CardContent className="pt-6">
                  <div className="flex items-center gap-2 text-muted-foreground text-sm mb-1">
                    <Wallet className="h-3.5 w-3.5" />总预算
                  </div>
                  <div className="text-2xl font-bold">{budget ? formatCredits(budget) : '∞'}</div>
                </CardContent>
              </Card>
              <Card>
                <CardContent className="pt-6">
                  <div className="flex items-center gap-2 text-muted-foreground text-sm mb-1">
                    <Coins className="h-3.5 w-3.5" />可分配
                  </div>
                  <div className="text-2xl font-bold text-blue-600 dark:text-blue-400">
                    {formatCredits(data.allocatable)}
                  </div>
                </CardContent>
              </Card>
              <Card>
                <CardContent className="pt-6">
                  <div className="text-muted-foreground text-sm mb-1">自己已用</div>
                  <div className="text-2xl font-bold">{formatCredits(data.ownUsed)}</div>
                  <div className="text-xs text-muted-foreground mt-0.5">本卡直接消耗</div>
                </CardContent>
              </Card>
              <Card>
                <CardContent className="pt-6">
                  <div className="text-muted-foreground text-sm mb-1">已分配</div>
                  <div className="text-2xl font-bold">{formatCredits(data.allocated)}</div>
                  <div className="text-xs text-muted-foreground mt-0.5">子卡密占用</div>
                </CardContent>
              </Card>
              <Card>
                <CardContent className="pt-6">
                  <div className="text-muted-foreground text-sm mb-1">已结算</div>
                  <div className="text-2xl font-bold">{formatCredits(data.committed)}</div>
                  <div className="text-xs text-muted-foreground mt-0.5">已删除子卡密的消耗</div>
                </CardContent>
              </Card>
            </div>

            {budget > 0 && (
              <div className="space-y-1.5">
                <div className="flex justify-between text-sm">
                  <span className="text-muted-foreground">预算占用（自己已用 + 已分配 + 已结算）</span>
                  <span>{formatCredits(usedOfBudget)} / {formatCredits(budget)}</span>
                </div>
                <Progress value={budgetPercent} />
              </div>
            )}

            {/* 新建子卡密 */}
            <Card>
              <CardHeader className="pb-3 flex flex-row items-center justify-between">
                <CardTitle className="text-base font-medium">子卡密（{data.subKeyCount}）</CardTitle>
                <Button size="sm" onClick={() => setShowCreate((v) => !v)}>
                  <Plus className="h-4 w-4 mr-1" />开卡
                </Button>
              </CardHeader>
              {showCreate && (
                <CardContent className="border-t pt-4">
                  <div className="grid gap-3 sm:grid-cols-3">
                    <div className="space-y-1">
                      <label className="text-xs text-muted-foreground">名称</label>
                      <Input value={newName} onChange={(e) => setNewName(e.target.value)} placeholder="如 张三-月付" />
                    </div>
                    <div className="space-y-1">
                      <label className="text-xs text-muted-foreground">额度 (credits)</label>
                      <Input type="number" value={newCredits} onChange={(e) => setNewCredits(e.target.value)} placeholder="1000" />
                    </div>
                    <div className="space-y-1">
                      <label className="text-xs text-muted-foreground">有效期（天，可选）</label>
                      <Input type="number" value={newDays} onChange={(e) => setNewDays(e.target.value)} placeholder="留空=不限" />
                    </div>
                  </div>
                  <div className="mt-3 flex items-center gap-2">
                    <Button
                      size="sm"
                      disabled={!newName.trim() || !(Number(newCredits) > 0) || createMut.isPending}
                      onClick={() => createMut.mutate()}
                    >
                      {createMut.isPending ? <Loader2 className="h-4 w-4 animate-spin mr-1" /> : <PlusCircle className="h-4 w-4 mr-1" />}
                      创建
                    </Button>
                    <Button size="sm" variant="ghost" onClick={() => setShowCreate(false)}>取消</Button>
                    <span className="text-xs text-muted-foreground ml-auto">
                      可分配余额：{formatCredits(data.allocatable)} credits
                    </span>
                  </div>
                </CardContent>
              )}

              <CardContent className={showCreate ? 'border-t pt-4' : ''}>
                {data.subKeys.length === 0 ? (
                  <div className="py-8 text-center text-muted-foreground text-sm">还没有子卡密，点击「开卡」创建</div>
                ) : (
                  <div className="space-y-3">
                    {data.subKeys.map((k) => {
                      const used = k.usedCredits
                      const limit = k.creditLimit ?? 0
                      const pct = limit > 0 ? Math.min((used / limit) * 100, 100) : 0
                      const exhausted = limit > 0 && used >= limit
                      return (
                        <div key={k.id} className="rounded-lg border p-3">
                          <div className="flex items-start justify-between gap-3">
                            <div className="min-w-0 flex-1">
                              <div className="flex items-center gap-2">
                                <span className="font-medium text-sm truncate">{k.name}</span>
                                {!k.enabled && <Badge variant="destructive">已禁用</Badge>}
                                {exhausted && <Badge variant="destructive">额度用完</Badge>}
                              </div>
                              <button
                                className="mt-1 flex items-center gap-1.5 text-xs font-mono text-muted-foreground hover:text-foreground transition-colors"
                                onClick={() => handleCopy(k)}
                                title="点击复制"
                              >
                                {copiedId === k.id ? <Check className="h-3 w-3 text-green-500" /> : <Copy className="h-3 w-3" />}
                                {k.key}
                              </button>
                              <div className="text-xs text-muted-foreground mt-1">到期：{formatDate(k.expiresAt)}</div>
                            </div>
                            <div className="flex items-center gap-1 shrink-0">
                              <Button variant="ghost" size="icon" title="充值额度" onClick={() => handleTopup(k)}>
                                <Coins className="h-4 w-4" />
                              </Button>
                              <Button
                                variant="ghost" size="icon"
                                title={k.enabled ? '禁用' : '启用'}
                                onClick={() => toggleMut.mutate(k)}
                                disabled={toggleMut.isPending}
                              >
                                {k.enabled ? <PowerOff className="h-4 w-4" /> : <Power className="h-4 w-4 text-green-600" />}
                              </Button>
                              <Button
                                variant="ghost" size="icon"
                                title="删除"
                                onClick={() => {
                                  if (window.confirm(`确定删除「${k.name}」？已消耗的 ${formatCredits(used)} credits 会计入你的预算，未用完的额度将释放。`)) {
                                    deleteMut.mutate(k.id)
                                  }
                                }}
                                disabled={deleteMut.isPending}
                              >
                                <Trash2 className="h-4 w-4 text-destructive" />
                              </Button>
                            </div>
                          </div>
                          <div className="mt-2 space-y-1">
                            <div className="flex justify-between text-xs text-muted-foreground">
                              <span>已用 {formatCredits(used)} / {limit > 0 ? formatCredits(limit) : '∞'} credits</span>
                              <span>{k.activatedAt ? '已激活' : '待激活'}</span>
                            </div>
                            {limit > 0 && <Progress value={pct} />}
                          </div>
                        </div>
                      )
                    })}
                  </div>
                )}
              </CardContent>
            </Card>
          </>
        ) : (
          <Card>
            <CardContent className="py-12 text-center text-muted-foreground">加载失败</CardContent>
          </Card>
        )}
      </main>
    </div>
  )
}

function errMsg(e: unknown, fallback: string): string {
  const ax = e as { response?: { data?: { error?: string } } }
  return ax.response?.data?.error || fallback
}
