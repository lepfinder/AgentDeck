import React, { useState } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import {
  X,
  Copy,
  Check,
  Calendar,
  Layers,
  Sparkles,
  ChevronRight,
  Maximize2,
  Minimize2,
} from 'lucide-react';
import type { ArtifactItem } from '../../types';

interface Props {
  isOpen: boolean;
  onClose: () => void;
  artifacts: ArtifactItem[];
  currentArtifact: ArtifactItem | null;
  onSelectArtifact: (art: ArtifactItem) => void;
}

export const ArtifactsModal: React.FC<Props> = ({
  isOpen,
  onClose,
  artifacts,
  currentArtifact,
  onSelectArtifact,
}) => {
  const [copied, setCopied] = useState(false);
  const [isFullScreen, setIsFullScreen] = useState(false);

  if (!isOpen || artifacts.length === 0) return null;

  const active = currentArtifact || artifacts[0];

  const handleCopy = () => {
    if (!active?.content) return;
    navigator.clipboard.writeText(active.content);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const getDocTypeBadge = (fileName: string) => {
    if (fileName.startsWith('implementation_plan')) {
      return (
        <span className="px-2 py-0.5 rounded text-[10px] font-bold bg-emerald-500/15 text-emerald-600 dark:text-emerald-400 border border-emerald-500/30">
          实施计划
        </span>
      );
    }
    if (fileName.startsWith('walkthrough')) {
      return (
        <span className="px-2 py-0.5 rounded text-[10px] font-bold bg-purple-500/15 text-purple-600 dark:text-purple-400 border border-purple-500/30">
          复盘走查
        </span>
      );
    }
    if (fileName.startsWith('task')) {
      return (
        <span className="px-2 py-0.5 rounded text-[10px] font-bold bg-blue-500/15 text-blue-600 dark:text-blue-400 border border-blue-500/30">
          任务清单
        </span>
      );
    }
    return (
      <span className="px-2 py-0.5 rounded text-[10px] font-bold bg-slate-500/15 text-slate-600 dark:text-slate-400 border border-slate-500/30">
        技术文档
      </span>
    );
  };

  const getVersionBadge = (fileName: string) => {
    const match = fileName.match(/\.v(\d+)\.md$/);
    if (match) {
      return (
        <span className="px-1.5 py-0.5 rounded text-[9.5px] font-mono font-bold bg-slate-500/15 text-slate-600 dark:text-slate-300 border border-slate-500/25">
          v{match[1]}
        </span>
      );
    }
    const isMultiVersion = artifacts.some((a) =>
      a.file_name.startsWith(fileName.replace('.md', '.v'))
    );
    if (isMultiVersion) {
      return (
        <span className="px-1.5 py-0.5 rounded text-[9.5px] font-mono font-bold bg-emerald-500/20 text-emerald-600 dark:text-emerald-300 border border-emerald-500/35">
          最新
        </span>
      );
    }
    return null;
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-xs p-4 sm:p-6 transition-opacity animate-in fade-in duration-200">
      <div
        className={`flex flex-col theme-bg-card border theme-border rounded-2xl shadow-2xl overflow-hidden transition-all duration-300 ${
          isFullScreen
            ? 'w-full h-full'
            : 'w-full max-w-6xl h-[88vh] max-h-[920px]'
        }`}
      >
        {/* 顶部标题栏 */}
        <div className="flex items-center justify-between px-6 py-4 border-b theme-border-sub bg-slate-500/5 select-none">
          <div className="flex items-center gap-2.5">
            <div className="w-8 h-8 rounded-lg bg-emerald-500/15 text-emerald-500 flex items-center justify-center">
              <Layers className="h-4 w-4" />
            </div>
            <div>
              <div className="flex items-center gap-2">
                <h3 className="text-sm font-bold theme-text-main">
                  实施方案与产物文档 (Artifacts)
                </h3>
                <span className="px-2 py-0.5 text-[11px] font-semibold rounded-full bg-emerald-500/15 text-emerald-600 dark:text-emerald-400">
                  {artifacts.length} 篇文档
                </span>
              </div>
              <p className="text-[11px] theme-text-sub mt-0.5">
                浏览智能体在规划与研发过程中生成的架构设计、实施方案与交付成果
              </p>
            </div>
          </div>

          <div className="flex items-center gap-2">
            <button
              onClick={() => setIsFullScreen(!isFullScreen)}
              className="p-2 rounded-lg theme-text-sub hover:theme-text-main theme-bg-sub border theme-border transition-colors cursor-pointer"
              title={isFullScreen ? '还原' : '全屏'}
            >
              {isFullScreen ? (
                <Minimize2 className="h-4 w-4" />
              ) : (
                <Maximize2 className="h-4 w-4" />
              )}
            </button>
            <button
              onClick={onClose}
              className="p-2 rounded-lg theme-text-sub hover:theme-text-main theme-bg-sub border theme-border transition-colors cursor-pointer"
              title="关闭 (Esc)"
            >
              <X className="h-4 w-4" />
            </button>
          </div>
        </div>

        {/* 内容双栏区域 */}
        <div className="flex-1 flex overflow-hidden">
          {/* 左侧文档索引列表 */}
          <div className="w-80 sm:w-88 border-r theme-border-sub flex flex-col theme-bg-sub shrink-0">
            <div className="px-4 py-3 border-b theme-border-sub text-[11px] font-bold tracking-wider theme-text-sub uppercase flex items-center justify-between">
              <span>文档索引列表</span>
              <span className="text-[10px] font-normal">{artifacts.length} 项</span>
            </div>

            <div className="flex-1 overflow-y-auto p-3 space-y-2">
              {artifacts.map((art) => {
                const isSelected = active?.file_name === art.file_name;
                return (
                  <div
                    key={art.file_name}
                    onClick={() => onSelectArtifact(art)}
                    className={`p-3 rounded-xl border transition-all cursor-pointer group ${
                      isSelected
                        ? 'bg-emerald-500/10 border-emerald-500/40 shadow-xs'
                        : 'theme-bg-card hover:bg-slate-500/5 theme-border border-transparent'
                    }`}
                  >
                    <div className="flex items-center justify-between gap-1.5 mb-1.5">
                      <div className="flex items-center gap-1.5">
                        {getDocTypeBadge(art.file_name)}
                        {getVersionBadge(art.file_name)}
                      </div>
                      {art.request_feedback && (
                        <span className="px-1.5 py-0.5 rounded text-[9.5px] font-semibold bg-amber-500/15 text-amber-600 dark:text-amber-400">
                          需审批
                        </span>
                      )}
                    </div>

                    <div className="font-semibold text-xs theme-text-main line-clamp-1 group-hover:text-emerald-500 transition-colors">
                      {art.title || art.file_name}
                    </div>

                    <div className="text-[10.5px] font-mono theme-text-sub mt-0.5 truncate">
                      {art.file_name}
                    </div>

                    {art.summary && (
                      <p className="text-[11px] leading-relaxed text-slate-500 dark:text-slate-400 line-clamp-2 mt-1.5">
                        {art.summary}
                      </p>
                    )}

                    <div className="flex items-center justify-between text-[10px] theme-text-muted mt-2 pt-2 border-t theme-border-sub">
                      <span className="flex items-center gap-1">
                        <Calendar className="h-3 w-3" />
                        {art.updated_at ? art.updated_at.slice(0, 16).replace('T', ' ') : '-'}
                      </span>
                      <ChevronRight
                        className={`h-3.5 w-3.5 transition-transform ${
                          isSelected ? 'text-emerald-500 translate-x-0.5' : 'theme-text-muted'
                        }`}
                      />
                    </div>
                  </div>
                );
              })}
            </div>
          </div>

          {/* 右侧主阅读与 Markdown 渲染区 */}
          <div className="flex-1 flex flex-col overflow-hidden theme-bg-card">
            {active ? (
              <>
                {/* 阅读器工具条 */}
                <div className="flex items-center justify-between px-6 py-3 border-b theme-border-sub bg-slate-500/5">
                  <div className="flex-1 min-w-0 pr-4">
                    <div className="flex items-center gap-2">
                      <h4 className="font-bold text-sm theme-text-main truncate">
                        {active.title || active.file_name}
                      </h4>
                      {getDocTypeBadge(active.file_name)}
                      {getVersionBadge(active.file_name)}
                    </div>
                    <div className="flex items-center gap-3 text-[11px] theme-text-sub font-mono mt-0.5">
                      <span>{active.file_name}</span>
                      <span>·</span>
                      <span>{active.content.length} 字符</span>
                    </div>
                  </div>

                  <div className="flex items-center gap-2">
                    <button
                      onClick={handleCopy}
                      className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg border theme-border theme-bg-sub hover:opacity-80 text-xs font-medium theme-text-main transition-all cursor-pointer shadow-xs"
                    >
                      {copied ? (
                        <>
                          <Check className="h-3.5 w-3.5 text-emerald-500" />
                          <span className="text-emerald-500 font-semibold">已复制</span>
                        </>
                      ) : (
                        <>
                          <Copy className="h-3.5 w-3.5 theme-text-sub" />
                          <span>复制 Markdown</span>
                        </>
                      )}
                    </button>
                  </div>
                </div>

                {/* 摘要 Banner（若有） */}
                {active.summary && (
                  <div className="mx-6 mt-4 p-3.5 rounded-xl bg-emerald-500/10 border border-emerald-500/25 flex items-start gap-2.5">
                    <Sparkles className="h-4 w-4 text-emerald-500 shrink-0 mt-0.5" />
                    <div className="text-xs leading-relaxed text-emerald-950 dark:text-emerald-200">
                      <span className="font-bold mr-1.5">方案摘要:</span>
                      {active.summary}
                    </div>
                  </div>
                )}

                {/* Markdown 正文滚动区域 */}
                <div className="flex-1 overflow-y-auto px-6 py-5">
                  <div className="max-w-4xl mx-auto markdown-body select-text text-xs leading-relaxed">
                    <ReactMarkdown remarkPlugins={[remarkGfm]}>
                      {active.content}
                    </ReactMarkdown>
                  </div>
                </div>
              </>
            ) : (
              <div className="flex-1 flex items-center justify-center theme-text-sub text-xs">
                请从左侧选择需要查看的文档
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};
