// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { ArrowLeft, RefreshCw } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import { useDailyUsage } from '@/hooks/use-credentials'

interface DailyStatsPageProps {
  onBack: () => void
  onViewDay: (date: string) => void
  showBack?: boolean
}

function formatDate(dateStr: string): string {
  return new Date(dateStr + 'T00:00:00+08:00').toLocaleDateString('zh-CN', {
    year: 'numeric', month: '2-digit', day: '2-digit', timeZone: 'Asia/Shanghai',
  })
}

// 中文单位：万 / 亿
function fmtCn(n: number): string {
  if (!isFinite(n)) return '-'
  const a = Math.abs(n)
  if (a >= 1e8) return (n / 1e8).toFixed(2) + '亿'
  if (a >= 1e4) return (n / 1e4).toFixed(2) + '万'
  return String(Math.round(n))
}
function fmtCreditsCn(n: number): string {
  if (!isFinite(n)) return '-'
  const a = Math.abs(n)
  if (a >= 1e8) return (n / 1e8).toFixed(2) + '亿'
  if (a >= 1e4) return (n / 1e4).toFixed(2) + '万'
  return n.toFixed(2)
}

export function DailyStatsPage({ onBack, onViewDay, showBack = true }: DailyStatsPageProps) {
  const { data, isLoading, refetch } = useDailyUsage()

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-3">
        {showBack && (
          <Button variant="ghost" size="sm" onClick={onBack} className="gap-1">
            <ArrowLeft className="h-4 w-4" />
            返回
          </Button>
        )}
        <h2 className="text-xl font-semibold">每日用量统计</h2>
        <Button variant="ghost" size="sm" onClick={() => refetch()} disabled={isLoading} className="ml-auto">
          <RefreshCw className={`h-4 w-4 ${isLoading ? 'animate-spin' : ''}`} />
        </Button>
      </div>

      <Card>
        <CardContent className="p-0">
          {isLoading ? (
            <div className="py-8 text-center text-muted-foreground text-sm">加载中...</div>
          ) : !data || data.length === 0 ? (
            <div className="py-8 text-center text-muted-foreground text-sm">暂无用量记录</div>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b bg-muted/50">
                    <th className="text-left px-4 py-2 font-medium text-muted-foreground">日期</th>
                    <th className="text-right px-4 py-2 font-medium text-muted-foreground">请求数</th>
                    <th className="text-right px-4 py-2 font-medium text-muted-foreground">Token</th>
                    <th className="text-right px-4 py-2 font-medium text-muted-foreground">积分</th>
                  </tr>
                </thead>
                <tbody>
                  {data.map((row) => {
                    const inTok = row.totalInputTokens ?? 0
                    const outTok = row.totalOutputTokens ?? 0
                    return (
                    <tr
                      key={row.date}
                      className="border-b last:border-0 hover:bg-muted/30 transition-colors cursor-pointer"
                      onClick={() => onViewDay(row.date)}
                    >
                      <td className="px-4 py-2 font-medium">{formatDate(row.date)}</td>
                      <td className="px-4 py-2 text-right tabular-nums">{fmtCn(row.totalRequests)}</td>
                      <td className="px-4 py-2 text-right tabular-nums text-purple-600 dark:text-purple-400">
                        <div>{fmtCn(inTok + outTok)}</div>
                        <div className="text-xs text-muted-foreground">入 {fmtCn(inTok)} / 出 {fmtCn(outTok)}</div>
                      </td>
                      <td className="px-4 py-2 text-right tabular-nums font-medium text-blue-600 dark:text-blue-400">
                        <div>{fmtCreditsCn(row.totalCredits)}</div>
                        {row.totalCreditsSaved != null && row.totalCreditsSaved > 0 && (
                          <div className="text-xs text-green-600 dark:text-green-400">
                            省 {fmtCreditsCn(row.totalCreditsSaved)}
                          </div>
                        )}
                      </td>
                    </tr>
                    )
                  })}
                </tbody>
              </table>
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  )
}
