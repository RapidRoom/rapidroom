import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';
import { sameAdjustmentValue } from '../../utils/editHistory';
import type { UserPreset } from '../../hooks/usePresets';
import { useEditorStore } from '../../store/useEditorStore';
import { useUIStore } from '../../store/useUIStore';
import { useSettingsStore } from '../../store/useSettingsStore';
import { debouncedSetHistory, useEditorActions } from '../../hooks/useEditorActions';
import { APP_ACTION_DEFINITIONS, executeAppAction, isAppActionAvailable } from '../../utils/appActions';
import { getDefaultCombo } from '../../utils/keyboardUtils';
import {
  PALETTE_PARAMETERS,
  getParameterValue,
  parseTypedValue,
  searchScore,
  setParameterValue,
  type PaletteParameter,
} from '../../utils/commandPalette';
import type { Adjustments } from '../../utils/adjustments';
import { Invokes, type Preset } from './AppProperties';
import { calculateCenteredCrop, forceCropInBounds, getOrientedDimensions } from '../../utils/cropUtils';

interface Entry {
  id: string;
  title: string;
  terms: string[];
  section: string;
  shortcut?: string;
  available: boolean;
  parameter?: PaletteParameter;
  value?: number;
  execute?(): void;
  preview?(base: Adjustments): Adjustments;
}

export default function CommandPalette() {
  const { t } = useTranslation();
  const editor = useEditorStore();
  const ui = useUIStore();
  const { appSettings, osPlatform } = useSettingsStore();
  const { setAdjustments } = useEditorActions();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [selected, setSelected] = useState(0);
  const [parameter, setParameter] = useState<PaletteParameter | null>(null);
  const [value, setValue] = useState(0);
  const [presets, setPresets] = useState<Preset[]>([]);
  const input = useRef<HTMLInputElement>(null);
  const previousFocus = useRef<HTMLElement | null>(null);
  const returnQuery = useRef('');
  const preview = useRef<Adjustments | null>(null);
  const base = useRef<{
    path: string;
    adjustments: Adjustments;
    override: Adjustments | null;
    showOriginal: boolean;
  } | null>(null);
  const canEdit = ui.activeView === 'editor' && !!editor.selectedImage?.isReady;
  const translate = useCallback((key: string) => String(t(key as never)), [t]);

  const revert = useCallback(() => {
    const state = useEditorStore.getState();
    if (preview.current && state.previewOverride === preview.current) {
      const original = base.current;
      state.setEditor({ previewOverride: original?.override ?? null, showOriginal: original?.showOriginal ?? false });
    }
    preview.current = null;
  }, []);
  const close = useCallback(() => {
    revert();
    setOpen(false);
    setParameter(null);
    setQuery('');
    previousFocus.current?.focus();
  }, [revert]);

  useEffect(() => {
    const keydown = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.code === 'KeyK') {
        event.preventDefault();
        event.stopImmediatePropagation();
        if (open) {
          close();
          return;
        }
        const ui = useUIStore.getState();
        if (
          ui.isSettingsOpen ||
          Object.entries(ui).some(
            ([key, value]) =>
              (key.endsWith('ModalOpen') && value === true) ||
              (key.endsWith('ModalState') && value && typeof value === 'object' && 'isOpen' in value && value.isOpen),
          )
        )
          return;
        const state = useEditorStore.getState();
        // Flush a prior slider gesture before the palette starts its own history step.
        debouncedSetHistory.flush();
        previousFocus.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
        base.current = state.selectedImage
          ? {
              path: state.selectedImage.path,
              adjustments: state.adjustments,
              override: state.previewOverride,
              showOriginal: state.showOriginal,
            }
          : null;
        setOpen(true);
        setQuery('');
        setSelected(0);
      }
    };
    window.addEventListener('keydown', keydown, true);
    return () => window.removeEventListener('keydown', keydown, true);
  }, [open, close]);

  useEffect(() => {
    if (!open) return;
    input.current?.focus();
    const original = base.current;
    if (original && (editor.selectedImage?.path !== original.path || editor.adjustments !== original.adjustments))
      close();
  }, [open, editor.selectedImage?.path, editor.adjustments, close, parameter]);
  useEffect(() => () => revert(), [revert]);

  useEffect(() => {
    if (!open) return;
    let current = true;
    invoke<UserPreset[]>(Invokes.LoadPresets)
      .then((items) => {
        if (current) setPresets(items.flatMap((item) => (item.preset ? [item.preset] : (item.folder?.children ?? []))));
      })
      .catch(() => {
        if (current) setPresets([]);
      });
    return () => {
      current = false;
    };
  }, [open]);

  const commit = useCallback(
    (next: Adjustments) => {
      const state = useEditorStore.getState();
      const original = base.current;
      if (!original || state.selectedImage?.path !== original.path || state.adjustments !== original.adjustments) {
        close();
        return;
      }
      revert();
      if (!sameAdjustmentValue(next, state.adjustments)) {
        state.setEditor({ showOriginal: false, previewOverride: null });
        setAdjustments(next);
        debouncedSetHistory.flush();
      }
      setOpen(false);
      setParameter(null);
      setQuery('');
      previousFocus.current?.focus();
    },
    [close, revert, setAdjustments],
  );

  const entries = useMemo<Entry[]>(() => {
    const controls: Entry[] = PALETTE_PARAMETERS.map((parameter) => ({
      id: parameter.id,
      title: parameter.labelKey ? translate(parameter.labelKey) : parameter.title,
      terms: [parameter.id, parameter.section, ...parameter.aliases],
      section: parameter.section,
      available: canEdit,
      parameter,
    }));
    const actions: Entry[] = APP_ACTION_DEFINITIONS.map((definition) => ({
      id: definition.action,
      title: translate(definition.description),
      section: translate(`settings.keybinds.sections.${definition.section}`),
      terms: [definition.action, definition.action === 'toggle_export' ? 'export recipe' : ''],
      shortcut: (appSettings?.keybinds?.[definition.action] ?? getDefaultCombo(definition, osPlatform)).join('+'),
      available: isAppActionAvailable(definition.action),
      execute: () => {
        close();
        executeAppAction(definition.action);
      },
    }));
    const choices: Entry[] = [
      ...(['basic', 'agx'] as const).map((toneMapper) => ({
        id: `toneMapper:${toneMapper}`,
        title: translate(`adjustments.basic.mappers.${toneMapper}`),
        terms: ['tone mapper treatment'],
        section: translate('adjustments.basic.toneMapper'),
        available: canEdit,
        preview: (base: Adjustments) => ({ ...base, toneMapper }),
        execute: () => commit({ ...editor.adjustments, toneMapper }),
      })),
      ...presets.map((preset) => ({
        id: `apply_preset:${preset.id}`,
        title: preset.name,
        terms: ['preset recipe style look'],
        section: translate('editor.presets.title'),
        available: canEdit,
        preview: (base: Adjustments) => ({ ...base, ...preset.adjustments }),
        execute: () => commit({ ...editor.adjustments, ...preset.adjustments }),
      })),
    ];
    const typed = parseTypedValue(query);
    const typedEntries: Entry[] = typed
      ? controls
          .map((entry) => ({ entry, score: searchScore(typed.name, entry.title, entry.terms) }))
          .filter(({ score }) => score >= 40)
          .sort((a, b) => b.score - a.score)
          .map(({ entry }) => entry)
          .map((entry) => {
            const p = entry.parameter!;
            const requested = typed.value + (typed.relative ? getParameterValue(editor.adjustments, p.id) : 0);
            const value = Math.min(p.maximum, Math.max(p.minimum, requested));
            return {
              ...entry,
              id: `set:${entry.id}`,
              title: t('commandPalette.setValue', { name: entry.title, value }),
              value,
            };
          })
      : [];
    const crop = /^crop\s+(\d+(?:\.\d+)?)\s*:\s*(\d+(?:\.\d+)?)$/i.exec(query.trim());
    const crops: Entry[] =
      crop && Number(crop[1]) > 0 && Number(crop[2]) > 0
        ? [
            {
              id: 'crop',
              title: t('commandPalette.cropRatio', { ratio: `${crop[1]}:${crop[2]}` }),
              section: translate('editor.crop.title'),
              terms: [],
              available: canEdit,
              execute: () => {
                const state = useEditorStore.getState();
                const photo = state.selectedImage;
                if (!photo) return;
                const aspectRatio = Number(crop[1]) / Number(crop[2]);
                const orientation = state.adjustments.orientationSteps || 0;
                const rotation = state.adjustments.rotation || 0;
                const { width, height } = getOrientedDimensions(photo.width, photo.height, orientation);
                const centered = calculateCenteredCrop(photo.width, photo.height, orientation, aspectRatio, rotation);
                commit({
                  ...state.adjustments,
                  aspectRatio,
                  crop: centered ? forceCropInBounds(centered, width, height, rotation) : null,
                });
              },
            },
          ]
        : [];
    if (crops.length || typedEntries.length) return [...crops, ...typedEntries];
    return [...choices, ...controls, ...actions]
      .map((entry) => ({ entry, score: searchScore(query, entry.title, entry.terms) }))
      .filter(({ score }) => score > 0)
      .sort((a, b) => b.score - a.score)
      .map(({ entry }) => entry);
  }, [query, translate, canEdit, appSettings?.keybinds, osPlatform, editor, ui, close, commit, presets, t]);

  useEffect(() => setSelected(0), [query]);
  const highlighted = entries[Math.min(selected, entries.length - 1)];
  useEffect(() => {
    if (open && highlighted)
      document.getElementById(`palette-${highlighted.id}`)?.scrollIntoView?.({ block: 'nearest' });
  }, [open, highlighted?.id]);
  useEffect(() => {
    if (!open) return;
    const original = base.current;
    const p = parameter ?? highlighted?.parameter;
    const nextValue = parameter ? value : highlighted?.value;
    if (!original || !canEdit || (!highlighted?.preview && (!p || nextValue === undefined))) {
      revert();
      return;
    }
    const timer = window.setTimeout(() => {
      const state = useEditorStore.getState();
      if (state.selectedImage?.path !== original.path || state.adjustments !== original.adjustments) return;
      const next =
        !parameter && highlighted?.preview
          ? highlighted.preview(original.adjustments)
          : setParameterValue(original.adjustments, p!, nextValue!);
      preview.current = next;
      state.setEditor({ previewOverride: next, showOriginal: false });
    }, 150);
    return () => clearTimeout(timer);
  }, [open, parameter, value, highlighted?.id, highlighted?.value, canEdit, revert]);

  const choose = (entry: Entry | undefined) => {
    if (!entry?.available) return;
    if (entry.parameter) {
      if (entry.value !== undefined) commit(setParameterValue(editor.adjustments, entry.parameter, entry.value));
      else {
        returnQuery.current = query;
        setParameter(entry.parameter);
        setValue(getParameterValue(editor.adjustments, entry.parameter.id));
        setQuery('');
      }
    } else entry.execute?.();
  };

  if (!open) return null;
  return (
    <div
      className="fixed inset-0 z-[100]"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) close();
      }}
    >
      <div
        role="dialog"
        aria-label={t('commandPalette.title')}
        aria-modal="true"
        className="absolute top-[12%] left-1/2 -translate-x-1/2 w-[min(620px,90vw)] bg-bg-secondary border border-border-color rounded-lg shadow-2xl text-text-primary"
        onKeyDown={(event) => {
          event.stopPropagation();
          if (event.key === 'Tab') {
            event.preventDefault();
            input.current?.focus();
            return;
          }
          if (event.key === 'Escape') {
            event.preventDefault();
            if (parameter) {
              revert();
              setParameter(null);
              setQuery(returnQuery.current);
            } else close();
          } else if (event.key === 'Enter') {
            event.preventDefault();
            if (parameter) {
              const typed = parseTypedValue(`value ${query}`);
              const next = typed ? typed.value + (typed.relative ? value : 0) : value;
              commit(setParameterValue(editor.adjustments, parameter, next));
            } else choose(highlighted);
          } else if (parameter && ['ArrowLeft', 'ArrowRight'].includes(event.key)) {
            event.preventDefault();
            const step = parameter.step * (event.shiftKey ? 10 : event.altKey ? 0.1 : 1);
            setValue(
              Math.min(
                parameter.maximum,
                Math.max(parameter.minimum, value + (event.key === 'ArrowRight' ? step : -step)),
              ),
            );
          } else if (!parameter && ['ArrowDown', 'ArrowUp'].includes(event.key)) {
            event.preventDefault();
            setSelected((index) =>
              Math.max(0, Math.min(entries.length - 1, index + (event.key === 'ArrowDown' ? 1 : -1))),
            );
          }
        }}
      >
        {parameter && (
          <div className="px-4 pt-3 text-sm">
            {parameter.labelKey ? translate(parameter.labelKey) : parameter.title}
            <span className="float-right tabular-nums">{Number(value.toFixed(4))}</span>
          </div>
        )}
        <input
          ref={input}
          role="combobox"
          aria-label={t('commandPalette.search')}
          aria-controls="command-palette-results"
          aria-expanded={!parameter}
          aria-activedescendant={!parameter && highlighted ? `palette-${highlighted.id}` : undefined}
          autoComplete="off"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder={parameter ? t('commandPalette.valueHint') : t('commandPalette.searchHint')}
          className="w-full p-4 bg-transparent outline-none text-sm"
        />
        {!parameter && (
          <div
            role="listbox"
            id="command-palette-results"
            className="max-h-[50vh] overflow-y-auto border-t border-border-color"
          >
            {entries.length === 0 && (
              <div className="p-4 text-sm text-text-secondary">{t('commandPalette.noResults')}</div>
            )}
            {entries.map((entry, index) => (
              <button
                type="button"
                role="option"
                aria-selected={index === selected}
                aria-disabled={!entry.available}
                tabIndex={-1}
                id={`palette-${entry.id}`}
                key={entry.id}
                onMouseEnter={() => setSelected(index)}
                onClick={() => choose(entry)}
                className={`w-full text-left px-4 py-2 flex items-center gap-3 text-sm ${index === selected ? 'bg-surface' : ''} ${entry.available ? '' : 'opacity-40'}`}
              >
                <span className="flex-1">
                  {entry.title}
                  <span className="block text-xs text-text-secondary">{entry.section}</span>
                </span>
                <span className="text-xs text-text-secondary">
                  {entry.parameter
                    ? Number(getParameterValue(editor.adjustments, entry.parameter.id).toFixed(4))
                    : entry.shortcut}
                </span>
              </button>
            ))}
          </div>
        )}
        <div className="px-4 py-2 text-xs text-text-secondary border-t border-border-color">
          {parameter ? t('commandPalette.sliderKeys') : t('commandPalette.listKeys')}
        </div>
      </div>
    </div>
  );
}
