import React, { useState, useRef, useEffect } from 'react';
import { ChevronDown, Check } from 'lucide-react';
import { useI18n } from '../../i18n';

export interface SelectOption<T = string | number> {
  value: T;
  label: string;
  subLabel?: string;
  icon?: React.ReactNode;
}

interface CustomSelectProps<T = string | number> {
  value: T;
  options: SelectOption<T>[];
  onChange: (value: T) => void;
  placeholder?: string;
  className?: string;
  triggerClassName?: string;
  menuClassName?: string;
  disabled?: boolean;
}

export function CustomSelect<T extends string | number>({
  value,
  options,
  onChange,
  placeholder,
  className = '',
  triggerClassName = '',
  menuClassName = '',
  disabled = false,
}: CustomSelectProps<T>) {
  const { t } = useI18n();
  const resolvedPlaceholder = placeholder ?? t('settings.select');
  const [isOpen, setIsOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  const selectedOption = options.find((opt) => opt.value === value);

  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(event.target as Node)) {
        setIsOpen(false);
      }
    };

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setIsOpen(false);
      }
    };

    if (isOpen) {
      document.addEventListener('mousedown', handleClickOutside);
      document.addEventListener('keydown', handleKeyDown);
    }
    return () => {
      document.removeEventListener('mousedown', handleClickOutside);
      document.removeEventListener('keydown', handleKeyDown);
    };
  }, [isOpen]);

  return (
    <div ref={containerRef} className={`relative inline-block ${className}`}>
      <button
        type="button"
        disabled={disabled}
        onClick={() => setIsOpen((prev) => !prev)}
        className={`w-full flex items-center justify-between gap-2.5 px-3 py-1.5 text-xs font-medium theme-bg-card border theme-border rounded-lg theme-text-main shadow-xs hover:border-blue-500/50 focus:outline-none focus:ring-1 focus:ring-blue-500/50 transition-all cursor-pointer select-none disabled:opacity-50 disabled:cursor-not-allowed ${
          isOpen ? 'border-blue-500 ring-1 ring-blue-500/30' : ''
        } ${triggerClassName}`}
      >
        <span className="flex items-center gap-2 truncate">
          {selectedOption?.icon}
          <span className="truncate">
            {selectedOption ? selectedOption.label : resolvedPlaceholder}
          </span>
        </span>
        <ChevronDown
          className={`h-3.5 w-3.5 theme-text-muted transition-transform duration-200 flex-shrink-0 ${
            isOpen ? 'rotate-180 text-blue-500' : ''
          }`}
        />
      </button>

      {isOpen && (
        <div
          className={`absolute left-0 top-full z-50 mt-1 min-w-full w-max max-h-60 overflow-y-auto rounded-xl p-1 shadow-xl border theme-border theme-bg-card backdrop-blur-md animate-in fade-in-0 zoom-in-95 duration-100 scrollbar-thin ${menuClassName}`}
        >
          {options.map((opt) => {
            const isSelected = opt.value === value;
            return (
              <button
                key={String(opt.value)}
                type="button"
                onClick={() => {
                  onChange(opt.value);
                  setIsOpen(false);
                }}
                className={`w-full flex items-center justify-between gap-3 px-2.5 py-1.5 rounded-lg text-xs font-medium text-left transition-colors cursor-pointer select-none ${
                  isSelected
                    ? 'bg-blue-600/10 text-blue-600 dark:text-blue-400 font-semibold'
                    : 'theme-text-main hover:theme-bg-sub'
                }`}
              >
                <div className="flex items-center gap-2 truncate">
                  {opt.icon}
                  <span className="truncate">{opt.label}</span>
                  {opt.subLabel && (
                    <span className="text-[10px] theme-text-sub font-normal truncate">
                      {opt.subLabel}
                    </span>
                  )}
                </div>
                {isSelected && (
                  <Check className="h-3.5 w-3.5 text-blue-500 flex-shrink-0 ml-1.5" />
                )}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
