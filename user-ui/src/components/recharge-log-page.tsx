// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { ArrowLeft, RefreshCw, ChevronLeft, ChevronRight } from 'lucide-react'
import { getRechargeRecords, getSubKeyRecharges } from '@/api/user'
import { Card, CardContent } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'

interface RechargeLogPageProps {
  onBack: () => void
  /** 传入则查询该子卡密的充值流水（分销商），否则查询本卡 */
  subKeyId?: number
  title?: string
}

function formatDate(iso: string) {
  return new Date(iso).toLocaleString('zh-CN', {
    year: 'numeric', month: '2-digit', day: '2-digit',
    hour: '2-digit', minute: '2-digit', second: '2-digit',
  })
}

function formatNum(n: number) {
  return n.toLocaleString('zh-CN')
}

export function RechargeLogPage({ onBack, subKeyId, title }: RechargeLogPageProps) {
  const [page, setPage] = useState(1)
  const pageSize = 50

  const { data, isLoading, isFetching, refetch } = useQuery({
    queryKey: ['rechargeRecords', subKeyId ?? 'self', page, pageSize],
    queryFn: () =>
      subKeyId != null
        ? getSubKeyRecharges(subKeyId, page, pageSize)
        : getRechargeRecords(page, pageSize),
  })

  const records = data?.records ?? []

  return (
    <div className="min-h-screen bg-background">
      <header className="border-b">
        <div className="max-w-4xl mx-auto px-4 py-4 flex items-center justify-between">
          <div className="flex items-center gap-3">
            <Button variant="ghost" size="icon" onClick={onBack} aria-label="返回">
              <ArrowLeft className="h-4 w-4" />
            </Button>
            <h1 className="text-xl font-semibold">{title ?? '充值记录'}</h1>
            {data && <span className="text-sm text-muted-foreground">共 {data.total} 条</span>}
          </div>
          <Button variant="ghost" size="icon" onClick={() => refetch()} disabled={isFetching} aria-label="刷新">
            <RefreshCw className={`h-4 w-4 ${isFetching ? 'animate-spin' : ''}`} />
          </Button>
        </div>
      </header>

      <main className="max-w-4xl mx-auto px-4 py-6 space-y-4">
        {isLoading ? (
          <div className="flex justify-center py-20">
            <RefreshCw className="h-8 w-8 animate-spin text-muted-foreground" />
          </div>
        ) : !data || data.total === 0 ? (
          <Card>
            <CardContent className="py-12 text-center text-muted-foreground">
              暂无充值记录
            </CardContent>
          </Card>
        ) : (
          <Card>
            <CardContent className="p-0">
              <div className="overflow-x-auto">
                <table className="w-full text-sm">
                  <thead>
                    <tr className="border-b bg-muted/50">
                      <th className="text-left px-4 py-3 font-medium text-muted-foreground">时间</th>
                      <th className="text-left px-4 py-3 font-medium text-muted-foreground">类型</th>
                      <th className="text-right px-4 py-3 font-medium text-muted-foreground">增加额度</th>
                      <th className="text-right px-4 py-3 font-medium text-muted-foreground">增加时长</th>
                      <th className="text-right px-4 py-3 font-medium text-muted-foreground">充值后额度</th>
                      <th className="text-left px-4 py-3 font-medium text-muted-foreground">来源</th>
                    </tr>
                  </thead>
                  <tbody>
                    {records.map((r, i) => (
                      <tr key={i} className="border-b last:border-0 hover:bg-muted/30 transition-colors">
                        <td className="px-4 py-3 text-xs text-muted-foreground whitespace-nowrap">
                          {formatDate(r.createdAt)}
                        </td>
                        <td className="px-4 py-3 text-xs">
                          <Badge variant={r.kind === 'create' ? 'secondary' : 'success'}>
                            {r.kind === 'create' ? '开卡' : '充值'}
                          </Badge>
                        </td>
                        <td className="px-4 py-3 text-right tabular-nums font-medium text-blue-600 dark:text-blue-400">
                          {r.addCredits != null ? `+${formatNum(r.addCredits)}` : '—'}
                        </td>
                        <td className="px-4 py-3 text-right tabular-nums">
                          {r.addDays != null ? `+${r.addDays} 天` : '—'}
                        </td>
                        <td className="px-4 py-3 text-right tabular-nums text-muted-foreground">
                          {r.creditLimitAfter != null ? formatNum(r.creditLimitAfter) : '∞'}
                        </td>
                        <td className="px-4 py-3 text-xs text-muted-foreground">
                          {r.source === 'reseller' ? '分销商' : '管理员'}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>

              {data.totalPages > 1 && (
                <div className="flex items-center justify-between px-4 py-3 border-t">
                  <span className="text-sm text-muted-foreground">
                    第 {data.page} / {data.totalPages} 页
                  </span>
                  <div className="flex items-center gap-2">
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => setPage((p) => Math.max(1, p - 1))}
                      disabled={data.page <= 1 || isFetching}
                    >
                      <ChevronLeft className="h-4 w-4" />
                    </Button>
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => setPage((p) => Math.min(data.totalPages, p + 1))}
                      disabled={data.page >= data.totalPages || isFetching}
                    >
                      <ChevronRight className="h-4 w-4" />
                    </Button>
                  </div>
                </div>
              )}
            </CardContent>
          </Card>
        )}
      </main>
    </div>
  )
}
