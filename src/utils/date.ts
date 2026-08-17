/**
 * 统一时间格式化工具库（默认按中国/北京时间 UTC+8 / Asia/Shanghai 转换与展示）
 */

export const formatBeijingTime = (timeStr?: string, includeSeconds = false): string => {
  if (!timeStr) return '';
  try {
    let str = timeStr.trim();
    if (/^\d{10,13}$/.test(str)) {
      const num = Number(str);
      str = new Date(str.length === 10 ? num * 1000 : num).toISOString();
    }
    const d = new Date(str);
    if (!isNaN(d.getTime())) {
      const parts = new Intl.DateTimeFormat('zh-CN', {
        timeZone: 'Asia/Shanghai',
        year: 'numeric',
        month: '2-digit',
        day: '2-digit',
        hour: '2-digit',
        minute: '2-digit',
        second: includeSeconds ? '2-digit' : undefined,
        hour12: false,
      }).formatToParts(d);
      const p = Object.fromEntries(parts.map((it) => [it.type, it.value]));
      if (includeSeconds) {
        return `${p.year}-${p.month}-${p.day} ${p.hour}:${p.minute}:${p.second}`;
      }
      return `${p.year}-${p.month}-${p.day} ${p.hour}:${p.minute}`;
    }
  } catch {
    // fallback
  }
  if (timeStr.length >= 16) {
    return timeStr.substring(0, 16).replace('T', ' ');
  }
  return timeStr;
};

export const formatRelativeTime = (timeStr?: string): string => {
  if (!timeStr) return '';
  try {
    let str = timeStr.trim();
    if (/^\d{10,13}$/.test(str)) {
      const num = Number(str);
      str = new Date(str.length === 10 ? num * 1000 : num).toISOString();
    }
    const d = new Date(str);
    if (isNaN(d.getTime())) return timeStr;
    const now = new Date();
    const diffSec = Math.floor((now.getTime() - d.getTime()) / 1000);
    if (diffSec < 0 || diffSec < 60) return '刚刚';
    const diffMin = Math.floor(diffSec / 60);
    if (diffMin < 60) return `${diffMin} 分钟前`;
    const diffHour = Math.floor(diffMin / 60);
    if (diffHour < 24) return `${diffHour} 小时前`;
    const diffDay = Math.floor(diffHour / 24);
    if (diffDay < 30) return `${diffDay} 天前`;
    const diffMonth = Math.floor(diffDay / 30);
    if (diffMonth < 12) return `${diffMonth} 个月前`;
    return `${Math.floor(diffDay / 365)} 年前`;
  } catch {
    return timeStr;
  }
};
