import React, { useCallback, useEffect, useMemo, useState } from 'react';
import type { PromptCategory, PromptInput, PromptItem } from '../../types';
import { api } from '../../api/tauriBridge';
import { CustomSelect } from '../common/CustomSelect';
import { useI18n, type MessageKey } from '../../i18n';
import {
  BookMarked,
  Copy,
  ExternalLink,
  Pencil,
  Plus,
  Search,
  Star,
  Trash2,
  X,
  Check,
} from 'lucide-react';

const EMPTY_FORM: PromptInput = {
  title: '',
  content: '',
  category: 'coding',
  tags: [],
  source_url: '',
  source_note: '',
  notes: '',
  is_starred: false,
};

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

interface Props {
  onPromptCountChange?: (count: number) => void;
}

const CATEGORY_KEYS: Record<string, MessageKey> = {
  '': 'prompt.cat.all',
  coding: 'prompt.cat.coding',
  research: 'prompt.cat.research',
  writing: 'prompt.cat.writing',
  product: 'prompt.cat.product',
  agent: 'prompt.cat.agent',
  image: 'prompt.cat.image',
  video: 'prompt.cat.video',
  persona: 'prompt.cat.persona',
  meta: 'prompt.cat.meta',
};

export const PromptLibraryView: React.FC<Props> = ({ onPromptCountChange }) => {
  const { t } = useI18n();
  const categories = useMemo(
    () =>
      (['', 'coding', 'research', 'writing', 'product', 'agent', 'image', 'video', 'persona', 'meta'] as const).map(
        (value) => ({ value, label: t(CATEGORY_KEYS[value]) })
      ),
    [t]
  );
  const [prompts, setPrompts] = useState<PromptItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [search, setSearch] = useState('');
  const [category, setCategory] = useState<PromptCategory | ''>('');
  const [starredOnly, setStarredOnly] = useState(false);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [editorOpen, setEditorOpen] = useState(false);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [form, setForm] = useState<PromptInput>(EMPTY_FORM);
  const [tagsInput, setTagsInput] = useState('');
  const [copyToast, setCopyToast] = useState<string | null>(null);
  const [variableModalOpen, setVariableModalOpen] = useState(false);
  const [variableValues, setVariableValues] = useState<Record<string, string>>({});
  const [saving, setSaving] = useState(false);

  const selectedPrompt = useMemo(
    () => prompts.find((p) => p.id === selectedId) || null,
    [prompts, selectedId]
  );

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
        starredOnly
      );
      setPrompts(list);
      await refreshTotalCount();
      setSelectedId((prev) => {
        if (prev && list.some((p) => p.id === prev)) return prev;
        return list[0]?.id ?? null;
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

  const showToast = (msg: string) => {
    setCopyToast(msg);
    setTimeout(() => setCopyToast(null), 2200);
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
      };
      if (editingId) {
        const updated = await api.updatePrompt(editingId, payload);
        setPrompts((prev) => prev.map((p) => (p.id === updated.id ? updated : p)));
        setSelectedId(updated.id);
      } else {
        const created = await api.createPrompt(payload);
        setPrompts((prev) => [created, ...prev]);
        setSelectedId(created.id);
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

  const handleDelete = async (id: number) => {
    if (!window.confirm(t('prompt.confirmDelete'))) return;
    try {
      await api.deletePrompt(id);
      const next = prompts.filter((p) => p.id !== id);
      setPrompts(next);
      await refreshTotalCount();
      setSelectedId(next[0]?.id ?? null);
      showToast(t('prompt.deleted'));
    } catch (e) {
      console.error(e);
      showToast(t('prompt.deleteFailed'));
    }
  };

  const handleToggleStar = async (prompt: PromptItem) => {
    try {
      const starred = await api.togglePromptStar(prompt.id);
      setPrompts((prev) =>
        prev.map((p) => (p.id === prompt.id ? { ...p, is_starred: starred } : p))
      );
    } catch (e) {
      console.error(e);
    }
  };

  const copyPromptContent = async (prompt: PromptItem, content: string) => {
    try {
      await navigator.clipboard.writeText(content);
      await api.recordPromptUse(prompt.id);
      setPrompts((prev) =>
        prev.map((p) =>
          p.id === prompt.id
            ? { ...p, use_count: p.use_count + 1, last_used_at: new Date().toISOString() }
            : p
        )
      );
      showToast(t('prompt.copied'));
    } catch (e) {
      console.error(e);
      showToast(t('prompt.copyFailed'));
    }
  };

  const handleCopy = async (prompt: PromptItem) => {
    const vars = extractVariables(prompt.content);
    if (vars.length > 0) {
      const initial: Record<string, string> = {};
      vars.forEach((v) => {
        initial[v] = variableValues[v] || '';
      });
      setVariableValues(initial);
      setVariableModalOpen(true);
      return;
    }
    await copyPromptContent(prompt, prompt.content);
  };

  const confirmVariableCopy = async () => {
    if (!selectedPrompt) return;
    const content = applyVariables(selectedPrompt.content, variableValues);
    setVariableModalOpen(false);
    await copyPromptContent(selectedPrompt, content);
  };

  return (
    <div className="flex h-full w-full overflow-hidden theme-bg-main">
      {/* 左侧列表 */}
      <section className="w-96 border-r theme-border flex flex-col theme-bg-sub flex-shrink-0">
        <div className="p-4 border-b theme-border space-y-3">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <BookMarked className="h-4 w-4 text-violet-500" />
              <h1 className="text-sm font-bold theme-text-main">{t('prompt.title')}</h1>
            </div>
            <button
              onClick={openCreate}
              className="flex items-center gap-1 px-2.5 py-1.5 text-xs font-medium rounded-lg bg-violet-600 hover:bg-violet-500 text-white transition-colors cursor-pointer"
            >
              <Plus className="h-3.5 w-3.5" />
              {t('prompt.new')}
            </button>
          </div>

          <div className="relative">
            <Search className="absolute left-2.5 top-2.5 h-3.5 w-3.5 theme-text-sub" />
            <input
              type="text"
              placeholder={t('prompt.search')}
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              className="w-full pl-8 pr-3 py-1.5 text-xs theme-bg-input border theme-border rounded-lg theme-text-main placeholder-slate-400 focus:outline-none focus:border-violet-500"
            />
          </div>

          <div className="flex items-center gap-2">
            <CustomSelect
              value={category}
              options={categories}
              onChange={(v) => setCategory(v as PromptCategory | '')}
              className="flex-1"
              triggerClassName="py-1.5"
            />
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
          </div>
        </div>

        <div className="flex-1 overflow-y-auto p-2 space-y-1.5">
          {loading ? (
            <div className="py-12 text-center text-xs theme-text-sub">{t('prompt.loading')}</div>
          ) : prompts.length === 0 ? (
            <div className="py-12 px-4 text-center space-y-2">
              <p className="text-xs theme-text-muted">{t('prompt.empty')}</p>
              <p className="text-[11px] theme-text-sub leading-relaxed">
                {t('prompt.emptyHint')}
              </p>
              <button
                onClick={openCreate}
                className="mt-2 text-xs text-violet-500 hover:underline cursor-pointer"
              >
                {t('prompt.addFirst')}
              </button>
            </div>
          ) : (
            prompts.map((prompt) => {
              const active = prompt.id === selectedId;
              return (
                <button
                  key={prompt.id}
                  onClick={() => setSelectedId(prompt.id)}
                  className={`w-full text-left p-3 rounded-xl border text-xs transition-all cursor-pointer ${
                    active
                      ? 'bg-violet-600/15 border-violet-500/50 theme-text-main shadow-xs'
                      : 'theme-bg-card border-transparent hover:theme-border theme-text-muted hover:theme-text-main'
                  }`}
                >
                  <div className="flex items-start justify-between gap-2">
                    <span className="font-semibold line-clamp-2 theme-text-main">{prompt.title}</span>
                    {prompt.is_starred && (
                      <Star className="h-3.5 w-3.5 fill-amber-400 text-amber-400 flex-shrink-0" />
                    )}
                  </div>
                  <p className="mt-1 line-clamp-2 theme-text-sub text-[11px] leading-relaxed">
                    {prompt.content}
                  </p>
                  <div className="flex items-center gap-2 mt-2 flex-wrap">
                    <span className="px-1.5 py-0.5 rounded bg-violet-500/10 text-violet-500 text-[10px]">
                      {t(CATEGORY_KEYS[prompt.category] || 'prompt.cat.coding')}
                    </span>
                    {prompt.tags.slice(0, 2).map((tag) => (
                      <span
                        key={tag}
                        className="px-1.5 py-0.5 rounded theme-bg-tag theme-text-sub text-[10px]"
                      >
                        {tag}
                      </span>
                    ))}
                    <span className="ml-auto text-[10px] theme-text-sub">
                      {t('prompt.usedShort', { n: prompt.use_count })}
                    </span>
                  </div>
                </button>
              );
            })
          )}
        </div>
      </section>

      {/* 右侧详情 */}
      <main className="flex-1 flex flex-col overflow-hidden">
        {selectedPrompt ? (
          <>
            <div className="p-4 border-b theme-border theme-bg-header backdrop-blur-sm flex items-start justify-between gap-4">
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2 flex-wrap mb-1">
                  <span className="px-2 py-0.5 rounded-lg bg-violet-500/10 text-violet-500 text-[11px] font-medium">
                    {t(CATEGORY_KEYS[selectedPrompt.category] || 'prompt.cat.coding')}
                  </span>
                  {selectedPrompt.tags.map((tag) => (
                    <span
                      key={tag}
                      className="px-2 py-0.5 rounded-lg theme-bg-tag theme-text-muted text-[10px]"
                    >
                      {tag}
                    </span>
                  ))}
                </div>
                <h2 className="text-lg font-bold theme-text-main truncate">{selectedPrompt.title}</h2>
                <div className="flex items-center gap-3 mt-1 text-[11px] theme-text-sub flex-wrap">
                  <span>{t('prompt.used', { n: selectedPrompt.use_count })}</span>
                  {selectedPrompt.source_note && <span>{t('prompt.sourcePrefix', { note: selectedPrompt.source_note })}</span>}
                  {selectedPrompt.source_url && (
                    <button
                      onClick={() => api.openUrl(selectedPrompt.source_url!)}
                      className="inline-flex items-center gap-1 text-blue-500 hover:underline cursor-pointer"
                    >
                      <ExternalLink className="h-3 w-3" />
                      {t('prompt.openLink')}
                    </button>
                  )}
                </div>
              </div>

              <div className="flex items-center gap-2 flex-shrink-0">
                <button
                  onClick={() => handleCopy(selectedPrompt)}
                  className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-violet-600 hover:bg-violet-500 text-white text-xs font-medium transition-colors cursor-pointer"
                >
                  <Copy className="h-3.5 w-3.5" />
                  {t('prompt.copy')}
                </button>
                <button
                  onClick={() => handleToggleStar(selectedPrompt)}
                  className={`p-2 rounded-lg border transition-colors cursor-pointer ${
                    selectedPrompt.is_starred
                      ? 'bg-amber-500/15 border-amber-500/40 text-amber-500'
                      : 'theme-bg-sub theme-border theme-text-muted'
                  }`}
                >
                  <Star
                    className={`h-4 w-4 ${selectedPrompt.is_starred ? 'fill-amber-400 text-amber-400' : ''}`}
                  />
                </button>
                <button
                  onClick={() => openEdit(selectedPrompt)}
                  className="p-2 rounded-lg border theme-border theme-bg-sub theme-text-muted hover:theme-text-main transition-colors cursor-pointer"
                >
                  <Pencil className="h-4 w-4" />
                </button>
                <button
                  onClick={() => handleDelete(selectedPrompt.id)}
                  className="p-2 rounded-lg border theme-border theme-bg-sub text-red-400 hover:bg-red-500/10 transition-colors cursor-pointer"
                >
                  <Trash2 className="h-4 w-4" />
                </button>
              </div>
            </div>

            <div className="flex-1 overflow-y-auto p-6 space-y-5">
              <section>
                <h3 className="text-xs font-semibold theme-text-muted mb-2 uppercase tracking-wide">
                  {t('prompt.body')}
                </h3>
                <pre className="whitespace-pre-wrap break-words text-sm leading-relaxed theme-text-main theme-bg-card border theme-border rounded-xl p-4 font-mono">
                  {selectedPrompt.content}
                </pre>
              </section>

              {selectedPrompt.notes && (
                <section>
                  <h3 className="text-xs font-semibold theme-text-muted mb-2 uppercase tracking-wide">
                    {t('prompt.savedNotes')}
                  </h3>
                  <p className="text-sm theme-text-main leading-relaxed">{selectedPrompt.notes}</p>
                </section>
              )}

              {extractVariables(selectedPrompt.content).length > 0 && (
                <section className="text-[11px] theme-text-sub">
                  {t('prompt.varsDetected')}
                  {extractVariables(selectedPrompt.content).map((v) => (
                    <code key={v} className="mx-1 px-1 py-0.5 rounded theme-bg-tag">
                      {`{{${v}}}`}
                    </code>
                  ))}
                  {t('prompt.varsReplace')}
                </section>
              )}
            </div>
          </>
        ) : (
          <div className="flex h-full flex-col items-center justify-center theme-text-sub px-8 text-center">
            <BookMarked className="h-10 w-10 text-violet-500/40 mb-3" />
            <p className="text-sm theme-text-muted">{t('prompt.pickOne')}</p>
            <p className="text-xs mt-1 max-w-sm leading-relaxed">
              {t('prompt.pickHint')}
            </p>
          </div>
        )}
      </main>

      {/* 编辑弹窗 */}
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
                    options={categories.filter((c) => c.value !== '')}
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

      {/* 变量填写弹窗 */}
      {variableModalOpen && selectedPrompt && (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/50 backdrop-blur-sm">
          <div className="w-full max-w-md theme-bg-card border theme-border rounded-2xl shadow-2xl p-5 space-y-4">
            <h3 className="text-sm font-bold theme-text-main">{t('prompt.fillVars')}</h3>
            {extractVariables(selectedPrompt.content).map((v) => (
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
                onClick={() => setVariableModalOpen(false)}
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
