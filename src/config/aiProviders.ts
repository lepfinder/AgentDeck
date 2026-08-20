export interface AiProviderConfig {
  id: string;
  name: string;
  iconName: 'Flame' | 'Bot' | 'Sparkles' | 'Zap' | 'Globe';
  baseUrl: string;
  defaultModel: string;
  models: string[];
  apiKeyLink: string;
  apiKeyPlaceholder: string;
}

export const AI_PROVIDERS: AiProviderConfig[] = [
  {
    id: 'bailian',
    name: '通义千问（百炼）',
    iconName: 'Flame',
    baseUrl: 'https://token-plan.cn-beijing.maas.aliyuncs.com/compatible-mode/v1',
    defaultModel: 'qwen3.8-max',
    models: ['qwen3.8-max', 'qwen3.6-flash', 'kimi-k2.7-code', 'kimi-k2.6', 'deepseek-v4-pro'],
    apiKeyLink: 'https://bailian.console.aliyun.com/cn-beijing?tab=plan#/efm/subscription/token-plan/enterprise',
    apiKeyPlaceholder: 'sk-...',
  },
  {
    id: 'deepseek',
    name: 'DeepSeek',
    iconName: 'Bot',
    baseUrl: 'https://api.deepseek.com',
    defaultModel: 'deepseek-v4-flash',
    models: ['deepseek-v4-flash', 'deepseek-v4-pro', 'deepseek-chat', 'deepseek-reasoner'],
    apiKeyLink: 'https://platform.deepseek.com/api_keys',
    apiKeyPlaceholder: 'sk-...',
  },
  {
    id: 'volcano_ark_coding_plan',
    name: '火山方舟 Coding Plan',
    iconName: 'Sparkles',
    baseUrl: 'https://ark.cn-beijing.volces.com/api/coding/v3',
    defaultModel: 'deepseek-v4-flash',
    models: ['deepseek-v4-flash', 'doubao-seed-2.0-mini', 'doubao-seed-evolving', 'minimax-m3', 'kimi-k2.7-code', 'kimi-k3'],
    apiKeyLink: 'https://console.volcengine.com/ark/region:cn-beijing/subscription/coding-plan?projectName=default',
    apiKeyPlaceholder: 'sk-...',
  },
  {
    id: 'zhipu',
    name: '智谱 GLM',
    iconName: 'Zap',
    baseUrl: 'https://open.bigmodel.cn/api/paas/v4',
    defaultModel: 'glm-4-plus',
    models: ['glm-4-plus', 'glm-4-air', 'glm-4-flash', 'codegeex-4'],
    apiKeyLink: 'https://open.bigmodel.cn/usercenter/apikeys',
    apiKeyPlaceholder: 'API Key...',
  },
  {
    id: 'minimax',
    name: 'MiniMax',
    iconName: 'Bot',
    baseUrl: 'https://api.minimax.chat/v1',
    defaultModel: 'MiniMax-Text-01',
    models: ['MiniMax-Text-01', 'abab6.5s-chat'],
    apiKeyLink: 'https://platform.minimaxi.com/user-center/basic-information/interface-key',
    apiKeyPlaceholder: 'sk-...',
  },
  {
    id: 'gemini',
    name: 'Google Gemini',
    iconName: 'Sparkles',
    baseUrl: 'https://generativelanguage.googleapis.com/v1beta/openai',
    defaultModel: 'gemini-2.0-flash',
    models: ['gemini-2.0-flash', 'gemini-2.0-pro-exp-02-05', 'gemini-1.5-pro', 'gemini-1.5-flash'],
    apiKeyLink: 'https://aistudio.google.com/app/apikey',
    apiKeyPlaceholder: 'AIzaSy...',
  },
  {
    id: 'custom',
    name: '自定义 OpenAI 兼容接口',
    iconName: 'Globe',
    baseUrl: 'http://localhost:11434/v1',
    defaultModel: 'llama3.3',
    models: ['llama3.3', 'qwen2.5-coder:32b', 'deepseek-r1:14b'],
    apiKeyLink: '',
    apiKeyPlaceholder: 'sk-...（本地 Ollama 可选留空）',
  },
];
