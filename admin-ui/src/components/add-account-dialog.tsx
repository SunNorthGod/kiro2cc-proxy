// Copyright (c) 2026 Harllan He. Licensed under MIT.
import { useState } from 'react'
import { UserPlus, KeyRound, FileUp, LogIn, Globe } from 'lucide-react'
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { cn } from '@/lib/utils'
import { ManualAddPanel } from '@/components/panels/manual-add-panel'
import { ApiKeyPanel } from '@/components/panels/api-key-panel'
import { BatchImportPanel } from '@/components/panels/batch-import-panel'
import { SsoLoginPanel } from '@/components/panels/sso-login-panel'
import { SocialLoginPanel } from '@/components/panels/social-login-panel'

interface AddAccountDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

type Tab = 'manual' | 'apikey' | 'batch' | 'sso' | 'social'

const TABS: { id: Tab; label: string; icon: typeof UserPlus }[] = [
  { id: 'manual', label: '手动', icon: UserPlus },
  { id: 'apikey', label: 'API Key', icon: KeyRound },
  { id: 'batch', label: '批量导入', icon: FileUp },
  { id: 'sso', label: 'SSO 登录', icon: LogIn },
  { id: 'social', label: 'Social 登录', icon: Globe },
]

/** 统一的「添加账号」入口：手动 / API Key / 批量导入 / SSO 登录 四个标签页。 */
export function AddAccountDialog({ open, onOpenChange }: AddAccountDialogProps) {
  const [tab, setTab] = useState<Tab>('manual')

  const close = () => onOpenChange(false)

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-2xl max-h-[85vh] flex flex-col">
        <DialogHeader>
          <DialogTitle>添加账号</DialogTitle>
        </DialogHeader>

        {/* 标签页切换 */}
        <div className="flex gap-1 rounded-lg bg-muted p-1">
          {TABS.map(({ id, label, icon: Icon }) => (
            <button
              key={id}
              type="button"
              onClick={() => setTab(id)}
              className={cn(
                'flex flex-1 items-center justify-center gap-1.5 rounded-md px-2 py-1.5 text-sm font-medium transition-colors',
                tab === id
                  ? 'bg-background text-foreground shadow-sm'
                  : 'text-muted-foreground hover:text-foreground'
              )}
            >
              <Icon className="h-4 w-4" />
              <span className="hidden sm:inline">{label}</span>
            </button>
          ))}
        </div>

        {/* 面板内容：用 key 强制切换时重挂载，重置各自状态 */}
        {tab === 'manual' && <ManualAddPanel key="manual" onClose={close} />}
        {tab === 'apikey' && <ApiKeyPanel key="apikey" onClose={close} />}
        {tab === 'batch' && <BatchImportPanel key="batch" onClose={close} />}
        {tab === 'sso' && <SsoLoginPanel key="sso" onClose={close} />}
        {tab === 'social' && <SocialLoginPanel key="social" onClose={close} />}
      </DialogContent>
    </Dialog>
  )
}
