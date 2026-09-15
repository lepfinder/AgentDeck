import { api } from '../api/tauriBridge';
import { AI_PROVIDERS } from '../config/aiProviders';

export interface ServiceLlmEndpoint {
  provider_name: string;
  base_url: string;
  api_key: string;
  model: string;
}

function readJsonRecord(key: string): Record<string, string> {
  try {
    const raw = localStorage.getItem(key);
    return raw ? (JSON.parse(raw) as Record<string, string>) : {};
  } catch {
    return {};
  }
}

function endpointFor(providerId: string): ServiceLlmEndpoint | null {
  const provider = AI_PROVIDERS.find((p) => p.id === providerId);
  if (!provider) return null;
  const apiKeys = readJsonRecord('agentdeck_ai_api_keys');
  const baseUrls = readJsonRecord('agentdeck_ai_base_urls');
  const models = readJsonRecord('agentdeck_ai_models');
  const apiKey = apiKeys[providerId] || '';
  if (!apiKey.trim()) return null;
  return {
    provider_name: provider.name,
    base_url: baseUrls[providerId] || provider.baseUrl,
    api_key: apiKey,
    model: models[providerId] || provider.defaultModel,
  };
}

/** Read AgentDeck settings (localStorage) — same source as SettingsModal. */
export function getServiceLlmConfig(): {
  primary: ServiceLlmEndpoint;
  fallback?: ServiceLlmEndpoint;
} | null {
  const primaryId = localStorage.getItem('agentdeck_primary_ai_provider') || 'bailian';
  const primary = endpointFor(primaryId);
  if (!primary) return null;

  const autoFallback = localStorage.getItem('agentdeck_auto_fallback') !== 'false';
  const fallbackId = localStorage.getItem('agentdeck_fallback_ai_provider') || 'deepseek';
  const fallback =
    autoFallback && fallbackId !== primaryId ? endpointFor(fallbackId) || undefined : undefined;

  return { primary, fallback };
}

export async function chatCompletionForService(messages: Array<{ role: string; content: string }>) {
  const cfg = getServiceLlmConfig();
  if (!cfg) {
    throw new Error('未配置 AI，请先在设置中填写 API Key');
  }
  const result = await api.callLlmWithFallback(
    cfg.primary,
    cfg.fallback,
    messages,
    1600,
    true,
    'service_registration'
  );
  if (!result.success || !result.content) {
    throw new Error(result.error || 'AI 返回为空');
  }
  return result.content;
}
