// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState, type ReactNode } from 'react'
import { Activity, Gauge, Coins, PiggyBank, Database, Layers, ServerCog } from 'lucide-react'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { useOverview, useRpm, useApiKeys, useCredentials } from '@/hooks/use-credentials'
import type { DailySummary } from '@/types/api'

// ============ 数值格式化（中文单位：万 / 亿）============
function fmtInt(n: number): string {
  if (!isFinite(n)) return '-'
  const a = Math.abs(n)
  if (a >= 1e8) return (n / 1e8).toFixed(2) + '亿'
  if (a >= 1e4) return (n / 1e4).toFixed(2) + '万'
  return String(Math.round(n))
}
function fmtCredits(n: number): string {
  if (!isFinite(n)) return '-'
  const a = Math.abs(n)
  if (a >= 1e8) return (n / 1e8).toFixed(2) + '亿'
  if (a >= 1e4) return (n / 1e4).toFixed(2) + '万'
  return n.toFixed(1)
}

const MODEL_COLORS = ['#8b5cf6', '#3b82f6', '#10b981', '#f59e0b', '#ef4444', '#ec4899', '#14b8a6', '#6366f1']

// 把最大值向上取整到"好看"的刻度（1/2/5 × 10^k），让 Y 轴网格线数值整齐
function niceCeil(v: number): number {
  if (v <= 0) return 1
  const p = Math.pow(10, Math.floor(Math.log10(v)))
  const f = v / p
  const nf = f <= 1 ? 1 : f <= 2 ? 2 : f <= 5 ? 5 : 10
  return nf * p
}

// ============ 内联 SVG 折线图（带网格线 / Y 轴刻度 / 渐变 / 悬停提示）============
function LineChart({
  points,
  height = 210,
  color = '#8b5cf6',
  format = fmtCredits,
}: {
  points: { label: string; value: number }[]
  height?: number
  color?: string
  format?: (n: number) => string
}) {
  if (points.length === 0) {
    return <div className="text-sm text-muted-foreground py-16 text-center">暂无数据</div>
  }
  const w = 760
  const h = height
  const padL = 52
  const padR = 14
  const padT = 14
  const padB = 26
  const rawMax = Math.max(...points.map((p) => p.value), 1)
  const max = niceCeil(rawMax)
  const n = points.length
  const x = (i: number) => padL + (n === 1 ? (w - padL - padR) / 2 : (i / (n - 1)) * (w - padL - padR))
  const y = (v: number) => padT + (1 - v / max) * (h - padT - padB)
  const linePts = points.map((p, i) => `${x(i).toFixed(1)},${y(p.value).toFixed(1)}`).join(' ')
  const areaPts = `${x(0).toFixed(1)},${(h - padB).toFixed(1)} ${linePts} ${x(n - 1).toFixed(1)},${(h - padB).toFixed(1)}`
  const gridN = 4
  const gridVals = Array.from({ length: gridN + 1 }, (_, k) => (max / gridN) * k)
  const labelIdx = n <= 1 ? [0] : n <= 8 ? points.map((_, i) => i) : [0, Math.floor((n - 1) / 2), n - 1]
  const gid = 'ovgrad-' + color.replace(/[^a-z0-9]/gi, '')
  return (
    <svg viewBox={`0 0 ${w} ${h}`} className="w-full" style={{ height }}>
      <defs>
        <linearGradient id={gid} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor={color} stopOpacity="0.28" />
          <stop offset="100%" stopColor={color} stopOpacity="0" />
        </linearGradient>
      </defs>
      {/* 水平网格线 + Y 轴刻度 */}
      {gridVals.map((v, k) => (
        <g key={k}>
          <line
            x1={padL}
            y1={y(v)}
            x2={w - padR}
            y2={y(v)}
            stroke="currentColor"
            strokeOpacity={0.08}
            strokeWidth={1}
          />
          <text x={padL - 8} y={y(v) + 3.5} fontSize={10.5} textAnchor="end" fill="currentColor" className="text-muted-foreground">
            {format(v)}
          </text>
        </g>
      ))}
      {/* 面积 + 折线 */}
      <polygon points={areaPts} fill={`url(#${gid})`} />
      <polyline
        points={linePts}
        fill="none"
        stroke={color}
        strokeWidth={2.5}
        strokeLinejoin="round"
        strokeLinecap="round"
      />
      {/* 数据点（带原生悬停提示）*/}
      {n <= 31 &&
        points.map((p, i) => (
          <g key={i}>
            <title>{`${p.label} · ${format(p.value)}`}</title>
            <circle cx={x(i)} cy={y(p.value)} r={6} fill="transparent" />
            <circle cx={x(i)} cy={y(p.value)} r={3} fill={color} stroke="hsl(var(--card))" strokeWidth={1.5} />
          </g>
        ))}
      {/* X 轴日期 */}
      {labelIdx.map((i) => (
        <text
          key={i}
          x={x(i)}
          y={h - 8}
          fontSize={10.5}
          fill="currentColor"
          className="text-muted-foreground"
          textAnchor={i === 0 ? 'start' : i === n - 1 ? 'end' : 'middle'}
        >
          {points[i]?.label?.slice(5) /* MM-DD */}
        </text>
      ))}
    </svg>
  )
}

// ============ 横向条形列表 ============
function BarList({
  items,
  colorFor,
}: {
  items: { label: string; value: number; sub?: string }[]
  colorFor?: (i: number) => string
}) {
  if (items.length === 0) {
    return <div className="text-sm text-muted-foreground py-8 text-center">暂无数据</div>
  }
  const max = Math.max(...items.map((i) => i.value), 1)
  return (
    <div className="space-y-2.5">
      {items.map((it, i) => (
        <div key={i} className="space-y-1">
          <div className="flex items-center justify-between text-xs">
            <span className="truncate font-medium">{it.label}</span>
            <span className="text-muted-foreground whitespace-nowrap ml-2">
              {fmtCredits(it.value)}{it.sub ? ` · ${it.sub}` : ''}
            </span>
          </div>
          <div className="h-2 rounded-full bg-muted overflow-hidden">
            <div
              className="h-full rounded-full"
              style={{ width: `${(it.value / max) * 100}%`, background: colorFor ? colorFor(i) : 'hsl(var(--primary))' }}
            />
          </div>
        </div>
      ))}
    </div>
  )
}

// ============ KPI 卡片 ============
function Kpi({ icon, label, value, sub, accent }: { icon: ReactNode; label: string; value: string; sub?: string; accent?: string }) {
  return (
    <Card>
      <CardHeader className="pb-1.5">
        <CardTitle className="text-xs font-medium text-muted-foreground flex items-center gap-1.5">
          <span className={accent}>{icon}</span>
          {label}
        </CardTitle>
      </CardHeader>
      <CardContent>
        <div className={`text-2xl font-bold ${accent ?? ''}`}>{value}</div>
        {sub && <div className="text-[11px] text-muted-foreground mt-0.5">{sub}</div>}
      </CardContent>
    </Card>
  )
}

type Metric = 'credits' | 'requests' | 'tokens'
type Range = 7 | 30

export function OverviewPage() {
  const { data: overview, isLoading } = useOverview()
  const { data: rpm } = useRpm()
  const { data: apiKeys } = useApiKeys()
  const { data: creds } = useCredentials()

  const [metric, setMetric] = useState<Metric>('credits')
  const [range, setRange] = useState<Range>(7)

  const daily: DailySummary[] = overview?.daily ?? []
  const sliced = daily.slice(Math.max(0, daily.length - range))

  const metricValue = (d: DailySummary) => {
    if (metric === 'requests') return d.totalRequests
    if (metric === 'tokens') return (d.totalInputTokens ?? 0) + (d.totalOutputTokens ?? 0)
    return d.totalCredits
  }
  const linePoints = sliced.map((d) => ({ label: d.date, value: metricValue(d) }))

  const at = overview?.allTime
  const totalTokens = at ? at.totalInputTokens + at.totalOutputTokens : 0

  // sticky 缓存命中率
  const hits = rpm?.stickyHits ?? 0
  const misses = rpm?.stickyMisses ?? 0
  const hitRate = hits + misses > 0 ? (hits / (hits + misses)) * 100 : null

  const keyName = (id: number) => apiKeys?.find((k) => k.id === id)?.name ?? `Key #${id}`

  const metricColor = metric === 'requests' ? '#3b82f6' : metric === 'tokens' ? '#10b981' : '#8b5cf6'

  return (
    <div>
      <div className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-[22px] font-bold tracking-[-0.02em]">概览</h1>
          <p className="text-[13px] text-muted-foreground mt-0.5">实时速率与历史用量总览</p>
        </div>
      </div>

      {/* KPI 卡片行 */}
      <div className="grid gap-3 grid-cols-2 md:grid-cols-4 mb-4">
        <Kpi icon={<Activity className="h-3.5 w-3.5" />} accent="text-blue-600 dark:text-blue-400"
          label="实时 RPM" value={String(rpm?.global ?? 0)} sub="最近 60 秒请求数" />
        <Kpi icon={<Gauge className="h-3.5 w-3.5" />} accent="text-emerald-600 dark:text-emerald-400"
          label="实时 TPM" value={fmtInt(rpm?.tokensPerMin ?? 0)} sub="最近 60 秒 token/分" />
        <Kpi icon={<Coins className="h-3.5 w-3.5" />} accent="text-orange-600 dark:text-orange-400"
          label="累计消耗积分" value={fmtCredits(at?.totalCredits ?? 0)} sub="全历史" />
        <Kpi icon={<PiggyBank className="h-3.5 w-3.5" />} accent="text-green-600 dark:text-green-400"
          label="累计节省积分" value={fmtCredits(at?.totalCreditsSaved ?? 0)} sub="缓存节省" />
        <Kpi icon={<Layers className="h-3.5 w-3.5" />} accent="text-purple-600 dark:text-purple-400"
          label="累计 Token" value={fmtInt(totalTokens)}
          sub={at ? `入 ${fmtInt(at.totalInputTokens)} / 出 ${fmtInt(at.totalOutputTokens)}` : undefined} />
        <Kpi icon={<Database className="h-3.5 w-3.5" />} accent="text-cyan-600 dark:text-cyan-400"
          label="缓存读取 Token" value={fmtInt(at?.totalCacheReadTokens ?? 0)} sub="全历史命中缓存量" />
        <Kpi icon={<Database className="h-3.5 w-3.5" />} accent="text-teal-600 dark:text-teal-400"
          label="缓存命中率" value={hitRate === null ? '-' : `${hitRate.toFixed(1)}%`} sub="账号路由 sticky 命中" />
        <Kpi icon={<ServerCog className="h-3.5 w-3.5" />} accent="text-foreground"
          label="健康账号" value={`${creds?.available ?? 0}/${creds?.total ?? 0}`} sub="可用 / 总数" />
      </div>

      {/* 趋势折线图 */}
      <Card className="mb-4">
        <CardHeader className="pb-2">
          <div className="flex items-center justify-between flex-wrap gap-2">
            <CardTitle className="text-sm font-medium">用量趋势</CardTitle>
            <div className="flex items-center gap-3">
              <div className="flex items-center gap-1 text-xs">
                {(['credits', 'requests', 'tokens'] as Metric[]).map((m) => (
                  <button key={m} onClick={() => setMetric(m)}
                    className={`px-2 py-1 rounded-md transition-colors ${metric === m ? 'bg-secondary text-foreground font-medium' : 'text-muted-foreground hover:text-foreground'}`}>
                    {m === 'credits' ? '积分' : m === 'requests' ? '请求' : 'Token'}
                  </button>
                ))}
              </div>
              <div className="flex items-center gap-1 text-xs">
                {([7, 30] as Range[]).map((r) => (
                  <button key={r} onClick={() => setRange(r)}
                    className={`px-2 py-1 rounded-md transition-colors ${range === r ? 'bg-secondary text-foreground font-medium' : 'text-muted-foreground hover:text-foreground'}`}>
                    {r}天
                  </button>
                ))}
              </div>
            </div>
          </div>
        </CardHeader>
        <CardContent>
          {isLoading ? (
            <div className="text-sm text-muted-foreground py-12 text-center">加载中...</div>
          ) : (
            <LineChart points={linePoints} color={metricColor} format={metric === 'credits' ? fmtCredits : fmtInt} />
          )}
        </CardContent>
      </Card>

      {/* 模型分布 + Top API Key */}
      <div className="grid gap-4 md:grid-cols-2">
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium">模型分布（按积分）</CardTitle>
          </CardHeader>
          <CardContent>
            <BarList
              colorFor={(i) => MODEL_COLORS[i % MODEL_COLORS.length]}
              items={(overview?.byModel ?? []).slice(0, 8).map((m) => ({
                label: m.model,
                value: m.credits,
                sub: `${fmtInt(m.requests)} 次`,
              }))}
            />
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium">Top 消耗 API Key</CardTitle>
          </CardHeader>
          <CardContent>
            <BarList
              items={(overview?.byApiKey ?? []).slice(0, 8).map((k) => ({
                label: keyName(k.apiKeyId),
                value: k.credits,
                sub: `${fmtInt(k.requests)} 次`,
              }))}
            />
          </CardContent>
        </Card>
      </div>
    </div>
  )
}
