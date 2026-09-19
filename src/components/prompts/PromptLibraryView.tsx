import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { CatalogSyncProgress, PromptCategory, PromptInput, PromptItem } from '../../types';
import { api, isTauri } from '../../api/tauriBridge';
import { CustomSelect } from '../common/CustomSelect';
import { useI18n, type MessageKey } from '../../i18n';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import {
  BookMarked,
  Copy,
  ExternalLink,
  Eye,
  Heart,
  Image as ImageIcon,
  Pencil,
  Plus,
  RefreshCw,
  Search,
  Star,
  Trash2,
  Video,
  X,
  Check,
  Upload,
  ArrowUp,
} from 'lucide-react';

const EMPTY_FORM: PromptInput = {
  title: '',
  content: '',
  category: 'image',
  tags: [],
  source_url: '',
  source_note: '',
  notes: '',
  preview_url: '',
  preview_local: '',
  is_starred: false,
};

/** 用户手动上传归档的本地预览图（区别于案例库自动缓存的 /media/prompt-catalog/…） */
function isUserUploadedPreview(prompt: PromptItem): boolean {
  return !!prompt.preview_local?.startsWith('/media/prompts/uploads/');
}

function isImagePath(path: string): boolean {
  return /\.(png|jpe?g|webp|gif)$/i.test(path);
}

function extractVariables(content: string): string[] {
  const matches = content.match(/\{\{([^}]+)\}\}/g) || [];
  return [...new Set(matches.map((m) => m.slice(2, -2).trim()).filter(Boolean))];
}

function applyVariables(content: string, values: Record<string, string>): string {
  let result = content;
  for (const [key, value] of Object.entries(values)) {
    result = result.replaceAll(`{{${key}}}`, value);
  }
  return result;
}

function isMediaCategory(category: string): boolean {
  return category === 'image' || category === 'video';
}

function looksLikeVideoUrl(url: string): boolean {
  return /\.(mp4|webm|mov|m4v)(\?|#|$)/i.test(url) || /\/video\//i.test(url);
}

function looksLikeImageUrl(url: string): boolean {
  return /\.(png|jpe?g|gif|webp|avif|bmp|svg)(\?|#|$)/i.test(url);
}

const LOCAL_MEDIA_BASE = 'http://127.0.0.1:8788';

/** Feed 增量渲染：首屏挂载的卡片数 */
const FEED_INITIAL_BATCH = 48;
/** 滚动步进（px）：每跨过一步才重算顶部窗口，避免逐像素重渲染 */
const SCROLL_STEP_PX = 160;
/** 顶部回收超距：视口上方保留的已挂载内容高度 */
const TOP_OVERSCAN_PX = 900;
/** 哨兵触底后追加的卡片数 */
const FEED_BATCH_SIZE = 48;

/** 封面默认宽高比（w/h），与视频/缺图占位一致；真实比例以 preview_width/height 为准 */
const DEFAULT_COVER_RATIO = 4 / 5;

/** 记录上的封面宽高比元数据（来自数据库），缺失时返回 undefined */
function coverRatio(p: PromptItem): number | undefined {
  if (p.preview_width && p.preview_height && p.preview_height > 0) {
    return p.preview_width / p.preview_height;
  }
  return undefined;
}

function useColumnCount(): number {
  const compute = () => {
    const w = typeof window === 'undefined' ? 1280 : window.innerWidth;
    if (w >= 1536) return 5;
    if (w >= 1280) return 4;
    if (w >= 1024) return 3;
    if (w >= 640) return 2;
    return 1;
  };
  const [count, setCount] = useState(compute);
  useEffect(() => {
    const onResize = () => setCount(compute());
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, []);
  return count;
}

/** jpg ↔ png，兼容目录里部分 case 用 png 而旧数据写死 jpg */
function alternateImageExt(url: string): string | null {
  if (/\.jpe?g(\?|#|$)/i.test(url)) return url.replace(/\.jpe?g(\?|#|$)/i, '.png$1');
  if (/\.png(\?|#|$)/i.test(url)) return url.replace(/\.png(\?|#|$)/i, '.jpg$1');
  return null;
}

function withCacheBust(url: string, bust: number): string {
  if (!bust) return url;
  const sep = url.includes('?') ? '&' : '?';
  return `${url}${sep}_r=${bust}`;
}

/** 候选顺序：本地 → CDN → 扩展名回退 → source */
function previewCandidates(prompt: PromptItem): string[] {
  const out: string[] = [];
  const push = (u?: string | null) => {
    const v = u?.trim();
    if (v && !out.includes(v)) out.push(v);
  };

  const local = prompt.preview_local?.trim();
  if (local) {
    if (local.startsWith('http')) push(local);
    else if (local.startsWith('/media/')) push(`${LOCAL_MEDIA_BASE}${local}`);
  }

  const preview = prompt.preview_url?.trim();
  push(preview);
  if (preview) push(alternateImageExt(preview));

  const source = prompt.source_url?.trim();
  if (source && (looksLikeImageUrl(source) || looksLikeVideoUrl(source))) {
    push(source);
    push(alternateImageExt(source));
  }

  return out;
}

/** 本地缓存优先，否则远程 preview_url / source_url */
function resolvePreviewUrl(prompt: PromptItem): string | null {
  return previewCandidates(prompt)[0] ?? null;
}

function PromptCoverImage({
  prompt,
  alt,
  className,
  ratio,
  onRatio,
  boxed = true,
}: {
  prompt: PromptItem;
  alt: string;
  className: string;
  /** 已知宽高比（w/h）；未知时用默认比例占位，加载后由 onRatio 回传真实值 */
  ratio?: number;
  onRatio?: (id: number, width: number, height: number) => void;
  /** Feed 卡：aspect-ratio 占位盒 + 绝对定位 img，加载前后高度恒定；详情面板不用 */
  boxed?: boolean;
}) {
  const { t } = useI18n();
  const candidates = useMemo(() => previewCandidates(prompt), [prompt]);
  const [idx, setIdx] = useState(0);
  const [bust, setBust] = useState(0);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    setIdx(0);
    setBust(0);
    setFailed(false);
  }, [prompt.id, prompt.preview_url, prompt.preview_local, prompt.source_url]);

  const boxRatio = ratio && ratio > 0 ? ratio : DEFAULT_COVER_RATIO;

  const handlers = {
    onLoad: (e: React.SyntheticEvent<HTMLImageElement>) => {
      const el = e.currentTarget;
      if (el.naturalWidth > 0 && el.naturalHeight > 0) {
        onRatio?.(prompt.id, el.naturalWidth, el.naturalHeight);
      }
    },
    onError: () => {
      if (idx + 1 < candidates.length) setIdx((n) => n + 1);
      else setFailed(true);
    },
  };

  const retryButton = (
    <button
      type="button"
      className={`flex flex-col items-center justify-center gap-2 theme-text-muted px-4 text-center cursor-pointer ${
        boxed ? 'absolute inset-0' : 'w-full min-h-[160px]'
      }`}
      onClick={(e) => {
        e.stopPropagation();
        setFailed(false);
        setIdx(0);
        setBust((n) => n + 1);
      }}
      title={t('prompt.previewRetry')}
    >
      <ImageIcon className="h-9 w-9 opacity-60" />
      <span className="text-[11px] opacity-80 leading-relaxed">{t('prompt.previewRetry')}</span>
    </button>
  );

  if (!candidates.length || failed) {
    if (!boxed) return <div className="theme-bg-sub">{retryButton}</div>;
    return (
      <div
        className="relative w-full theme-bg-sub"
        style={{ aspectRatio: `${DEFAULT_COVER_RATIO}` }}
      >
        {retryButton}
      </div>
    );
  }

  const src = withCacheBust(candidates[Math.min(idx, candidates.length - 1)], bust);

  if (!boxed) {
    return (
      <img src={src} alt={alt} className={className} loading="lazy" decoding="async" {...handlers} />
    );
  }

  return (
    <div className="relative w-full theme-bg-sub" style={{ aspectRatio: `${boxRatio}` }}>
      <img
        src={src}
        alt={alt}
        className={`absolute inset-0 h-full w-full ${className}`}
        loading="lazy"
        decoding="async"
        {...handlers}
      />
    </div>
  );
}

type TranslateFn = (key: MessageKey, vars?: Record<string, string | number>) => string;

/** Feed 媒体卡：抽成 memo 组件，避免详情开/关时整墙卡片重渲 */
const MediaCard = React.memo(function MediaCard({
  prompt,
  copied,
  onOpen,
  onToggleStar,
  onCopy,
  t,
  ratio,
  onRatio,
}: {
  prompt: PromptItem;
  copied: boolean;
  onOpen: (p: PromptItem) => void;
  onToggleStar: (p: PromptItem) => void;
  onCopy: (p: PromptItem) => void;
  t: TranslateFn;
  ratio?: number;
  onRatio?: (id: number, width: number, height: number) => void;
}) {
  const url = resolvePreviewUrl(prompt);
  const isVideo = prompt.category === 'video' || (url ? looksLikeVideoUrl(url) : false);

  return (
    <article
      key={prompt.id}
      role="button"
      tabIndex={0}
      onClick={() => onOpen(prompt)}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onOpen(prompt);
        }
      }}
      className="mb-4 break-inside-avoid theme-bg-card border theme-border rounded-2xl overflow-hidden cursor-pointer hover:border-violet-500/40 transition-colors group"
    >
      <div className="relative">
        <MediaCover prompt={prompt} url={url} isVideo={isVideo} ratio={ratio} onRatio={onRatio} />
        <span className="absolute top-2.5 left-2.5 px-2 py-0.5 rounded-md bg-black/65 text-white text-[10px] font-medium backdrop-blur-sm">
          #{prompt.id}
        </span>
        {prompt.category === 'video' && (
          <span className="absolute top-2.5 right-2.5 px-2 py-0.5 rounded-md bg-black/65 text-white text-[10px] font-medium backdrop-blur-sm flex items-center gap-1">
            <Video className="h-3 w-3" />
            {t('prompt.cat.video')}
          </span>
        )}
        {isUserUploadedPreview(prompt) && (
          <span className="absolute bottom-2.5 left-2.5 px-2 py-0.5 rounded-md bg-violet-600/85 text-white text-[10px] font-medium backdrop-blur-sm flex items-center gap-1">
            <Upload className="h-3 w-3" />
            {t('prompt.localUploadBadge')}
          </span>
        )}
      </div>

      <div className="p-3.5 space-y-2">
        <div className="flex items-center gap-2 text-[11px]">
          <span className="font-semibold tracking-wide text-teal-400 uppercase truncate">
            {prompt.genre || t(CATEGORY_KEYS[prompt.category] || 'prompt.cat.image')}
          </span>
          {prompt.source_note && (
            <span className="theme-text-sub truncate">@{prompt.source_note.replace(/^@/, '')}</span>
          )}
        </div>
        <h2 className="text-[15px] font-bold theme-text-main leading-snug line-clamp-2">
          {prompt.title}
        </h2>
        {prompt.tags.length > 0 && (
          <div className="flex flex-wrap gap-1.5">
            {prompt.tags.slice(0, 4).map((tag) => (
              <span
                key={tag}
                className="px-2 py-0.5 rounded-md theme-bg-tag theme-text-sub text-[10px]"
              >
                {tag}
              </span>
            ))}
          </div>
        )}
        {prompt.origin?.startsWith('catalog:') && (
          <p className="text-[10px] theme-text-sub">{t('prompt.attribution')}</p>
        )}
        <div className="flex items-center justify-end gap-1 pt-0.5" onClick={(e) => e.stopPropagation()}>
          <button
            onClick={() => onToggleStar(prompt)}
            className={`p-1.5 rounded-lg border transition-colors cursor-pointer ${
              prompt.is_starred
                ? 'bg-amber-500/15 border-amber-500/40 text-amber-500'
                : 'theme-border theme-text-muted hover:theme-text-main'
            }`}
          >
            <Star className={`h-3.5 w-3.5 ${prompt.is_starred ? 'fill-amber-400' : ''}`} />
          </button>
          <button
            onClick={() => onCopy(prompt)}
            className={`p-1.5 rounded-lg border transition-colors cursor-pointer ${
              copied
                ? 'bg-emerald-500/15 border-emerald-500/40 text-emerald-600'
                : 'theme-border theme-text-muted hover:theme-text-main'
            }`}
            title={copied ? t('prompt.copiedShort') : t('prompt.copy')}
          >
            {copied ? <Check className="h-3.5 w-3.5" /> : <Copy className="h-3.5 w-3.5" />}
          </button>
        </div>
      </div>
    </article>
  );
});

/** 媒体封面：Feed 卡与详情面板共用 */
function MediaCover({
  prompt,
  url,
  isVideo,
  opts,
  ratio,
  onRatio,
}: {
  prompt: PromptItem;
  url: string | null;
  isVideo: boolean;
  opts?: { fill?: boolean; controls?: boolean; panel?: boolean };
  ratio?: number;
  onRatio?: (id: number, width: number, height: number) => void;
}) {
  const { t } = useI18n();
  const fill = opts?.fill;
  const panel = opts?.panel;

  if (url && isVideo) {
    return (
      <div
        className={`relative w-full overflow-hidden theme-bg-sub ${
          panel ? 'h-full min-h-[240px]' : fill ? '' : 'aspect-[4/5]'
        }`}
      >
        <video
          src={url}
          className={`w-full ${
            panel
              ? 'h-full max-h-[38vh] md:max-h-none object-contain bg-black'
              : fill
                ? 'max-h-[50vh] object-contain bg-black'
                : 'h-full object-cover'
          }`}
          muted
          playsInline
          preload="metadata"
          controls={opts?.controls}
        />
        {!opts?.controls && (
          <div className="absolute inset-0 flex items-center justify-center bg-black/15 pointer-events-none">
            <div className="rounded-full bg-black/50 p-2.5">
              <Video className="h-5 w-5 text-white" />
            </div>
          </div>
        )}
      </div>
    );
  }

  if (url) {
    return (
      <div className={`relative w-full overflow-hidden ${panel ? 'h-full' : 'theme-bg-sub'}`}>
        <PromptCoverImage
          prompt={prompt}
          alt={prompt.title}
          ratio={panel ? undefined : ratio}
          onRatio={panel ? undefined : onRatio}
          boxed={!panel}
          className={`w-full ${
            panel
              ? 'h-full max-h-[38vh] md:max-h-[min(88vh,860px)] object-contain bg-black'
              : fill
                ? 'max-h-[50vh] object-contain bg-black'
                : ''
          }`}
        />
      </div>
    );
  }

  return (
    <div
      className={`relative w-full overflow-hidden ${
        panel ? 'h-full min-h-[240px]' : 'aspect-[4/5]'
      } bg-gradient-to-br ${
        isVideo
          ? 'from-rose-500/25 via-violet-500/20 to-slate-900/40'
          : 'from-sky-500/25 via-violet-500/20 to-fuchsia-500/25'
      }`}
    >
      <div className="absolute inset-0 flex flex-col items-center justify-center gap-2 theme-text-muted px-4 text-center">
        {isVideo ? <Video className="h-9 w-9 opacity-60" /> : <ImageIcon className="h-9 w-9 opacity-60" />}
        <span className="text-[11px] opacity-80 leading-relaxed">{t('prompt.previewMissing')}</span>
      </div>
    </div>
  );
}

/** 详情面板左侧大图：用本地/CDN 候选链渲染 */
function DetailMediaCover({ prompt }: { prompt: PromptItem }) {
  const url = resolvePreviewUrl(prompt);
  const isVideo = prompt.category === 'video' || (url ? looksLikeVideoUrl(url) : false);
  return <MediaCover prompt={prompt} url={url} isVideo={isVideo} opts={{ fill: true, controls: true, panel: true }} />;
}

function looksLikeJson(text: string): boolean {
  const t = text.trim();
  return (t.startsWith('{') && t.includes(':')) || (t.startsWith('[') && t.includes('{'));
}

/** 详情顶栏简介：避免把 JSON prompt 原文塞进摘要 */
function promptIntroText(prompt: PromptItem): string | null {
  const preview = prompt.prompt_preview?.trim();
  if (preview && !looksLikeJson(preview)) {
    return preview.length > 180 ? `${preview.slice(0, 180)}…` : preview;
  }

  const content = prompt.content?.trim() || '';
  if (!content) return null;

  if (looksLikeJson(content)) {
    try {
      const obj = JSON.parse(content) as Record<string, unknown>;
      const parts: string[] = [];
      if (typeof obj.type === 'string' && obj.type.trim()) parts.push(obj.type.trim());
      if (typeof obj.style === 'string' && obj.style.trim()) {
        // 去掉 {argument ...} 模板噪音，取可读片段
        const style = obj.style
          .replace(/\{argument[^}]*default="([^"]*)"[^}]*\}/gi, '$1')
          .replace(/\{argument[^}]*\}/gi, '')
          .trim();
        if (style) parts.push(style);
      }
      if (typeof obj.description === 'string' && obj.description.trim()) {
        parts.push(obj.description.trim());
      }
      if (parts.length === 0) return null;
      const joined = parts.join(' · ');
      return joined.length > 180 ? `${joined.slice(0, 180)}…` : joined;
    } catch {
      return null;
    }
  }

  return content.length > 160 ? `${content.slice(0, 160)}…` : content;
}

interface Props {
  onPromptCountChange?: (count: number) => void;
}

const CATEGORY_KEYS: Record<string, MessageKey> = {
  '': 'prompt.cat.all',
  image: 'prompt.cat.image',
  video: 'prompt.cat.video',
  text: 'prompt.cat.text',
};

const CATEGORY_VALUES = ['', 'image', 'video', 'text'] as const;

export const PromptLibraryView: React.FC<Props> = ({ onPromptCountChange }) => {
  const { t } = useI18n();
  const editorCategories = useMemo(
    () =>
      CATEGORY_VALUES.filter((v) => v !== '').map((value) => ({
        value,
        label: t(CATEGORY_KEYS[value]),
      })),
    [t]
  );
  const [prompts, setPrompts] = useState<PromptItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [search, setSearch] = useState('');
  const [category, setCategory] = useState<PromptCategory | ''>('image');
  const [scene, setScene] = useState('');
  const [starredOnly, setStarredOnly] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [syncProgress, setSyncProgress] = useState<CatalogSyncProgress | null>(null);
  const [detailPrompt, setDetailPrompt] = useState<PromptItem | null>(null);
  /** 详情是否仍在加载完整正文（骨架屏用） */
  const [detailLoading, setDetailLoading] = useState(false);
  const [detailDraft, setDetailDraft] = useState('');
  const [editorOpen, setEditorOpen] = useState(false);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [form, setForm] = useState<PromptInput>(EMPTY_FORM);
  const [tagsInput, setTagsInput] = useState('');
  const [copyToast, setCopyToast] = useState<string | null>(null);
  const [copiedId, setCopiedId] = useState<number | null>(null);
  const [variableModalOpen, setVariableModalOpen] = useState(false);
  const [variablePrompt, setVariablePrompt] = useState<PromptItem | null>(null);
  const [variableValues, setVariableValues] = useState<Record<string, string>>({});
  const [saving, setSaving] = useState(false);
  const [importingImage, setImportingImage] = useState(false);
  /** 编辑器打开时，拖拽图片悬停在窗口上方的引导浮层 */
  const [dragActive, setDragActive] = useState(false);
  /** 增量渲染：当前已挂载到 DOM 的卡片数，触底哨兵追加 */
  const [visibleCount, setVisibleCount] = useState(FEED_INITIAL_BATCH);
  /** 量化后的滚动位置（驱动顶部离屏卡片回收） */
  const [scrollStep, setScrollStep] = useState(0);
  /** 是否显示回到顶部悬浮按钮 */
  const [showBackTop, setShowBackTop] = useState(false);
  const sentinelRef = useRef<HTMLDivElement>(null);
  const feedScrollRef = useRef<HTMLDivElement>(null);

  const formIsMedia = isMediaCategory(form.category);

  const refreshTotalCount = useCallback(async () => {
    try {
      const all = await api.listPrompts();
      onPromptCountChange?.(all.length);
    } catch (e) {
      console.error(e);
    }
  }, [onPromptCountChange]);

  const loadPrompts = useCallback(async () => {
    setLoading(true);
    try {
      const list = await api.listPrompts(
        search.trim() || undefined,
        category || undefined,
        starredOnly,
        true
      );
      setPrompts(list);
      await refreshTotalCount();
      setDetailPrompt((prev) => {
        if (!prev) return null;
        const next = list.find((p) => p.id === prev.id);
        if (!next) return null;
        // 列表是 lite，保留详情里已加载的正文
        return { ...next, content: prev.content || next.content };
      });
    } catch (e) {
      console.error('Failed to load prompts:', e);
    } finally {
      setLoading(false);
    }
  }, [search, category, starredOnly, refreshTotalCount]);

  useEffect(() => {
    loadPrompts();
  }, [loadPrompts]);

  /** 后台把预览图增量落到本地，避免 Feed 一直打 CDN */
  const warmLocalPreviews = useCallback(async () => {
    try {
      const pending = await api.countUncachedPromptPreviews();
      if (pending <= 0) return;
      await api.warmPromptPreviewCache(40, 25);
      await loadPrompts();
      const still = await api.countUncachedPromptPreviews();
      if (still > 0) {
        // 继续下一批，不阻塞 UI
        void api.warmPromptPreviewCache(40, 25).then(() => loadPrompts());
      }
    } catch (e) {
      console.error('warm preview cache failed', e);
    }
  }, [loadPrompts]);

  useEffect(() => {
    const t = window.setTimeout(() => {
      void warmLocalPreviews();
    }, 800);
    return () => window.clearTimeout(t);
  }, [warmLocalPreviews]);

  const showToast = useCallback((msg: string) => {
    setCopyToast(msg);
    setTimeout(() => setCopyToast(null), 2800);
  }, []);

  const handleSyncCatalog = useCallback(async () => {
    setSyncing(true);
    setSyncProgress({
      phase: 'download',
      message: t('prompt.syncing'),
      current: 0,
      total: 0,
      percent: 1,
      inserted: 0,
      updated: 0,
      unchanged: 0,
      imagesCached: 0,
    });
    try {
      const result = await api.syncGptImageCatalog();
      showToast(result.message);
      await loadPrompts();
      void warmLocalPreviews();
    } catch (e) {
      console.error(e);
      showToast(t('prompt.syncFailed'));
    } finally {
      setSyncing(false);
      setSyncProgress(null);
    }
  }, [loadPrompts, showToast, t, warmLocalPreviews]);

  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    listen<CatalogSyncProgress>('prompt-catalog-sync-progress', (event) => {
      setSyncProgress(event.payload);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  // 首次进入：若库里几乎没有目录项，自动同步一次
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const list = await api.listPrompts(undefined, 'image', false);
        const catalogCount = list.filter((p) => p.origin?.startsWith('catalog:')).length;
        if (!cancelled && catalogCount < 10) {
          await handleSyncCatalog();
        }
      } catch {
        /* ignore auto-sync errors */
      }
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const filteredPrompts = useMemo(() => {
    if (!scene) return prompts;
    return prompts.filter((p) => (p.scenes || []).includes(scene));
  }, [prompts, scene]);

  /** 筛选/搜索条件变化时回到首屏批次；列表内容更新（收藏/复制/同步）不重置挂载量 */
  useEffect(() => {
    setVisibleCount(FEED_INITIAL_BATCH);
    feedScrollRef.current?.scrollTo({ top: 0 });
  }, [search, category, starredOnly, scene]);

  /** 滚动联动：量化步进驱动顶部回收；超过一屏显示回到顶部按钮 */
  const handleFeedScroll = useCallback(() => {
    const el = feedScrollRef.current;
    if (!el) return;
    const step = Math.floor(el.scrollTop / SCROLL_STEP_PX);
    setScrollStep((prev) => (prev === step ? prev : step));
    const show = el.scrollTop > 480;
    setShowBackTop((prev) => (prev === show ? prev : show));
  }, []);

  /** 只把可见切片挂进 DOM；哨兵进入视口即追加下一批 */
  const visiblePrompts = useMemo(
    () => filteredPrompts.slice(0, visibleCount),
    [filteredPrompts, visibleCount]
  );
  const hasMore = visibleCount < filteredPrompts.length;

  useEffect(() => {
    const el = sentinelRef.current;
    if (!el || !hasMore) return;
    const io = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) {
          setVisibleCount((c) => c + FEED_BATCH_SIZE);
        }
      },
      { rootMargin: '600px 0px' }
    );
    io.observe(el);
    return () => io.disconnect();
  }, [hasMore, visibleCount]);

  const sceneOptions = useMemo(() => {
    const set = new Set<string>();
    for (const p of prompts) {
      for (const s of p.scenes || []) set.add(s);
    }
    return Array.from(set).sort();
  }, [prompts]);

  const openDetail = useCallback(async (prompt: PromptItem) => {
    // 立刻打开：lite 列表缺少完整正文时用骨架占位，getPrompt 完成后回填
    const needFullBody = !prompt.content && !prompt.prompt_preview;
    setDetailPrompt(prompt);
    setDetailDraft(prompt.content || prompt.prompt_preview || '');
    if (!needFullBody) return;
    setDetailLoading(true);
    try {
      const full = await api.getPrompt(prompt.id);
      setDetailPrompt(full);
      setDetailDraft(full.content);
      setPrompts((prev) => prev.map((p) => (p.id === full.id ? { ...p, ...full, content: '' } : p)));
    } catch (e) {
      console.error(e);
    } finally {
      setDetailLoading(false);
    }
  }, []);

  const closeDetail = useCallback(() => {
    setDetailPrompt(null);
    setDetailDraft('');
    setDetailLoading(false);
  }, []);

  const caseNumber = (prompt: PromptItem): string | null => {
    const id = prompt.external_id?.match(/gpt-image-2:(\d+)/)?.[1];
    return id || null;
  };

  const openCreate = () => {
    setEditingId(null);
    setForm(EMPTY_FORM);
    setTagsInput('');
    setEditorOpen(true);
  };

  const openEdit = (prompt: PromptItem) => {
    setEditingId(prompt.id);
    setForm({
      title: prompt.title,
      content: prompt.content,
      category: prompt.category,
      tags: prompt.tags,
      source_url: prompt.source_url || '',
      source_note: prompt.source_note || '',
      notes: prompt.notes || '',
      preview_url: prompt.preview_url || '',
      preview_local: prompt.preview_local || '',
      is_starred: prompt.is_starred,
    });
    setTagsInput(prompt.tags.join(', '));
    setEditorOpen(true);
  };

  const parseTags = (raw: string): string[] =>
    raw
      .split(/[,，]/)
      .map((t) => t.trim())
      .filter(Boolean);

  const handleSave = async () => {
    if (!form.title.trim()) {
      showToast(t('prompt.needTitle'));
      return;
    }
    setSaving(true);
    try {
      const payload: PromptInput = {
        ...form,
        title: form.title.trim(),
        tags: parseTags(tagsInput),
        source_url: form.source_url?.trim() || undefined,
        source_note: form.source_note?.trim() || undefined,
        notes: form.notes?.trim() || undefined,
        preview_url: form.preview_url?.trim() || undefined,
        preview_local: form.preview_local,
      };
      if (editingId) {
        const updated = await api.updatePrompt(editingId, payload);
        setPrompts((prev) => prev.map((p) => (p.id === updated.id ? updated : p)));
        setDetailPrompt((prev) => (prev?.id === updated.id ? updated : prev));
      } else {
        const created = await api.createPrompt(payload);
        setPrompts((prev) => [created, ...prev]);
        await refreshTotalCount();
      }
      setEditorOpen(false);
      showToast(editingId ? t('prompt.updated') : t('prompt.added'));
    } catch (e) {
      console.error(e);
      showToast(t('prompt.saveFailed'));
    } finally {
      setSaving(false);
    }
  };

  const handlePickImage = async () => {
    if (importingImage) return;
    setImportingImage(true);
    try {
      const path = await api.pickPromptPreviewImage();
      if (path) {
        setForm((f) => ({ ...f, preview_local: path }));
      }
    } catch (e) {
      console.error(e);
      showToast(t('prompt.uploadImageFailed'));
    } finally {
      setImportingImage(false);
    }
  };

  // 编辑器打开时支持整窗拖拽导入预览图（Tauri v2 窗口级拖放事件，HTML5 drop 拿不到文件）
  useEffect(() => {
    if (!isTauri() || !editorOpen || !formIsMedia) return;
    let unlisten: (() => void) | null = null;
    let cancelled = false;

    getCurrentWebview()
      .onDragDropEvent((event) => {
        const payload = event.payload;
        if (payload.type === 'enter') {
          setDragActive(payload.paths.some(isImagePath));
        } else if (payload.type === 'leave') {
          setDragActive(false);
        } else if (payload.type === 'drop') {
          setDragActive(false);
          const image = payload.paths.find(isImagePath);
          if (!image) {
            showToast(t('prompt.dropImageOnly'));
            return;
          }
          setImportingImage(true);
          api
            .importPromptPreviewImage(image)
            .then((path) => {
              setForm((f) => ({ ...f, preview_local: path }));
            })
            .catch((e) => {
              console.error(e);
              showToast(t('prompt.uploadImageFailed'));
            })
            .finally(() => setImportingImage(false));
        }
      })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch((e) => console.error('drag-drop listener failed:', e));

    return () => {
      cancelled = true;
      setDragActive(false);
      if (unlisten) unlisten();
    };
  }, [editorOpen, formIsMedia, showToast, t]);

  const handleDelete = async (id: number) => {
    if (!window.confirm(t('prompt.confirmDelete'))) return;
    try {
      await api.deletePrompt(id);
      setPrompts((prev) => prev.filter((p) => p.id !== id));
      await refreshTotalCount();
      if (detailPrompt?.id === id) closeDetail();
      showToast(t('prompt.deleted'));
    } catch (e) {
      console.error(e);
      showToast(t('prompt.deleteFailed'));
    }
  };

  const handleToggleStar = useCallback(async (prompt: PromptItem) => {
    try {
      const starred = await api.togglePromptStar(prompt.id);
      const patch = (p: PromptItem) =>
        p.id === prompt.id ? { ...p, is_starred: starred } : p;
      setPrompts((prev) => prev.map(patch));
      setDetailPrompt((prev) => (prev ? patch(prev) : prev));
    } catch (e) {
      console.error(e);
    }
  }, []);

  const copyPromptContent = useCallback(async (prompt: PromptItem, content: string) => {
    try {
      await navigator.clipboard.writeText(content);
      await api.recordPromptUse(prompt.id);
      const bump = (p: PromptItem) =>
        p.id === prompt.id
          ? { ...p, use_count: p.use_count + 1, last_used_at: new Date().toISOString() }
          : p;
      setPrompts((prev) => prev.map(bump));
      setDetailPrompt((prev) => (prev ? bump(prev) : prev));
      setCopiedId(prompt.id);
      window.setTimeout(() => {
        setCopiedId((cur) => (cur === prompt.id ? null : cur));
      }, 1800);
      showToast(t('prompt.copied'));
    } catch (e) {
      console.error(e);
      showToast(t('prompt.copyFailed'));
    }
  }, [showToast, t]);

  const handleCopy = useCallback(async (prompt: PromptItem) => {
    const vars = extractVariables(prompt.content);
    if (vars.length > 0) {
      const initial: Record<string, string> = {};
      vars.forEach((v) => {
        initial[v] = '';
      });
      setVariableValues(initial);
      setVariablePrompt(prompt);
      setVariableModalOpen(true);
      return;
    }
    await copyPromptContent(prompt, prompt.content);
  }, [copyPromptContent]);

  const confirmVariableCopy = async () => {
    if (!variablePrompt) return;
    const content = applyVariables(variablePrompt.content, variableValues);
    setVariableModalOpen(false);
    setVariablePrompt(null);
    await copyPromptContent(variablePrompt, content);
  };

  const renderCardActions = (prompt: PromptItem, opts?: { showView?: boolean }) => (
    <div className="flex items-center gap-1.5 pt-1">
      <button
        onClick={(e) => {
          e.stopPropagation();
          void handleToggleStar(prompt);
        }}
        className={`flex items-center gap-1 px-2.5 py-1.5 rounded-lg border text-[11px] transition-colors cursor-pointer ${
          prompt.is_starred
            ? 'bg-amber-500/15 border-amber-500/40 text-amber-500'
            : 'theme-bg-sub theme-border theme-text-muted hover:theme-text-main'
        }`}
        title={t('prompt.starred')}
      >
        <Star className={`h-3.5 w-3.5 ${prompt.is_starred ? 'fill-amber-400' : ''}`} />
      </button>
      <button
        onClick={(e) => {
          e.stopPropagation();
          void handleCopy(prompt);
        }}
        className={`flex items-center gap-1 px-2.5 py-1.5 rounded-lg border text-[11px] transition-colors cursor-pointer ${
          copiedId === prompt.id
            ? 'bg-emerald-500/15 border-emerald-500/40 text-emerald-600'
            : 'theme-border theme-bg-sub theme-text-muted hover:theme-text-main'
        }`}
      >
        {copiedId === prompt.id ? (
          <Check className="h-3.5 w-3.5" />
        ) : (
          <Copy className="h-3.5 w-3.5" />
        )}
        {copiedId === prompt.id ? t('prompt.copiedShort') : t('prompt.copy')}
      </button>
      {opts?.showView !== false && (
        <button
          onClick={(e) => {
            e.stopPropagation();
            openDetail(prompt);
          }}
          className="flex items-center gap-1 px-2.5 py-1.5 rounded-lg border theme-border theme-bg-sub theme-text-muted hover:theme-text-main text-[11px] transition-colors cursor-pointer"
        >
          <Eye className="h-3.5 w-3.5" />
          {t('prompt.viewDetail')}
        </button>
      )}
      <button
        onClick={(e) => {
          e.stopPropagation();
          openEdit(prompt);
        }}
        className="ml-auto p-1.5 rounded-lg border theme-border theme-bg-sub theme-text-muted hover:theme-text-main transition-colors cursor-pointer"
      >
        <Pencil className="h-3.5 w-3.5" />
      </button>
      <button
        onClick={(e) => {
          e.stopPropagation();
          void handleDelete(prompt.id);
        }}
        className="p-1.5 rounded-lg border theme-border theme-bg-sub text-red-400 hover:bg-red-500/10 transition-colors cursor-pointer"
      >
        <Trash2 className="h-3.5 w-3.5" />
      </button>
    </div>
  );

  /** 文本卡：正文直接展示 */
  const renderTextCard = (prompt: PromptItem) => (
    <article
      key={prompt.id}
      className="mb-4 break-inside-avoid theme-bg-card border theme-border rounded-2xl overflow-hidden hover:border-violet-500/35 transition-colors"
    >
      <div className="p-4 space-y-2.5">
        <div className="flex items-center gap-2 text-[11px]">
          <span className="font-semibold tracking-wide text-emerald-500 uppercase">
            {t(CATEGORY_KEYS[prompt.category] || 'prompt.cat.coding')}
          </span>
          {prompt.source_note && (
            <span className="theme-text-sub truncate">@{prompt.source_note}</span>
          )}
          {prompt.is_starred && (
            <Star className="h-3 w-3 fill-amber-400 text-amber-400 ml-auto flex-shrink-0" />
          )}
        </div>

        <h2 className="text-[15px] font-bold theme-text-main leading-snug">{prompt.title}</h2>

        <pre className="whitespace-pre-wrap break-words text-[13px] leading-relaxed theme-text-main font-sans">
          {prompt.content}
        </pre>

        {prompt.tags.length > 0 && (
          <div className="flex flex-wrap gap-1.5">
            {prompt.tags.map((tag) => (
              <span
                key={tag}
                className="px-2 py-0.5 rounded-md theme-bg-tag theme-text-sub text-[10px]"
              >
                {tag}
              </span>
            ))}
          </div>
        )}

        {renderCardActions(prompt)}
      </div>
    </article>
  );

  /** 会话内测量的封面比例（仅 DB 尚无尺寸的记录）；数据库是权威来源 */
  const measuredRef = useRef<Map<number, number>>(new Map());
  /** 新测得、待写回 DB 的原始宽高 */
  const pendingSizeRef = useRef<Map<number, [number, number]>>(new Map());
  /** 卡片包装元素（按 id），用于比例测量后复测实际高度 */
  const cardElsRef = useRef<Map<number, HTMLElement>>(new Map());
  /** 卡片实际渲染高度（含 mb-4 间距），离屏回收时折算成等高占位 */
  const cardHeightsRef = useRef<Map<number, number>>(new Map());
  const [sizeVersion, setSizeVersion] = useState(0);
  const promptsRef = useRef<PromptItem[]>([]);
  useEffect(() => {
    promptsRef.current = prompts;
  }, [prompts]);

  const handleRatio = useCallback((id: number, width: number, height: number) => {
    const ratio = width / height;
    const prev = measuredRef.current.get(id);
    if (prev && Math.abs(prev - ratio) < 0.01) return;
    // DB 已有相同尺寸则不重复写
    const p = promptsRef.current.find((x) => x.id === id);
    const dbRatio = p ? coverRatio(p) : undefined;
    if (dbRatio && Math.abs(dbRatio - ratio) < 0.01) return;
    measuredRef.current.set(id, ratio);
    pendingSizeRef.current.set(id, [width, height]);
    setSizeVersion((v) => v + 1);
  }, []);

  /** 记录卡片实际高度（含 mb-4 间距）；卸载后保留高度供占位计算 */
  const measureCard = useCallback((id: number, el: HTMLElement | null) => {
    if (el) {
      cardElsRef.current.set(id, el);
      cardHeightsRef.current.set(id, el.getBoundingClientRect().height);
    } else {
      cardElsRef.current.delete(id);
    }
  }, []);

  /** 防抖批量写回数据库；失败不影响渲染，下次加载会重新测量 */
  useEffect(() => {
    if (!sizeVersion || !isTauri()) return;
    const timer = window.setTimeout(() => {
      const batch = Array.from(pendingSizeRef.current.entries());
      pendingSizeRef.current.clear();
      for (const [id, [w, h]] of batch) {
        void api.updatePromptPreviewSize(id, w, h).catch(() => {
          /* ignore */
        });
      }
    }, 2000);
    return () => window.clearTimeout(timer);
  }, [sizeVersion]);

  const columnCount = useColumnCount();
  /** 卡片 → 列 的稳定分配：已挂载的卡不随比例测量/批次追加换列 */
  const assignRef = useRef<{ cols: number; map: Map<number, number> } | null>(null);

  /**
   * JS 分列（最短列优先）替代 CSS columns：CSS 多列是全局平衡布局，
   * 任何高度变化都会整墙重排；这里已分配的卡固定列，追加批次只往下接。
   */
  const columns = useMemo(() => {
    const est = (p: PromptItem): number => {
      if (isMediaCategory(p.category)) {
        const ratio = coverRatio(p) ?? measuredRef.current.get(p.id) ?? DEFAULT_COVER_RATIO;
        return 100 / ratio + 46;
      }
      return 56 + Math.min(p.prompt_preview?.length ?? 0, 300) / 6;
    };
    let assign = assignRef.current;
    if (!assign || assign.cols !== columnCount) {
      assign = { cols: columnCount, map: new Map() };
      assignRef.current = assign;
    }
    const colsArr: PromptItem[][] = Array.from({ length: columnCount }, () => []);
    const heights = new Array<number>(columnCount).fill(0);
    // 已分配的卡留在原列（保持滚动中不跳列）
    for (const p of visiblePrompts) {
      const ci = assign.map.get(p.id);
      if (ci != null && ci < columnCount) {
        colsArr[ci].push(p);
        heights[ci] += est(p);
      }
    }
    // 新卡贪心放入当前最矮列，并记住分配
    for (const p of visiblePrompts) {
      if (assign.map.has(p.id)) continue;
      let target = 0;
      for (let i = 1; i < columnCount; i++) {
        if (heights[i] < heights[target]) target = i;
      }
      assign.map.set(p.id, target);
      colsArr[target].push(p);
      heights[target] += est(p);
    }
    return colsArr;
  }, [visiblePrompts, columnCount, sizeVersion]);

  /** 顶部离屏回收：滚出视口上方的卡片折算成等高占位，DOM 数量不随滚动无限增长 */
  const cutoffTop = Math.max(0, scrollStep * SCROLL_STEP_PX - TOP_OVERSCAN_PX);
  const splitColumn = (col: PromptItem[]): { spacerHeight: number; start: number } => {
    let height = 0;
    let start = 0;
    for (let i = 0; i < col.length; i++) {
      const fallback = isMediaCategory(col[i].category) ? 340 : 120;
      const h = cardHeightsRef.current.get(col[i].id) ?? fallback;
      if (height + h <= cutoffTop) {
        height += h;
        start = i + 1;
      } else {
        break;
      }
    }
    return { spacerHeight: height, start };
  };

  return (
    <div className="flex h-full w-full overflow-hidden theme-bg-main">
      <div className="flex-1 flex flex-col overflow-hidden">
        <header className="flex-shrink-0 border-b theme-border theme-bg-header backdrop-blur-sm">
          <div className="w-full px-5 py-4 space-y-3">
            <div className="flex items-center justify-between gap-3">
              <div className="flex items-center gap-2 min-w-0">
                <BookMarked className="h-4 w-4 text-violet-500 flex-shrink-0" />
                <h1 className="text-sm font-bold theme-text-main truncate">{t('prompt.title')}</h1>
                {!loading && (
                  <span className="text-[11px] theme-text-sub flex-shrink-0">
                    {t('prompt.matchCount', { n: filteredPrompts.length })}
                  </span>
                )}
              </div>
              <div className="flex items-center gap-2 flex-shrink-0">
                <div className="relative hidden sm:block">
                  <Search className="absolute left-2.5 top-2 h-3.5 w-3.5 theme-text-sub" />
                  <input
                    type="text"
                    placeholder={t('prompt.search')}
                    value={search}
                    onChange={(e) => setSearch(e.target.value)}
                    className="w-56 pl-8 pr-3 py-1.5 text-xs theme-bg-input border theme-border rounded-lg theme-text-main placeholder-slate-400 focus:outline-none focus:border-violet-500"
                  />
                </div>
                <button
                  onClick={() => void handleSyncCatalog()}
                  disabled={syncing}
                  className="flex items-center gap-1 px-2.5 py-1.5 text-xs rounded-lg border theme-bg-card theme-border theme-text-muted hover:theme-text-main transition-colors cursor-pointer disabled:opacity-60"
                  title={t('prompt.attribution')}
                >
                  <RefreshCw className={`h-3.5 w-3.5 ${syncing ? 'animate-spin' : ''}`} />
                  {syncing ? t('prompt.syncing') : t('prompt.sync')}
                </button>
                <button
                  onClick={() => setStarredOnly((v) => !v)}
                  className={`flex items-center gap-1 px-2.5 py-1.5 text-xs rounded-lg border transition-colors cursor-pointer ${
                    starredOnly
                      ? 'bg-amber-500/15 border-amber-500/40 text-amber-500'
                      : 'theme-bg-card theme-border theme-text-muted hover:theme-text-main'
                  }`}
                >
                  <Star className={`h-3.5 w-3.5 ${starredOnly ? 'fill-amber-400' : ''}`} />
                  {t('prompt.starred')}
                </button>
                <button
                  onClick={openCreate}
                  className="flex items-center gap-1 px-2.5 py-1.5 text-xs font-medium rounded-lg bg-violet-600 hover:bg-violet-500 text-white transition-colors cursor-pointer"
                >
                  <Plus className="h-3.5 w-3.5" />
                  {t('prompt.new')}
                </button>
              </div>
            </div>

            <div className="relative sm:hidden">
              <Search className="absolute left-2.5 top-2 h-3.5 w-3.5 theme-text-sub" />
              <input
                type="text"
                placeholder={t('prompt.search')}
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                className="w-full pl-8 pr-3 py-1.5 text-xs theme-bg-input border theme-border rounded-lg theme-text-main placeholder-slate-400 focus:outline-none focus:border-violet-500"
              />
            </div>

            <div className="flex items-center gap-2 overflow-x-auto pb-0.5 -mx-1 px-1 scrollbar-none">
              {CATEGORY_VALUES.map((value) => {
                const active = category === value;
                return (
                  <button
                    key={value || 'all'}
                    onClick={() => {
                      setCategory(value);
                      setScene('');
                    }}
                    className={`flex-shrink-0 px-3 py-1.5 rounded-full text-xs font-medium border transition-colors cursor-pointer ${
                      active
                        ? 'bg-violet-600 border-violet-500 text-white'
                        : 'theme-bg-card theme-border theme-text-muted hover:theme-text-main'
                    }`}
                  >
                    {t(CATEGORY_KEYS[value])}
                  </button>
                );
              })}
            </div>

            {sceneOptions.length > 0 && (category === '' || category === 'image') && (
              <div className="flex items-center gap-2 overflow-x-auto pb-0.5 -mx-1 px-1 scrollbar-none">
                <button
                  onClick={() => setScene('')}
                  className={`flex-shrink-0 px-2.5 py-1 rounded-full text-[11px] border transition-colors cursor-pointer ${
                    !scene
                      ? 'bg-teal-600/90 border-teal-500 text-white'
                      : 'theme-bg-card theme-border theme-text-muted hover:theme-text-main'
                  }`}
                >
                  {t('prompt.scene.all')}
                </button>
                {sceneOptions.map((s) => (
                  <button
                    key={s}
                    onClick={() => setScene(s)}
                    className={`flex-shrink-0 px-2.5 py-1 rounded-full text-[11px] border transition-colors cursor-pointer ${
                      scene === s
                        ? 'bg-teal-600/90 border-teal-500 text-white'
                        : 'theme-bg-card theme-border theme-text-muted hover:theme-text-main'
                    }`}
                  >
                    {s}
                  </button>
                ))}
              </div>
            )}
          </div>
        </header>

        {/* 多列瀑布流 */}
        <div ref={feedScrollRef} className="flex-1 overflow-y-auto" onScroll={handleFeedScroll}>
          <div className="w-full px-5 py-5">
            {syncing && filteredPrompts.length === 0 ? (
              <div className="py-16 px-6 flex flex-col items-center text-center max-w-md mx-auto space-y-4">
                <RefreshCw className="h-10 w-10 text-violet-500 animate-spin" />
                <div className="space-y-1">
                  <p className="text-sm font-semibold theme-text-main">
                    {t('prompt.syncProgressTitle')}
                  </p>
                  <p className="text-xs theme-text-sub leading-relaxed">
                    {syncProgress?.message || t('prompt.syncProgressHint')}
                  </p>
                </div>
                <div className="w-full space-y-2">
                  <div className="h-2 w-full rounded-full theme-bg-sub overflow-hidden border theme-border">
                    <div
                      className="h-full rounded-full bg-violet-500 transition-all duration-300 ease-out"
                      style={{ width: `${Math.max(2, syncProgress?.percent ?? 2)}%` }}
                    />
                  </div>
                  <div className="flex items-center justify-between text-[11px] theme-text-sub">
                    <span>
                      {syncProgress && syncProgress.total > 0
                        ? `${syncProgress.current} / ${syncProgress.total}`
                        : t('prompt.syncing')}
                    </span>
                    <span>{syncProgress?.percent ?? 0}%</span>
                  </div>
                  {syncProgress && (syncProgress.inserted > 0 || syncProgress.updated > 0 || syncProgress.unchanged > 0) && (
                    <p className="text-[11px] theme-text-sub">
                      {t('prompt.syncStats', {
                        inserted: syncProgress.inserted,
                        updated: syncProgress.updated,
                        unchanged: syncProgress.unchanged,
                      })}
                    </p>
                  )}
                </div>
              </div>
            ) : loading && filteredPrompts.length === 0 ? (
              <div className="py-16 text-center text-xs theme-text-sub">
                {t('prompt.loading')}
              </div>
            ) : filteredPrompts.length === 0 ? (
              <div className="py-16 px-4 text-center space-y-2">
                <BookMarked className="h-10 w-10 text-violet-500/40 mx-auto mb-2" />
                <p className="text-sm theme-text-muted">{t('prompt.feedEmpty')}</p>
                <p className="text-xs theme-text-sub leading-relaxed max-w-sm mx-auto">
                  {t('prompt.feedEmptyHint')}
                </p>
                <div className="flex items-center justify-center gap-3 mt-2">
                  <button
                    onClick={() => void handleSyncCatalog()}
                    className="text-xs text-violet-500 hover:underline cursor-pointer"
                  >
                    {t('prompt.sync')}
                  </button>
                  <button
                    onClick={openCreate}
                    className="text-xs text-violet-500 hover:underline cursor-pointer"
                  >
                    {t('prompt.addFirst')}
                  </button>
                </div>
              </div>
            ) : (
              <div className="flex items-start gap-4">
                {columns.map((col, ci) => {
                  const { spacerHeight, start } = splitColumn(col);
                  return (
                    <div key={ci} className="flex-1 min-w-0">
                      {spacerHeight > 0 && <div style={{ height: spacerHeight }} aria-hidden="true" />}
                      {col.slice(start).map((prompt) => (
                        <div
                          key={prompt.id}
                          className="flow-root"
                          ref={(el) => measureCard(prompt.id, el)}
                        >
                          {isMediaCategory(prompt.category) ? (
                        <MediaCard
                          prompt={prompt}
                          copied={copiedId === prompt.id}
                          onOpen={openDetail}
                          onToggleStar={handleToggleStar}
                          onCopy={handleCopy}
                          t={t}
                          ratio={coverRatio(prompt) ?? measuredRef.current.get(prompt.id)}
                          onRatio={handleRatio}
                        />
                          ) : (
                            renderTextCard(prompt)
                          )}
                        </div>
                      ))}
                    </div>
                  );
                })}
              </div>
            )}
            {hasMore && (
              <div
                ref={sentinelRef}
                className="py-6 text-center text-[11px] theme-text-muted"
              >
                {t('prompt.loadMore')}
              </div>
            )}
          </div>
        </div>
      </div>

      {/* 回到顶部悬浮按钮 */}
      {showBackTop && (
        <button
          type="button"
          onClick={() => feedScrollRef.current?.scrollTo({ top: 0, behavior: 'smooth' })}
          title={t('prompt.backToTop')}
          className="fixed bottom-8 right-8 z-40 h-10 w-10 rounded-full theme-bg-card border theme-border shadow-lg flex items-center justify-center theme-text-muted hover:theme-text-main hover:border-violet-500/50 hover:-translate-y-0.5 transition-all cursor-pointer"
        >
          <ArrowUp className="h-5 w-5" />
        </button>
      )}

      {/* 详情：官网风格左右分栏 */}
      {detailPrompt && (
        <div
          className="fixed inset-0 z-50 flex items-end sm:items-center justify-center p-0 sm:p-4 bg-black/60 backdrop-blur-sm animate-in fade-in duration-150"
          onClick={closeDetail}
        >
          <div
            className="w-full max-w-6xl max-h-[94vh] overflow-hidden flex flex-col md:flex-row theme-bg-card border theme-border rounded-t-2xl md:rounded-2xl shadow-2xl animate-in zoom-in-95 fade-in duration-150"
            onClick={(e) => e.stopPropagation()}
          >
            {/* 左侧预览 */}
            {isMediaCategory(detailPrompt.category) && (
              <div className="md:w-[44%] flex-shrink-0 bg-black/90 flex items-center justify-center max-h-[38vh] md:max-h-none md:min-h-[560px] overflow-hidden">
                <DetailMediaCover prompt={detailPrompt} />
              </div>
            )}

            {/* 右侧信息 */}
            <div className="flex-1 flex flex-col min-w-0 min-h-0 relative">
              <button
                onClick={closeDetail}
                className="absolute top-3 right-3 z-10 p-1.5 rounded-lg theme-text-muted hover:theme-text-main theme-bg-card/80 border theme-border cursor-pointer"
                aria-label={t('prompt.closeDetail')}
              >
                <X className="h-4 w-4" />
              </button>

              <div className="flex-1 overflow-y-auto p-5 md:p-6 space-y-4 pr-12">
                <div className="flex items-center gap-2 flex-wrap">
                  {caseNumber(detailPrompt) && (
                    <span className="px-2.5 py-1 rounded-full text-[10px] font-semibold tracking-wide theme-bg-tag theme-text-muted border theme-border">
                      {t('prompt.caseLabel', { n: caseNumber(detailPrompt)! })}
                    </span>
                  )}
                  <span className="px-2.5 py-1 rounded-full text-[10px] font-semibold tracking-wide uppercase bg-teal-500/15 text-teal-500 border border-teal-500/30">
                    {detailPrompt.genre ||
                      t(CATEGORY_KEYS[detailPrompt.category] || 'prompt.cat.image')}
                  </span>
                  {isUserUploadedPreview(detailPrompt) && (
                    <span className="px-2.5 py-1 rounded-full text-[10px] font-semibold tracking-wide bg-violet-500/15 text-violet-500 border border-violet-500/30 flex items-center gap-1">
                      <Upload className="h-3 w-3" />
                      {t('prompt.localUploadBadge')}
                    </span>
                  )}
                </div>

                <div className="space-y-2">
                  <h2 className="text-xl font-bold theme-text-main leading-snug pr-2">
                    {detailPrompt.title}
                  </h2>
                  {(() => {
                    const intro = promptIntroText(detailPrompt);
                    return intro ? (
                      <p className="text-sm theme-text-sub leading-relaxed line-clamp-3">{intro}</p>
                    ) : null;
                  })()}
                </div>

                {(detailPrompt.tags.length > 0 || (detailPrompt.scenes || []).length > 0) && (
                  <div className="flex flex-wrap gap-1.5">
                    {detailPrompt.tags.map((tag) => (
                      <span
                        key={tag}
                        className="px-2 py-0.5 rounded-md theme-bg-tag theme-text-muted text-[10px]"
                      >
                        {tag}
                      </span>
                    ))}
                  </div>
                )}

                {/* 主操作：收藏 + 复制 */}
                <div className="flex items-center gap-2 flex-wrap">
                  <button
                    onClick={() => void handleToggleStar(detailPrompt)}
                    className={`flex items-center gap-1.5 px-3.5 py-2 rounded-xl border text-xs font-medium transition-colors cursor-pointer ${
                      detailPrompt.is_starred
                        ? 'bg-rose-500/15 border-rose-500/40 text-rose-500'
                        : 'theme-border theme-text-muted hover:theme-text-main'
                    }`}
                  >
                    <Heart
                      className={`h-3.5 w-3.5 ${detailPrompt.is_starred ? 'fill-rose-500' : ''}`}
                    />
                    {detailPrompt.is_starred ? t('prompt.favorited') : t('prompt.favorite')}
                  </button>
                  <button
                    onClick={() =>
                      void handleCopy({ ...detailPrompt, content: detailDraft || detailPrompt.content })
                    }
                    disabled={detailLoading}
                    className={`flex items-center gap-1.5 px-3.5 py-2 rounded-xl text-xs font-medium transition-colors ${
                      copiedId === detailPrompt.id
                        ? 'bg-emerald-600 hover:bg-emerald-500 text-white'
                        : 'bg-violet-600 hover:bg-violet-500 text-white disabled:opacity-50 disabled:cursor-not-allowed'
                    }`}
                  >
                    {copiedId === detailPrompt.id ? (
                      <Check className="h-3.5 w-3.5" />
                    ) : (
                      <Copy className="h-3.5 w-3.5" />
                    )}
                    {copiedId === detailPrompt.id ? t('prompt.copiedShort') : t('prompt.copyPrompt')}
                  </button>
                  <button
                    onClick={() => openEdit(detailPrompt)}
                    className="p-2 rounded-xl border theme-border theme-text-muted hover:theme-text-main cursor-pointer"
                    title={t('prompt.edit')}
                  >
                    <Pencil className="h-3.5 w-3.5" />
                  </button>
                </div>

                {/* 外链 */}
                <div className="flex items-center gap-2 flex-wrap">
                  {detailPrompt.source_url && (
                    <button
                      onClick={() => api.openUrl(detailPrompt.source_url!)}
                      className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg border theme-border text-[11px] theme-text-muted hover:theme-text-main cursor-pointer"
                    >
                      <ExternalLink className="h-3 w-3" />
                      {t('prompt.openSource')}
                    </button>
                  )}
                  {detailPrompt.github_url && (
                    <button
                      onClick={() => api.openUrl(detailPrompt.github_url!)}
                      className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg border theme-border text-[11px] theme-text-muted hover:theme-text-main cursor-pointer"
                    >
                      <ExternalLink className="h-3 w-3" />
                      {t('prompt.openGithub')}
                    </button>
                  )}
                  {detailPrompt.source_note && (
                    <span className="text-[11px] theme-text-sub">
                      {t('prompt.sourcePrefix', { note: detailPrompt.source_note })}
                    </span>
                  )}
                </div>

                {/* 可编辑提示词区 */}
                <section className="space-y-2">
                  <div className="flex items-center justify-between gap-2">
                    <h3 className="text-xs font-semibold theme-text-muted uppercase tracking-wide">
                      {t('prompt.editablePrompt')}
                    </h3>
                    {!detailLoading && detailDraft !== detailPrompt.content && (
                      <button
                        onClick={() => setDetailDraft(detailPrompt.content)}
                        className="text-[11px] text-violet-500 hover:underline cursor-pointer"
                      >
                        {t('prompt.resetPrompt')}
                      </button>
                    )}
                  </div>
                  {detailLoading ? (
                    <div
                      className="w-full px-3 py-2.5 min-h-[200px] rounded-xl border theme-border theme-bg-main space-y-2.5"
                      aria-busy="true"
                      aria-label={t('prompt.loading')}
                    >
                      <div className="h-3 rounded theme-bg-tag animate-pulse w-3/4" />
                      <div className="h-3 rounded theme-bg-tag animate-pulse w-full" />
                      <div className="h-3 rounded theme-bg-tag animate-pulse w-5/6" />
                      <div className="h-3 rounded theme-bg-tag animate-pulse w-2/3" />
                      <div className="h-3 rounded theme-bg-tag animate-pulse w-full" />
                      <div className="h-3 rounded theme-bg-tag animate-pulse w-4/5" />
                      <div className="h-3 rounded theme-bg-tag animate-pulse w-1/2" />
                    </div>
                  ) : (
                    <textarea
                      value={detailDraft}
                      onChange={(e) => setDetailDraft(e.target.value)}
                      rows={12}
                      className="w-full px-3 py-2.5 text-[13px] leading-relaxed theme-bg-main border theme-border rounded-xl theme-text-main font-mono focus:outline-none focus:border-violet-500 resize-y min-h-[200px]"
                    />
                  )}
                  {extractVariables(detailDraft).length > 0 && (
                    <p className="text-[11px] theme-text-sub">
                      {t('prompt.varsDetected')}
                      {extractVariables(detailDraft).map((v) => (
                        <code key={v} className="mx-1 px-1 py-0.5 rounded theme-bg-tag">
                          {`{{${v}}}`}
                        </code>
                      ))}
                      {t('prompt.varsReplace')}
                    </p>
                  )}
                </section>

                {detailPrompt.notes && (
                  <section className="space-y-1">
                    <h3 className="text-xs font-semibold theme-text-muted uppercase tracking-wide">
                      {t('prompt.savedNotes')}
                    </h3>
                    <p className="text-sm theme-text-main leading-relaxed">{detailPrompt.notes}</p>
                  </section>
                )}

                {detailPrompt.origin?.startsWith('catalog:') && (
                  <p className="text-[10px] theme-text-sub pt-1">{t('prompt.attribution')}</p>
                )}
              </div>
            </div>
          </div>
        </div>
      )}

      {dragActive && (
        <div className="fixed inset-0 z-[70] pointer-events-none flex items-center justify-center bg-violet-500/10 backdrop-blur-[2px]">
          <div className="flex flex-col items-center gap-3 px-10 py-8 rounded-2xl border-2 border-dashed border-violet-500/70 bg-white/85 dark:bg-slate-900/85 shadow-2xl">
            <ImageIcon className="h-10 w-10 text-violet-500" />
            <div className="text-sm font-semibold theme-text-main">{t('prompt.dropToImport')}</div>
            <div className="text-[11px] theme-text-muted">{t('prompt.dropToImportHint')}</div>
          </div>
        </div>
      )}

      {editorOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/50 backdrop-blur-sm">
          <div className="w-full max-w-2xl max-h-[90vh] overflow-hidden flex flex-col theme-bg-card border theme-border rounded-2xl shadow-2xl">
            <div className="px-5 py-4 border-b theme-border flex items-center justify-between">
              <h3 className="text-sm font-bold theme-text-main">
                {editingId ? t('prompt.edit') : t('prompt.create')}
              </h3>
              <button
                onClick={() => setEditorOpen(false)}
                className="p-1 rounded-lg theme-text-muted hover:theme-text-main cursor-pointer"
              >
                <X className="h-4 w-4" />
              </button>
            </div>

            <div className="flex-1 overflow-y-auto p-5 space-y-4">
              <div>
                <label className="text-[11px] font-medium theme-text-muted mb-1 block">{t('prompt.fieldTitle')}</label>
                <input
                  value={form.title}
                  onChange={(e) => setForm((f) => ({ ...f, title: e.target.value }))}
                  placeholder={t('prompt.titlePh')}
                  className="w-full px-3 py-2 text-sm theme-bg-input border theme-border rounded-lg theme-text-main focus:outline-none focus:border-violet-500"
                />
              </div>

              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="text-[11px] font-medium theme-text-muted mb-1 block">{t('prompt.category')}</label>
                  <CustomSelect
                    value={form.category}
                    options={editorCategories}
                    onChange={(v) => setForm((f) => ({ ...f, category: v as string }))}
                    className="w-full"
                  />
                </div>
                <div>
                  <label className="text-[11px] font-medium theme-text-muted mb-1 block">{t('prompt.tags')}</label>
                  <input
                    value={tagsInput}
                    onChange={(e) => setTagsInput(e.target.value)}
                    placeholder={t('prompt.tagsPh')}
                    className="w-full px-3 py-2 text-sm theme-bg-input border theme-border rounded-lg theme-text-main focus:outline-none focus:border-violet-500"
                  />
                </div>
              </div>

              {formIsMedia && (
                <div>
                  <label className="text-[11px] font-medium theme-text-muted mb-1 block">
                    {t('prompt.previewUrl')}
                  </label>
                  <div className="flex gap-2">
                    <input
                      value={form.preview_url || ''}
                      onChange={(e) => setForm((f) => ({ ...f, preview_url: e.target.value }))}
                      placeholder={t('prompt.previewUrlPh')}
                      className="flex-1 min-w-0 px-3 py-2 text-sm theme-bg-input border theme-border rounded-lg theme-text-main focus:outline-none focus:border-violet-500"
                    />
                    <button
                      type="button"
                      onClick={handlePickImage}
                      disabled={importingImage}
                      className="shrink-0 inline-flex items-center gap-1.5 px-3 py-2 text-xs font-medium theme-bg-sub hover:theme-bg-tag theme-text-muted hover:theme-text-main border theme-border rounded-lg transition-colors cursor-pointer disabled:opacity-50"
                    >
                      <Upload className={`h-3.5 w-3.5 ${importingImage ? 'animate-pulse' : ''}`} />
                      <span>{importingImage ? t('prompt.importingImage') : t('prompt.uploadImage')}</span>
                    </button>
                  </div>
                  <p className="text-[10px] theme-text-muted mt-1.5">{t('prompt.dragHint')}</p>
                  {form.preview_local ? (
                    <div className="mt-2 flex items-center gap-2.5 p-2 theme-bg-sub border theme-border rounded-lg">
                      <img
                        src={`${LOCAL_MEDIA_BASE}${form.preview_local}`}
                        alt={t('prompt.localPreview')}
                        className="h-11 w-11 rounded-md object-cover border theme-border shrink-0"
                      />
                      <div className="flex-1 min-w-0">
                        <div className="text-[11px] font-medium theme-text-main truncate">
                          {t('prompt.localPreview')}
                        </div>
                        <div className="text-[10px] theme-text-muted truncate font-mono">
                          {form.preview_local}
                        </div>
                      </div>
                      <button
                        type="button"
                        onClick={() => setForm((f) => ({ ...f, preview_local: '' }))}
                        title={t('prompt.removeLocalPreview')}
                        className="p-1.5 rounded-md theme-text-muted hover:text-red-500 hover:bg-red-500/10 transition-colors cursor-pointer shrink-0"
                      >
                        <X className="h-3.5 w-3.5" />
                      </button>
                    </div>
                  ) : null}
                </div>
              )}

              <div>
                <label className="text-[11px] font-medium theme-text-muted mb-1 block">
                  {t('prompt.body')}
                </label>
                <textarea
                  value={form.content}
                  onChange={(e) => setForm((f) => ({ ...f, content: e.target.value }))}
                  placeholder={t('prompt.contentPh')}
                  rows={10}
                  className="w-full px-3 py-2 text-sm theme-bg-input border theme-border rounded-lg theme-text-main font-mono leading-relaxed focus:outline-none focus:border-violet-500 resize-y"
                />
              </div>

              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="text-[11px] font-medium theme-text-muted mb-1 block">
                    {t('prompt.sourceNote')}
                  </label>
                  <input
                    value={form.source_note || ''}
                    onChange={(e) => setForm((f) => ({ ...f, source_note: e.target.value }))}
                    placeholder={t('prompt.sourcePh')}
                    className="w-full px-3 py-2 text-sm theme-bg-input border theme-border rounded-lg theme-text-main focus:outline-none focus:border-violet-500"
                  />
                </div>
                <div>
                  <label className="text-[11px] font-medium theme-text-muted mb-1 block">
                    {t('prompt.url')}
                  </label>
                  <input
                    value={form.source_url || ''}
                    onChange={(e) => setForm((f) => ({ ...f, source_url: e.target.value }))}
                    placeholder="https://…"
                    className="w-full px-3 py-2 text-sm theme-bg-input border theme-border rounded-lg theme-text-main focus:outline-none focus:border-violet-500"
                  />
                </div>
              </div>

              <div>
                <label className="text-[11px] font-medium theme-text-muted mb-1 block">
                  {t('prompt.notesOptional')}
                </label>
                <textarea
                  value={form.notes || ''}
                  onChange={(e) => setForm((f) => ({ ...f, notes: e.target.value }))}
                  placeholder={t('prompt.notesPh')}
                  rows={3}
                  className="w-full px-3 py-2 text-sm theme-bg-input border theme-border rounded-lg theme-text-main focus:outline-none focus:border-violet-500 resize-y"
                />
              </div>

              <label className="flex items-center gap-2 text-xs theme-text-muted cursor-pointer">
                <input
                  type="checkbox"
                  checked={form.is_starred}
                  onChange={(e) => setForm((f) => ({ ...f, is_starred: e.target.checked }))}
                  className="rounded border theme-border"
                />
                {t('prompt.addStar')}
              </label>
            </div>

            <div className="px-5 py-4 border-t theme-border flex justify-end gap-2">
              <button
                onClick={() => setEditorOpen(false)}
                className="px-4 py-2 text-xs rounded-lg border theme-border theme-text-muted hover:theme-text-main cursor-pointer"
              >
                {t('prompt.cancel')}
              </button>
              <button
                onClick={handleSave}
                disabled={saving}
                className="flex items-center gap-1.5 px-4 py-2 text-xs rounded-lg bg-violet-600 hover:bg-violet-500 text-white font-medium disabled:opacity-60 cursor-pointer"
              >
                <Check className="h-3.5 w-3.5" />
                {saving ? t('prompt.saving') : t('prompt.save')}
              </button>
            </div>
          </div>
        </div>
      )}

      {variableModalOpen && variablePrompt && (
        <div className="fixed inset-0 z-[60] flex items-center justify-center p-4 bg-black/50 backdrop-blur-sm">
          <div className="w-full max-w-md theme-bg-card border theme-border rounded-2xl shadow-2xl p-5 space-y-4">
            <h3 className="text-sm font-bold theme-text-main">{t('prompt.fillVars')}</h3>
            {extractVariables(variablePrompt.content).map((v) => (
              <div key={v}>
                <label className="text-[11px] theme-text-muted mb-1 block">{`{{${v}}}`}</label>
                <input
                  value={variableValues[v] || ''}
                  onChange={(e) =>
                    setVariableValues((prev) => ({ ...prev, [v]: e.target.value }))
                  }
                  className="w-full px-3 py-2 text-sm theme-bg-input border theme-border rounded-lg theme-text-main focus:outline-none focus:border-violet-500"
                />
              </div>
            ))}
            <div className="flex justify-end gap-2 pt-2">
              <button
                onClick={() => {
                  setVariableModalOpen(false);
                  setVariablePrompt(null);
                }}
                className="px-3 py-1.5 text-xs rounded-lg border theme-border theme-text-muted cursor-pointer"
              >
                {t('prompt.cancel')}
              </button>
              <button
                onClick={confirmVariableCopy}
                className="px-3 py-1.5 text-xs rounded-lg bg-violet-600 text-white cursor-pointer"
              >
                {t('prompt.copy')}
              </button>
            </div>
          </div>
        </div>
      )}

      {copyToast && (
        <div className="fixed bottom-6 left-1/2 -translate-x-1/2 z-50 px-4 py-2 text-xs font-medium rounded-xl bg-slate-900/90 text-white border border-white/10 shadow-xl">
          {copyToast}
        </div>
      )}
    </div>
  );
};
